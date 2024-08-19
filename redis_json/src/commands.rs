/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use crate::error::Error;
use crate::formatter::ReplyFormatOptions;
use crate::key_value::KeyValue;
use crate::manager::{
    err_msg_json_path_doesnt_exist_with_param, err_msg_json_path_doesnt_exist_with_param_or,
    Manager, ReadHolder, UpdateInfo, WriteHolder,
};
use crate::redisjson::{Format, Path, ReplyFormat, ResultInto, SetOptions, JSON_ROOT_PATH};
use json_path::select_value::{SelectValue, SelectValueType};
use redis_module::{Context, RedisValue};
use redis_module::{NextArg, RedisError, RedisResult, RedisString, REDIS_OK};
use std::cmp::Ordering;
use std::str::FromStr;

use json_path::{calc_once_with_paths, compile, json_path::UserPathTracker};

use serde_json::{Number, Value};

use itertools::FoldWhile::{Continue, Done};
use itertools::{EitherOrBoth, Itertools};
use serde::{Serialize, Serializer};

const CMD_ARG_NOESCAPE: &str = "NOESCAPE";
const CMD_ARG_INDENT: &str = "INDENT";
const CMD_ARG_NEWLINE: &str = "NEWLINE";
const CMD_ARG_SPACE: &str = "SPACE";
const CMD_ARG_FORMAT: &str = "FORMAT";

// Compile time evaluation of the max len() of all elements of the array
const fn max_strlen(arr: &[&str]) -> usize {
    let mut max_strlen = 0;
    let arr_len = arr.len();
    if arr_len < 1 {
        return max_strlen;
    }
    let mut pos = 0;
    while pos < arr_len {
        let curr_strlen = arr[pos].len();
        if max_strlen < curr_strlen {
            max_strlen = curr_strlen;
        }
        pos += 1;
    }
    max_strlen
}

// We use this constant to further optimize json_get command, by calculating the max subcommand length
// Any subcommand added to JSON.GET should be included on the following array
const JSONGET_SUBCOMMANDS_MAXSTRLEN: usize = max_strlen(&[
    CMD_ARG_NOESCAPE,
    CMD_ARG_INDENT,
    CMD_ARG_NEWLINE,
    CMD_ARG_SPACE,
    CMD_ARG_FORMAT,
]);

pub enum Values<'a, V: SelectValue> {
    Single(&'a V),
    Multi(Vec<&'a V>),
}

impl<'a, V: SelectValue> Serialize for Values<'a, V> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Values::Single(v) => v.serialize(serializer),
            Values::Multi(v) => v.serialize(serializer),
        }
    }
}

fn is_resp3(ctx: &Context) -> bool {
    ctx.get_flags()
        .contains(redis_module::ContextFlags::FLAGS_RESP3)
}

///
/// JSON.GET <key>
///         [INDENT indentation-string]
///         [NEWLINE line-break-string]
///         [SPACE space-string]
///         [FORMAT {STRING|EXPAND1|EXPAND}]      /* default is STRING */
///         [path ...]
///
pub fn json_get<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;

    // Set Capacity to 1 assuming the common case has one path
    let mut paths = Vec::with_capacity(1);

    let mut format_options = ReplyFormatOptions::new(is_resp3(ctx), ReplyFormat::STRING);

    while let Ok(arg) = args.next_str() {
        match arg {
            // fast way to consider arg a path by using the max length of all possible subcommands
            // See #390 for the comparison of this function with/without this optimization
            arg if arg.len() > JSONGET_SUBCOMMANDS_MAXSTRLEN => paths.push(Path::new(arg)),
            arg if arg.eq_ignore_ascii_case(CMD_ARG_FORMAT) => {
                if !format_options.resp3 && paths.is_empty() {
                    return Err(RedisError::Str(
                        "ERR FORMAT argument is not supported on RESP2",
                    ));
                }
                // Temporary fix until STRINGS is also supported
                let next = args.next_str()?;
                if next.eq_ignore_ascii_case("STRINGS") {
                    return Err(RedisError::Str("ERR wrong reply format"));
                }
                format_options.format = ReplyFormat::from_str(next)?;
            }
            arg if arg.eq_ignore_ascii_case(CMD_ARG_INDENT) => {
                format_options.indent = Some(args.next_str()?)
            }
            arg if arg.eq_ignore_ascii_case(CMD_ARG_NEWLINE) => {
                format_options.newline = Some(args.next_str()?)
            }
            arg if arg.eq_ignore_ascii_case(CMD_ARG_SPACE) => {
                format_options.space = Some(args.next_str()?)
            }
            // Silently ignore. Compatibility with ReJSON v1.0 which has this option. See #168 TODO add support
            arg if arg.eq_ignore_ascii_case(CMD_ARG_NOESCAPE) => continue,
            _ => paths.push(Path::new(arg)),
        };
    }

    // path is optional -> no path found we use legacy root "."
    if paths.is_empty() {
        paths.push(Path::default());
    }

    let key = manager.open_key_read(ctx, &key)?;
    let value = match key.get_value()? {
        Some(doc) => KeyValue::new(doc).to_json(paths, format_options)?,
        None => RedisValue::Null,
    };

    Ok(value)
}

///
/// JSON.SET <key> <path> <json> [NX | XX | FORMAT <format>]
///
pub fn json_set<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let value = args.next_str()?;

    let mut format = Format::JSON;
    let mut set_option = SetOptions::None;

    while let Some(s) = args.next() {
        match s.try_as_str()? {
            arg if arg.eq_ignore_ascii_case("NX") && set_option == SetOptions::None => {
                set_option = SetOptions::NotExists
            }
            arg if arg.eq_ignore_ascii_case("XX") && set_option == SetOptions::None => {
                set_option = SetOptions::AlreadyExists
            }
            arg if arg.eq_ignore_ascii_case("FORMAT") => {
                format = Format::from_str(args.next_str()?)?;
            }
            _ => return Err(RedisError::Str("ERR syntax error")),
        };
    }

    let mut redis_key = manager.open_key_write(ctx, key)?;
    let current = redis_key.get_value()?;

    let val = manager.from_str(value, format, true)?;

    match (current, set_option) {
        (Some(doc), op) => {
            if path == *JSON_ROOT_PATH {
                if op != SetOptions::NotExists {
                    redis_key.set_value(Vec::new(), val)?;
                    redis_key.notify_keyspace_event(ctx, "json.set")?;
                    manager.apply_changes(ctx);
                    REDIS_OK
                } else {
                    Ok(RedisValue::Null)
                }
            } else {
                let update_info = KeyValue::new(doc).find_paths(path.get_path(), op)?;
                if !update_info.is_empty() && apply_updates::<M>(&mut redis_key, val, update_info) {
                    redis_key.notify_keyspace_event(ctx, "json.set")?;
                    manager.apply_changes(ctx);
                    REDIS_OK
                } else {
                    Ok(RedisValue::Null)
                }
            }
        }
        (None, SetOptions::AlreadyExists) => Ok(RedisValue::Null),
        _ => {
            if path == *JSON_ROOT_PATH {
                redis_key.set_value(Vec::new(), val)?;
                redis_key.notify_keyspace_event(ctx, "json.set")?;
                manager.apply_changes(ctx);
                REDIS_OK
            } else {
                Err(RedisError::Str(
                    "ERR new objects must be created at the root",
                ))
            }
        }
    }
}

///
/// JSON.MERGE <key> <path> <json> [FORMAT <format>]
///
pub fn json_merge<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let value = args.next_str()?;

    let mut format = Format::JSON;

    while let Some(s) = args.next() {
        match s.try_as_str()? {
            arg if arg.eq_ignore_ascii_case("FORMAT") => {
                format = Format::from_str(args.next_str()?)?;
            }
            _ => return Err(RedisError::Str("ERR syntax error")),
        };
    }

    let mut redis_key = manager.open_key_write(ctx, key)?;
    let current = redis_key.get_value()?;

    let val = manager.from_str(value, format, true)?;

    match current {
        Some(doc) => {
            if path == *JSON_ROOT_PATH {
                redis_key.merge_value(Vec::new(), val)?;
                redis_key.notify_keyspace_event(ctx, "json.merge")?;
                manager.apply_changes(ctx);
                REDIS_OK
            } else {
                let update_info =
                    KeyValue::new(doc).find_paths(path.get_path(), SetOptions::MergeExisting)?;
                if !update_info.is_empty() {
                    let res = if update_info.len() == 1 {
                        match update_info.into_iter().next().unwrap() {
                            UpdateInfo::SUI(sui) => redis_key.merge_value(sui.path, val),
                            UpdateInfo::AUI(aui) => redis_key.dict_add(aui.path, &aui.key, val),
                        }
                    } else {
                        update_info
                            .into_iter()
                            .try_fold(false, |res, ui| -> RedisResult<_> {
                                match ui {
                                    UpdateInfo::SUI(sui) => {
                                        redis_key.merge_value(sui.path, val.clone())
                                    }
                                    UpdateInfo::AUI(aui) => {
                                        redis_key.dict_add(aui.path, &aui.key, val.clone())
                                    }
                                }
                                .and_then(|updated| Ok(updated || res)) // If any of the updates succeed, return true
                            })
                    }?;
                    if res {
                        redis_key.notify_keyspace_event(ctx, "json.merge")?;
                        manager.apply_changes(ctx);
                        REDIS_OK
                    } else {
                        Ok(RedisValue::Null)
                    }
                } else {
                    Ok(RedisValue::Null)
                }
            }
        }
        None => {
            if path == *JSON_ROOT_PATH {
                // Nothing to merge with it's a new doc
                redis_key.set_value(Vec::new(), val)?;
                redis_key.notify_keyspace_event(ctx, "json.merge")?;
                manager.apply_changes(ctx);
                REDIS_OK
            } else {
                Err(RedisError::Str(
                    "ERR new objects must be created at the root",
                ))
            }
        }
    }
}

///
/// JSON.MSET <key> <path> <json> [[<key> <path> <json>]...]
///
pub fn json_mset<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() == 1 || (args.len() - 1) % 3 != 0 {
        return Err(RedisError::WrongArity);
    }

    // Collect all the actions from the args (redis_key, update_info, value)
    let actions: Vec<_> = args[1..]
        .chunks_exact(3)
        .map(|args| -> RedisResult<_> {
            let [key, path, value] = args else {
                unreachable!();
            };
            let mut redis_key = manager.open_key_write(ctx, key.safe_clone(ctx))?;

            // Verify the path is valid and get all the update info
            let update_info = path.try_as_str().map(Path::new).and_then(|path| {
                if path == *JSON_ROOT_PATH {
                    Ok(None)
                } else {
                    // Verify the key is a JSON type
                    redis_key.get_value()?.map_or_else(
                        || {
                            Err(RedisError::Str(
                                "ERR new objects must be created at the root",
                            ))
                        },
                        |value| {
                            KeyValue::new(value)
                                .find_paths(path.get_path(), SetOptions::None)
                                .into_both()
                        },
                    )
                }
            })?;

            // Parse the input and validate it's valid JSON
            let value = value
                .try_as_str()
                .and_then(|value| manager.from_str(value, Format::JSON, true).into_both())?;

            Ok((redis_key, update_info, value))
        })
        .try_collect()?;

    actions
        .into_iter()
        .try_for_each(|(mut redis_key, update_info, value)| {
            let updated = if let Some(update_info) = update_info {
                !update_info.is_empty() && apply_updates::<M>(&mut redis_key, value, update_info)
            } else {
                // In case it is a root path
                redis_key.set_value(Vec::new(), value)?
            };
            if updated {
                redis_key.notify_keyspace_event(ctx, "json.mset")?
            }
            Ok(())
        })
        .and_then(|_| {
            manager.apply_changes(ctx);
            REDIS_OK
        })
}

fn apply_updates<M>(redis_key: &mut M::WriteHolder, value: M::O, ui: Vec<UpdateInfo>) -> bool
where
    M: Manager,
{
    // If there is only one update info, we can avoid cloning the value
    if ui.len() == 1 {
        match ui.into_iter().next().unwrap() {
            UpdateInfo::SUI(sui) => redis_key.set_value(sui.path, value),
            UpdateInfo::AUI(aui) => redis_key.dict_add(aui.path, &aui.key, value),
        }
        .unwrap_or(false)
    } else {
        ui.into_iter().fold(false, |updated, ui| {
            match ui {
                UpdateInfo::SUI(sui) => redis_key.set_value(sui.path, value.clone()),
                UpdateInfo::AUI(aui) => redis_key.dict_add(aui.path, &aui.key, value.clone()),
            }
            .unwrap_or(updated)
        })
    }
}

fn find_paths<T, F>(path: &str, doc: &T, mut f: F) -> RedisResult<Vec<Vec<String>>>
where
    T: SelectValue,
    F: FnMut(&T) -> bool,
{
    let query = compile(path).map_err(|e| RedisError::String(e.to_string()))?;
    let res = calc_once_with_paths(query, doc)
        .into_iter()
        .filter_map(|e| {
            f(e.res)
                .then(|| e.path_tracker.map(UserPathTracker::to_string_path))
                .flatten()
        })
        .collect();
    Ok(res)
}

/// Returns tuples of Value and its concrete path which match the given `path`
fn get_all_values_and_paths<'a, T: SelectValue>(
    path: &str,
    doc: &'a T,
) -> RedisResult<Vec<(&'a T, Vec<String>)>> {
    let query = compile(path).map_err(|e| RedisError::String(e.to_string()))?;
    let res = calc_once_with_paths(query, doc)
        .into_iter()
        .map(|e| (e.res, e.path_tracker.unwrap().to_string_path()))
        .collect();
    Ok(res)
}

/// Returns a Vec of paths with `None` for Values that do not match the filter
fn filter_paths<T, F>(values_and_paths: Vec<(&T, Vec<String>)>, f: F) -> Vec<Option<Vec<String>>>
where
    F: Fn(&T) -> bool,
{
    values_and_paths
        .into_iter()
        .map(|(v, p)| f(v).then_some(p))
        .collect()
}

/// Returns a Vec of Values with `None` for Values that do not match the filter
fn filter_values<T, F>(values_and_paths: Vec<(&T, Vec<String>)>, f: F) -> Vec<Option<&T>>
where
    F: Fn(&T) -> bool,
{
    values_and_paths
        .into_iter()
        .map(|(v, _)| f(v).then_some(v))
        .collect()
}

fn find_all_paths<T: SelectValue, F>(
    path: &str,
    doc: &T,
    f: F,
) -> RedisResult<Vec<Option<Vec<String>>>>
where
    F: Fn(&T) -> bool,
{
    let res = get_all_values_and_paths(path, doc)?;
    Ok(filter_paths(res, f))
}

fn find_all_values<'a, T: SelectValue, F>(
    path: &str,
    doc: &'a T,
    f: F,
) -> RedisResult<Vec<Option<&'a T>>>
where
    F: Fn(&T) -> bool,
{
    let res = get_all_values_and_paths(path, doc)?;
    Ok(filter_values(res, f))
}

fn to_json_value<T>(values: Vec<Option<T>>, none_value: Value) -> Vec<Value>
where
    Value: From<T>,
{
    values
        .into_iter()
        .map(|n| n.map_or_else(|| none_value.clone(), |t| t.into()))
        .collect()
}

/// Sort the paths so higher indices precede lower indices on the same array,
/// And longer paths precede shorter paths
/// And if a path is a sub-path of the other, then only paths with shallower hierarchy (closer to the top-level) remain
pub fn prepare_paths_for_updating(mut paths: Vec<Vec<String>>) -> Vec<Vec<String>> {
    if paths.len() < 2 {
        // No need to reorder when there are less than 2 paths
        return paths;
    }
    paths.sort_by(|v1, v2| {
        v1.iter()
            .zip_longest(v2.iter())
            .fold_while(Ordering::Equal, |_acc, v| {
                match v {
                    EitherOrBoth::Left(_) => Done(Ordering::Less), // Shorter paths after longer paths
                    EitherOrBoth::Right(_) => Done(Ordering::Greater), // Shorter paths after longer paths
                    EitherOrBoth::Both(p1, p2) => {
                        let i1 = p1.parse::<usize>();
                        let i2 = p2.parse::<usize>();
                        match (i1, i2) {
                            (Err(_), Err(_)) => match p1.cmp(p2) {
                                // String compare
                                Ordering::Less => Done(Ordering::Less),
                                Ordering::Equal => Continue(Ordering::Equal),
                                Ordering::Greater => Done(Ordering::Greater),
                            },
                            (Ok(_), Err(_)) => Done(Ordering::Greater), //String before Numeric
                            (Err(_), Ok(_)) => Done(Ordering::Less),    //String before Numeric
                            (Ok(i1), Ok(i2)) => {
                                // Numeric compare - higher indices before lower ones
                                match i2.cmp(&i1) {
                                    Ordering::Greater => Done(Ordering::Greater),
                                    Ordering::Less => Done(Ordering::Less),
                                    Ordering::Equal => Continue(Ordering::Equal),
                                }
                            }
                        }
                    }
                }
            })
            .into_inner()
    });
    // Remove paths which are nested by others (on each sub-tree only top most ancestor should be deleted)
    // (TODO: Add a mode in which the jsonpath selector will already skip nested paths)
    let string_paths = paths.iter().map(|v| v.join(",")).sorted().collect_vec();

    paths.retain(|v| {
        let path = v.join(",");
        string_paths
            .iter()
            .skip_while(|p| !path.starts_with(*p))
            .next()
            .map(|found| path == *found)
            .unwrap_or(false)
    });
    paths
}

///
/// JSON.DEL <key> [path]
///
pub fn json_del<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = match args.next() {
        Some(s) => Path::new(s.try_as_str()?),
        _ => Path::default(),
    };

    let mut redis_key = manager.open_key_write(ctx, key)?;
    let deleted = if let Some(doc) = redis_key.get_value()? {
        let res = if path == *JSON_ROOT_PATH {
            redis_key.delete()?;
            1
        } else {
            let paths =
                find_paths(path.get_path(), doc, |_| true).map(prepare_paths_for_updating)?;
            paths.into_iter().try_fold(0i64, |acc, p| {
                redis_key
                    .delete_path(p)
                    .map(|deleted| acc + if deleted { 1 } else { 0 })
            })?
        };
        if res > 0 {
            redis_key.notify_keyspace_event(ctx, "json.del")?;
            manager.apply_changes(ctx);
        }
        res
    } else {
        0
    };
    Ok(deleted.into())
}

///
/// JSON.MGET <key> [key ...] <path>
///
pub fn json_mget<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() < 3 {
        return Err(RedisError::WrongArity);
    }

    args.last().ok_or(RedisError::WrongArity).and_then(|path| {
        let path = Path::new(path.try_as_str()?);
        let keys = &args[1..args.len() - 1];

        // Verify that at least one key exists
        if keys.is_empty() {
            return Err(RedisError::WrongArity);
        }

        let results = keys
            .into_iter()
            .map(|key| {
                manager
                    .open_key_read(ctx, key)
                    .map_or(RedisValue::Null, |json_key| {
                        json_key
                            .get_value()
                            .ok()
                            .flatten()
                            .map_or(RedisValue::Null, |doc| {
                                let key_value = KeyValue::new(doc);
                                let format_options =
                                    ReplyFormatOptions::new(is_resp3(ctx), ReplyFormat::STRING);
                                if !path.is_legacy() {
                                    key_value.to_string_multi(path.get_path(), format_options)
                                } else {
                                    key_value.to_string_single(path.get_path(), format_options)
                                }
                                .map_or(RedisValue::Null, |v| v.into())
                            })
                    })
            })
            .collect_vec();

        Ok(results.into())
    })
}
///
/// JSON.TYPE <key> [path]
///
pub fn json_type<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = args.next_str().map(Path::new).unwrap_or_default();

    let key = manager.open_key_read(ctx, &key)?;

    let value = if path.is_legacy() {
        json_type_legacy::<M>(&key, path.get_path())?
    } else {
        json_type_impl::<M>(&key, path.get_path())?
    };

    // Check context flags to see if RESP3 is enabled and return the appropriate result
    if is_resp3(ctx) {
        Ok(vec![value].into())
    } else {
        Ok(value)
    }
}

fn json_type_impl<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    redis_key.get_value()?.map_or(Ok(RedisValue::Null), |root| {
        let value = KeyValue::new(root)
            .get_values(path)?
            .into_iter()
            .map(|v| RedisValue::from(KeyValue::value_name(v)))
            .collect_vec()
            .into();
        Ok(value)
    })
}

fn json_type_legacy<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let value = redis_key
        .get_value()?
        .map(|doc| KeyValue::new(doc).get_type(path).map(|s| s.into()).ok())
        .flatten()
        .unwrap_or(RedisValue::Null);
    Ok(value)
}

enum NumOp {
    Incr,
    Mult,
    Pow,
}

fn json_num_op<M>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
    cmd: &str,
    op: NumOp,
) -> RedisResult
where
    M: Manager,
{
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let number = args.next_str()?;

    let redis_key = manager.open_key_write(ctx, key)?;

    // check context flags to see if RESP3 is enabled
    if is_resp3(ctx) {
        let res = json_num_op_impl::<M>(manager, redis_key, ctx, path.get_path(), number, op, cmd)?
            .into_iter()
            .map(|v| {
                v.map_or(RedisValue::Null, |v| {
                    if let Some(i) = v.as_i64() {
                        RedisValue::Integer(i)
                    } else {
                        RedisValue::Float(v.as_f64().unwrap_or_default())
                    }
                })
            })
            .collect_vec()
            .into();
        Ok(res)
    } else if path.is_legacy() {
        json_num_op_legacy::<M>(manager, redis_key, ctx, path.get_path(), number, op, cmd)
    } else {
        let results =
            json_num_op_impl::<M>(manager, redis_key, ctx, path.get_path(), number, op, cmd)?;

        // Convert to RESP2 format return as one JSON array
        let values = to_json_value::<Number>(results, Value::Null);
        Ok(KeyValue::<M::V>::serialize_object(&values, ReplyFormatOptions::default()).into())
    }
}

fn json_num_op_impl<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
    number: &str,
    op: NumOp,
    cmd: &str,
) -> RedisResult<Vec<Option<Number>>>
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let paths = find_all_paths(path, root, |v| {
        matches!(
            v.get_type(),
            SelectValueType::Double | SelectValueType::Long
        )
    })?;

    let mut need_notify = false;
    let res = paths
        .into_iter()
        .map(|p| {
            p.map(|p| {
                need_notify = true;
                match op {
                    NumOp::Incr => redis_key.incr_by(p, number),
                    NumOp::Mult => redis_key.mult_by(p, number),
                    NumOp::Pow => redis_key.pow_by(p, number),
                }
            })
            .transpose()
        })
        .try_collect()?;
    if need_notify {
        redis_key.notify_keyspace_event(ctx, cmd)?;
        manager.apply_changes(ctx);
    }
    Ok(res)
}

fn json_num_op_legacy<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
    number: &str,
    op: NumOp,
    cmd: &str,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let paths = find_paths(path, root, |v| {
        matches!(
            v.get_type(),
            SelectValueType::Double | SelectValueType::Long,
        )
    })?;
    let res = paths
        .into_iter()
        .try_fold(None, |_, p| {
            match op {
                NumOp::Incr => redis_key.incr_by(p, number),
                NumOp::Mult => redis_key.mult_by(p, number),
                NumOp::Pow => redis_key.pow_by(p, number),
            }
            .into_both()
        })
        .transpose()
        .unwrap_or_else(|| {
            Err(RedisError::String(
                err_msg_json_path_doesnt_exist_with_param_or(path, "does not contains a number"),
            ))
        })?;
    redis_key.notify_keyspace_event(ctx, cmd)?;
    manager.apply_changes(ctx);
    Ok(res.to_string().into())
}

///
/// JSON.NUMINCRBY <key> <path> <number>
///
pub fn json_num_incrby<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    json_num_op(manager, ctx, args, "json.numincrby", NumOp::Incr)
}

///
/// JSON.NUMMULTBY <key> <path> <number>
///
pub fn json_num_multby<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    json_num_op(manager, ctx, args, "json.nummultby", NumOp::Mult)
}

///
/// JSON.NUMPOWBY <key> <path> <number>
///
pub fn json_num_powby<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    json_num_op(manager, ctx, args, "json.numpowby", NumOp::Pow)
}

//
/// JSON.TOGGLE <key> <path>
///
pub fn json_bool_toggle<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = args.next_str().map(Path::new)?;
    let redis_key = manager.open_key_write(ctx, key)?;

    if path.is_legacy() {
        json_bool_toggle_legacy::<M>(manager, redis_key, ctx, path.get_path())
    } else {
        json_bool_toggle_impl::<M>(manager, redis_key, ctx, path.get_path())
    }
}

fn json_bool_toggle_impl<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Bool)?;
    let mut need_notify = false;
    let res: Vec<_> = paths
        .into_iter()
        .map(|p| {
            p.map_or(Ok(RedisValue::Null), |p| -> RedisResult {
                need_notify = true;
                redis_key.bool_toggle(p).map(Into::into)
            })
        })
        .try_collect()?;
    if need_notify {
        redis_key.notify_keyspace_event(ctx, "json.toggle")?;
        manager.apply_changes(ctx);
    }
    Ok(res.into())
}

fn json_bool_toggle_legacy<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let paths = find_paths(path, root, |v| v.get_type() == SelectValueType::Bool)?;
    if !paths.is_empty() {
        let mut res = false;
        for p in paths {
            res = redis_key.bool_toggle(p)?;
        }
        redis_key.notify_keyspace_event(ctx, "json.toggle")?;
        manager.apply_changes(ctx);
        Ok(res.to_string().into())
    } else {
        Err(RedisError::String(
            err_msg_json_path_doesnt_exist_with_param_or(path, "not a bool"),
        ))
    }
}

///
/// JSON.STRAPPEND <key> [path] <json-string>
///
pub fn json_str_append<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path_or_json = args.next_str()?;

    let path;
    let json;

    // path is optional
    if let Ok(val) = args.next_arg() {
        path = Path::new(path_or_json);
        json = val.try_as_str()?;
    } else {
        path = Path::default();
        json = path_or_json;
    }

    let redis_key = manager.open_key_write(ctx, key)?;

    if path.is_legacy() {
        json_str_append_legacy::<M>(manager, redis_key, ctx, path.get_path(), json)
    } else {
        json_str_append_impl::<M>(manager, redis_key, ctx, path.get_path(), json)
    }
}

fn json_str_append_impl<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
    json: &str,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::String)?;

    let mut need_notify = false;
    let res: Vec<_> = paths
        .into_iter()
        .map(|p| -> RedisResult {
            p.map_or(Ok(RedisValue::Null), |p| {
                need_notify = true;
                redis_key.str_append(p, json.to_string()).map(Into::into)
            })
        })
        .try_collect()?;
    if need_notify {
        redis_key.notify_keyspace_event(ctx, "json.strappend")?;
        manager.apply_changes(ctx);
    }
    Ok(res.into())
}

fn json_str_append_legacy<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
    json: &str,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_paths(path, root, |v| v.get_type() == SelectValueType::String)?;
    if !paths.is_empty() {
        let mut res = None;
        for p in paths {
            res = Some(redis_key.str_append(p, json.to_string())?);
        }
        redis_key.notify_keyspace_event(ctx, "json.strappend")?;
        manager.apply_changes(ctx);
        Ok(res.unwrap().into())
    } else {
        Err(RedisError::String(
            err_msg_json_path_doesnt_exist_with_param_or(path, "not a string"),
        ))
    }
}

///
/// JSON.STRLEN <key> [path]
///
pub fn json_str_len<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = args.next_str().map(Path::new).unwrap_or_default();

    let key = manager.open_key_read(ctx, &key)?;

    if path.is_legacy() {
        json_str_len_legacy::<M>(&key, path.get_path())
    } else {
        json_str_len_impl::<M>(&key, path.get_path())
    }
}

fn json_str_len_impl<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let values = find_all_values(path, root, |v| v.get_type() == SelectValueType::String)?;
    let res: Vec<_> = values
        .into_iter()
        .map(|v| v.map_or(RedisValue::Null, |v| v.get_str().len().into()))
        .collect();
    Ok(res.into())
}

fn json_str_len_legacy<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    match redis_key.get_value()? {
        Some(doc) => Ok(RedisValue::Integer(KeyValue::new(doc).str_len(path)? as i64)),
        None => Ok(RedisValue::Null),
    }
}

///
/// JSON.ARRAPPEND <key> <path> <json> [json ...]
///
pub fn json_arr_append<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1).peekable();

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);

    // We require at least one JSON item to append
    args.peek().ok_or(RedisError::WrongArity)?;

    let args = args
        .map(|arg| -> RedisResult<_> {
            let json = arg.try_as_str()?;
            let sv_holder = manager.from_str(json, Format::JSON, true)?;
            Ok(sv_holder)
        })
        .try_collect()?;

    let redis_key = manager.open_key_write(ctx, key)?;

    if path.is_legacy() {
        json_arr_append_legacy::<M>(manager, redis_key, ctx, &path, args)
    } else {
        json_arr_append_impl::<M>(manager, redis_key, ctx, path.get_path(), args)
    }
}

fn json_arr_append_legacy<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &Path,
    args: Vec<M::O>,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let mut paths = find_paths(path.get_path(), root, |v| {
        v.get_type() == SelectValueType::Array
    })?;
    if paths.is_empty() {
        Err(RedisError::String(
            err_msg_json_path_doesnt_exist_with_param_or(path.get_original(), "not an array"),
        ))
    } else if paths.len() == 1 {
        let res = redis_key.arr_append(paths.pop().unwrap(), args)?;
        redis_key.notify_keyspace_event(ctx, "json.arrappend")?;
        manager.apply_changes(ctx);
        Ok(res.into())
    } else {
        let mut res = 0;
        for p in paths {
            res = redis_key.arr_append(p, args.clone())?;
        }
        redis_key.notify_keyspace_event(ctx, "json.arrappend")?;
        manager.apply_changes(ctx);
        Ok(res.into())
    }
}

fn json_arr_append_impl<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
    args: Vec<M::O>,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;

    let mut need_notify = false;
    let res: Vec<_> = paths
        .into_iter()
        .map(|p| {
            p.map_or(Ok(RedisValue::Null), |p| {
                need_notify = true;
                redis_key.arr_append(p, args.clone()).map(Into::into)
            })
        })
        .try_collect()?;
    if need_notify {
        redis_key.notify_keyspace_event(ctx, "json.arrappend")?;
        manager.apply_changes(ctx);
    }
    Ok(res.into())
}

pub enum FoundIndex {
    Index(i64),
    NotFound,
    NotArray,
}

impl From<FoundIndex> for RedisValue {
    fn from(e: FoundIndex) -> Self {
        match e {
            FoundIndex::NotFound => Self::Integer(-1),
            FoundIndex::NotArray => Self::Null,
            FoundIndex::Index(i) => Self::Integer(i),
        }
    }
}

pub enum ObjectLen {
    Len(usize),
    NoneExisting,
    NotObject,
}

///
/// JSON.ARRINDEX <key> <path> <json-value> [start [stop]]
///
pub fn json_arr_index<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let value = args.next_str()?;
    let start: i64 = args.next().map_or(Ok(0), |v| v.parse_integer())?;
    let end: i64 = args.next().map_or(Ok(0), |v| v.parse_integer())?;

    args.done()?; // TODO: Add to other functions as well to terminate args list

    let key = manager.open_key_read(ctx, &key)?;

    let json_value: Value = serde_json::from_str(value)?;

    let res = key.get_value()?.map_or_else(
        || {
            Err(Error::from(err_msg_json_path_doesnt_exist_with_param(
                path.get_original(),
            )))
        },
        |doc| {
            if path.is_legacy() {
                KeyValue::new(doc).arr_index_legacy(path.get_path(), json_value, start, end)
            } else {
                KeyValue::new(doc).arr_index(path.get_path(), json_value, start, end)
            }
        },
    )?;

    Ok(res)
}

///
/// JSON.ARRINSERT <key> <path> <index> <json> [json ...]
///
pub fn json_arr_insert<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1).peekable();

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let index = args.next_i64()?;

    // We require at least one JSON item to insert
    args.peek().ok_or(RedisError::WrongArity)?;
    let args = args
        .map(|arg| -> RedisResult<_> {
            let json = arg.try_as_str()?;
            let sv_holder = manager.from_str(json, Format::JSON, true)?;
            Ok(sv_holder)
        })
        .try_collect()?;

    let redis_key = manager.open_key_write(ctx, key)?;
    if path.is_legacy() {
        json_arr_insert_legacy::<M>(manager, redis_key, ctx, path.get_path(), index, args)
    } else {
        json_arr_insert_impl::<M>(manager, redis_key, ctx, path.get_path(), index, args)
    }
}

fn json_arr_insert_impl<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
    index: i64,
    args: Vec<M::O>,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;

    let mut need_notify = false;
    let res: Vec<_> = paths
        .into_iter()
        .map(|p| {
            p.map_or(Ok(RedisValue::Null), |p| {
                need_notify = true;
                redis_key.arr_insert(p, &args, index).map(Into::into)
            })
        })
        .try_collect()?;

    if need_notify {
        redis_key.notify_keyspace_event(ctx, "json.arrinsert")?;
        manager.apply_changes(ctx);
    }
    Ok(res.into())
}

fn json_arr_insert_legacy<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
    index: i64,
    args: Vec<M::O>,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    if paths.is_empty() {
        Err(RedisError::String(
            err_msg_json_path_doesnt_exist_with_param_or(path, "not an array"),
        ))
    } else {
        let mut res = None;
        for p in paths {
            res = Some(redis_key.arr_insert(p, &args, index)?);
        }
        redis_key.notify_keyspace_event(ctx, "json.arrinsert")?;
        manager.apply_changes(ctx);
        Ok(res.unwrap().into())
    }
}

///
/// JSON.ARRLEN <key> [path]
///
pub fn json_arr_len<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = manager.open_key_read(ctx, &args.next_arg()?)?;
    let path = args.next_str().map(Path::new).unwrap_or_default();
    if path.is_legacy() {
        json_arr_len_legacy::<M>(key, path)
    } else {
        json_arr_len_impl::<M>(key, path)
    }
}

fn json_arr_len_impl<M>(key: M::ReadHolder, path: Path) -> RedisResult
where
    M: Manager,
{
    let root = key.get_value()?.ok_or(RedisError::nonexistent_key())?;
    let values = find_all_values(path.get_path(), root, |v| {
        v.get_type() == SelectValueType::Array
    })?;
    let res = values
        .into_iter()
        .map(|v| v.map_or(RedisValue::Null, |v| v.len().unwrap().into()))
        .collect_vec();
    Ok(res.into())
}

fn json_arr_len_legacy<M>(key: M::ReadHolder, path: Path) -> RedisResult
where
    M: Manager,
{
    let root = match key.get_value()? {
        Some(k) => k,
        None => {
            return Ok(RedisValue::Null);
        }
    };
    let values = find_all_values(path.get_path(), root, |v| {
        v.get_type() == SelectValueType::Array
    })?;
    values.into_iter().next().flatten().map_or_else(
        || {
            Err(RedisError::String(
                err_msg_json_path_doesnt_exist_with_param_or(path.get_original(), "not an array"),
            ))
        },
        |v| Ok(v.len().unwrap().into()),
    )
}

///
/// JSON.ARRPOP <key>
///         [FORMAT {STRINGS|EXPAND1|EXPAND}]   /* default is STRINGS */
///         [path [index]]
///
pub fn json_arr_pop<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;

    let is_resp3 = is_resp3(ctx);
    let mut format_options = ReplyFormatOptions::new(is_resp3, ReplyFormat::STRINGS);

    let path = if let Some(arg) = args.next() {
        if arg.try_as_str()?.eq_ignore_ascii_case(CMD_ARG_FORMAT) {
            if let Ok(next) = args.next_str() {
                format_options.format = ReplyFormat::from_str(next)?;
                if format_options.format == ReplyFormat::STRING {
                    // ARRPOP FORMAT STRING is not supported
                    return Err(RedisError::Str("ERR wrong reply format"));
                }
                if !format_options.resp3 {
                    return Err(RedisError::Str(
                        "ERR FORMAT argument is not supported on RESP2",
                    ));
                }
                args.next()
            } else {
                // If only the FORMAT subcommand is provided, then it's the path
                Some(arg)
            }
        } else {
            // if it's not FORMAT, then it's the path
            Some(arg)
        }
    } else {
        None
    };

    // Try to retrieve the optional arguments [path [index]]
    let (path, index) = match path {
        None => (Path::default(), i64::MAX),
        Some(s) => {
            let path = Path::new(s.try_as_str()?);
            let index = args.next_i64().unwrap_or(-1);
            (path, index)
        }
    };
    //args.done()?;

    let redis_key = manager.open_key_write(ctx, key)?;
    if path.is_legacy() {
        if format_options.format != ReplyFormat::STRINGS {
            return Err(RedisError::Str(
                "Legacy paths are supported only with FORMAT STRINGS",
            ));
        }

        json_arr_pop_legacy::<M>(manager, redis_key, ctx, path.get_path(), index)
    } else {
        json_arr_pop_impl::<M>(
            manager,
            redis_key,
            ctx,
            path.get_path(),
            index,
            format_options,
        )
    }
}

fn json_arr_pop_impl<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
    index: i64,
    format_options: ReplyFormatOptions,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    let mut need_notify = false;
    let res: Vec<_> = paths
        .into_iter()
        .map(|p| {
            p.map_or(Ok(RedisValue::Null), |p| {
                redis_key.arr_pop(p, index, |v| {
                    v.map_or(Ok(RedisValue::Null), |v| {
                        need_notify = true;
                        if format_options.is_resp3_reply() {
                            Ok(KeyValue::value_to_resp3(v, format_options))
                        } else {
                            serde_json::to_string(&v).into_both()
                        }
                    })
                })
            })
        })
        .try_collect()?;
    if need_notify {
        redis_key.notify_keyspace_event(ctx, "json.arrpop")?;
        manager.apply_changes(ctx);
    }
    Ok(res.into())
}

fn json_arr_pop_legacy<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
    index: i64,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    if paths.is_empty() {
        Err(RedisError::String(
            err_msg_json_path_doesnt_exist_with_param_or(path, "not an array"),
        ))
    } else {
        let res = paths.into_iter().try_fold(RedisValue::Null, |_, p| {
            redis_key.arr_pop(p, index, |v| {
                v.map_or(Ok(RedisValue::Null), |r| {
                    serde_json::to_string(&r).into_both()
                })
            })
        });
        redis_key.notify_keyspace_event(ctx, "json.arrpop")?;
        manager.apply_changes(ctx);
        res
    }
}

///
/// JSON.ARRTRIM <key> <path> <start> <stop>
///
pub fn json_arr_trim<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let start = args.next_i64()?;
    let stop = args.next_i64()?;

    let redis_key = manager.open_key_write(ctx, key)?;

    if path.is_legacy() {
        json_arr_trim_legacy::<M>(manager, redis_key, ctx, path.get_path(), start, stop)
    } else {
        json_arr_trim_impl::<M>(manager, redis_key, ctx, path.get_path(), start, stop)
    }
}
fn json_arr_trim_impl<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
    start: i64,
    stop: i64,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let mut need_notify = false;
    let res: Vec<_> = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Array)
        .and_then(|paths| {
            paths
                .into_iter()
                .map(|p| {
                    p.map_or(Ok(RedisValue::Null), |p| {
                        need_notify = true;
                        redis_key.arr_trim(p, start, stop).map(Into::into)
                    })
                })
                .try_collect()
        })?;
    if need_notify {
        redis_key.notify_keyspace_event(ctx, "json.arrtrim")?;
        manager.apply_changes(ctx);
    }
    Ok(res.into())
}

fn json_arr_trim_legacy<M>(
    manager: M,
    mut redis_key: M::WriteHolder,
    ctx: &Context,
    path: &str,
    start: i64,
    stop: i64,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    if paths.is_empty() {
        Err(RedisError::String(
            err_msg_json_path_doesnt_exist_with_param_or(path, "not an array"),
        ))
    } else {
        let res = paths
            .into_iter()
            .try_fold(0, |_, p| redis_key.arr_trim(p, start, stop))?;
        redis_key.notify_keyspace_event(ctx, "json.arrtrim")?;
        manager.apply_changes(ctx);
        Ok(res.into())
    }
}

///
/// JSON.OBJKEYS <key> [path]
///
pub fn json_obj_keys<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = args.next_str().map(Path::new).unwrap_or_default();

    let key = manager.open_key_read(ctx, &key)?;
    if path.is_legacy() {
        json_obj_keys_legacy::<M>(key, path.get_path())
    } else {
        json_obj_keys_impl::<M>(key, path.get_path())
    }
}

fn json_obj_keys_impl<M>(redis_key: M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    find_all_values(path, root, |v| v.get_type() == SelectValueType::Object).map(|v| {
        v.into_iter()
            .map(|v| v.map_or(RedisValue::Null, |v| v.keys().unwrap().collect_vec().into()))
            .collect_vec()
            .into()
    })
}

fn json_obj_keys_legacy<M>(redis_key: M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let root = match redis_key.get_value()? {
        Some(v) => v,
        _ => return Ok(RedisValue::Null),
    };
    KeyValue::new(root)
        .get_first(path)
        .map_or(Ok(RedisValue::Null), |v| match v.get_type() {
            SelectValueType::Object => Ok(v.keys().unwrap().collect_vec().into()),
            _ => Err(RedisError::String(
                err_msg_json_path_doesnt_exist_with_param_or(path, "not an object"),
            )),
        })
}

///
/// JSON.OBJLEN <key> [path]
///
pub fn json_obj_len<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = args.next_str().map(Path::new).unwrap_or_default();

    let key = manager.open_key_read(ctx, &key)?;
    if path.is_legacy() {
        json_obj_len_legacy::<M>(key, path.get_path())
    } else {
        json_obj_len_impl::<M>(key, path.get_path())
    }
}

fn json_obj_len_impl<M>(redis_key: M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    redis_key.get_value()?.map_or_else(
        || {
            Err(RedisError::String(
                err_msg_json_path_doesnt_exist_with_param_or(path, "not an object"),
            ))
        },
        |root| {
            find_all_values(path, root, |v| v.get_type() == SelectValueType::Object).map(|v| {
                v.into_iter()
                    .map(|v| {
                        v.map_or(RedisValue::Null, |v| {
                            RedisValue::Integer(v.len().unwrap() as _)
                        })
                    })
                    .collect_vec()
                    .into()
            })
        },
    )
}

fn json_obj_len_legacy<M>(redis_key: M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    match redis_key.get_value()? {
        Some(doc) => match KeyValue::new(doc).obj_len(path)? {
            ObjectLen::Len(l) => Ok(RedisValue::Integer(l as i64)),
            _ => Ok(RedisValue::Null),
        },
        None => Ok(RedisValue::Null),
    }
}

///
/// JSON.CLEAR <key> [path ...]
///
pub fn json_clear<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = match args.next() {
        Some(s) => Path::new(s.try_as_str()?),
        _ => Path::default(),
    };
    let path = path.get_path();

    let mut redis_key = manager.open_key_write(ctx, key)?;

    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_paths(path, root, |v| match v.get_type() {
        SelectValueType::Array | SelectValueType::Object => v.len().unwrap() > 0,
        SelectValueType::Long => v.get_long() != 0,
        SelectValueType::Double => v.get_double() != 0.0,
        _ => false,
    })?;
    let cleared = paths
        .into_iter()
        .try_fold(0, |acc, p| redis_key.clear(p).map(|cleared| acc + cleared))?;
    if cleared > 0 {
        redis_key.notify_keyspace_event(ctx, "json.clear")?;
        manager.apply_changes(ctx);
    }
    Ok(cleared.into())
}

///
/// JSON.DEBUG <subcommand & arguments>
///
/// subcommands:
/// MEMORY <key> [path]
/// HELP
///
pub fn json_debug<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    match args.next_str()?.to_uppercase().as_str() {
        "MEMORY" => {
            let key = args.next_arg()?;
            let path = args.next_str().map(Path::new).unwrap_or_default();
            let key = manager.open_key_read(ctx, &key)?;

            if path.is_legacy() {
                key.get_value()
                    .transpose()
                    .map_or(Ok(0), |doc| {
                        manager.get_memory(KeyValue::new(doc?).get_first(path.get_path())?)
                    })
                    .map(Into::into)
            } else {
                key.get_value()
                    .transpose()
                    .map_or(Ok(vec![]), |doc| {
                        KeyValue::new(doc?)
                            .get_values(path.get_path())?
                            .into_iter()
                            .map(|v| manager.get_memory(v))
                            .try_collect()
                    })
                    .map(Into::into)
            }
        }
        "HELP" => {
            let results = vec![
                "MEMORY <key> [path] - reports memory usage",
                "HELP                - this message",
            ];
            Ok(results.into())
        }
        _ => Err(RedisError::Str(
            "ERR unknown subcommand - try `JSON.DEBUG HELP`",
        )),
    }
}

///
/// JSON.RESP <key> [path]
///
pub fn json_resp<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = match args.next() {
        None => Path::default(),
        Some(s) => Path::new(s.try_as_str()?),
    };

    let key = manager.open_key_read(ctx, &key)?;
    key.get_value()?.map_or(Ok(RedisValue::Null), |doc| {
        KeyValue::new(doc).resp_serialize(path)
    })
}
