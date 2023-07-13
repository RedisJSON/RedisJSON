/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use crate::error::Error;
use crate::formatter::FormatOptions;
use crate::key_value::KeyValue;
use crate::manager::err_msg_json_path_doesnt_exist_with_param;
use crate::manager::err_msg_json_path_doesnt_exist_with_param_or;
use crate::manager::{Manager, ReadHolder, UpdateInfo, WriteHolder};
use crate::redisjson::{Format, Path};
use json_path::select_value::{SelectValue, SelectValueType};
use redis_module::{Context, RedisValue};
use redis_module::{NextArg, RedisError, RedisResult, RedisString, REDIS_OK};
use std::cmp::Ordering;
use std::str::FromStr;

use json_path::{calc_once_with_paths, compile, json_path::UserPathTracker};

use crate::redisjson::SetOptions;

use serde_json::{Number, Value};

use itertools::FoldWhile::{Continue, Done};
use itertools::{EitherOrBoth, Itertools};
use serde::{Serialize, Serializer};

const JSON_ROOT_PATH: &str = "$";
const JSON_ROOT_PATH_LEGACY: &str = ".";
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
///         [path ...]
///
/// TODO add support for multi path
pub fn json_get<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;

    // Set Capacity to 1 assuming the common case has one path
    let mut paths: Vec<Path> = Vec::with_capacity(1);

    let mut format_options = FormatOptions {
        resp3: is_resp3(ctx),
        ..Default::default()
    };

    while let Ok(arg) = args.next_str() {
        match arg {
            // fast way to consider arg a path by using the max length of all possible subcommands
            // See #390 for the comparison of this function with/without this optimization
            arg if arg.len() > JSONGET_SUBCOMMANDS_MAXSTRLEN => paths.push(Path::new(arg)),
            arg if arg.eq_ignore_ascii_case(CMD_ARG_FORMAT) => {
                format_options.format = Format::from_str(args.next_str()?)?;
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

    // path is optional -> no path found we use root "$"
    if paths.is_empty() {
        paths.push(Path::new(JSON_ROOT_PATH_LEGACY));
    }

    let key = manager.open_key_read(ctx, &key)?;
    let value = match key.get_value()? {
        Some(doc) => KeyValue::new(doc).to_json(&mut paths, &format_options)?,
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
            if path.get_path() == JSON_ROOT_PATH {
                if op != SetOptions::NotExists {
                    redis_key.set_value(Vec::new(), val)?;
                    redis_key.apply_changes(ctx, "json.set")?;
                    REDIS_OK
                } else {
                    Ok(RedisValue::Null)
                }
            } else {
                let update_info = KeyValue::new(doc).find_paths(path.get_path(), &op)?;
                if !update_info.is_empty() {
                    let updated = apply_updates::<M>(&mut redis_key, val, update_info);
                    if updated {
                        redis_key.apply_changes(ctx, "json.set")?;
                        REDIS_OK
                    } else {
                        Ok(RedisValue::Null)
                    }
                } else {
                    Ok(RedisValue::Null)
                }
            }
        }
        (None, SetOptions::AlreadyExists) => Ok(RedisValue::Null),
        (None, _) => {
            if path.get_path() == JSON_ROOT_PATH {
                redis_key.set_value(Vec::new(), val)?;
                redis_key.apply_changes(ctx, "json.set")?;
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
            if path.get_path() == JSON_ROOT_PATH {
                redis_key.merge_value(Vec::new(), val)?;
                redis_key.apply_changes(ctx, "json.merge")?;
                REDIS_OK
            } else {
                let mut update_info =
                    KeyValue::new(doc).find_paths(path.get_path(), &SetOptions::None)?;
                if !update_info.is_empty() {
                    let mut res = false;
                    if update_info.len() == 1 {
                        res = match update_info.pop().unwrap() {
                            UpdateInfo::SUI(sui) => redis_key.merge_value(sui.path, val)?,
                            UpdateInfo::AUI(aui) => redis_key.dict_add(aui.path, &aui.key, val)?,
                        }
                    } else {
                        for ui in update_info {
                            res = match ui {
                                UpdateInfo::SUI(sui) => {
                                    redis_key.merge_value(sui.path, val.clone())?
                                }
                                UpdateInfo::AUI(aui) => {
                                    redis_key.dict_add(aui.path, &aui.key, val.clone())?
                                }
                            } || res; // If any of the updates succeed, return true
                        }
                    }
                    if res {
                        redis_key.apply_changes(ctx, "json.merge")?;
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
            if path.get_path() == JSON_ROOT_PATH {
                // Nothing to merge with it's a new doc
                redis_key.set_value(Vec::new(), val)?;
                redis_key.apply_changes(ctx, "json.merge")?;
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
    let mut args = args.into_iter().skip(1);

    if args.len() < 3 {
        return Err(RedisError::WrongArity);
    }

    // Collect all the actions from the args (redis_key, update_info, value)
    let mut actions = Vec::new();
    while let Ok(key) = args.next_arg() {
        let mut redis_key = manager.open_key_write(ctx, key)?;

        // Verify the key is a JSON type
        let key_value = redis_key.get_value()?;

        // Verify the path is valid and get all the update info
        let path = Path::new(args.next_str()?);
        let update_info = if path.get_path() == JSON_ROOT_PATH {
            None
        } else if let Some(value) = key_value {
            Some(KeyValue::new(value).find_paths(path.get_path(), &SetOptions::None)?)
        } else {
            return Err(RedisError::Str(
                "ERR new objects must be created at the root",
            ));
        };

        // Parse the input and validate it's valid JSON
        let value_str = args.next_str()?;
        let value = manager.from_str(value_str, Format::JSON, true)?;

        actions.push((redis_key, update_info, value));
    }

    actions
        .into_iter()
        .fold(REDIS_OK, |res, (mut redis_key, update_info, value)| {
            let updated = if let Some(update_info) = update_info {
                !update_info.is_empty() && apply_updates::<M>(&mut redis_key, value, update_info)
            } else {
                // In case it is a root path
                redis_key.set_value(Vec::new(), value)?
            };
            if updated {
                redis_key.apply_changes(ctx, "json.mset")?;
            }
            res
        })
}

fn apply_updates<M: Manager>(
    redis_key: &mut M::WriteHolder,
    value: M::O,
    mut update_info: Vec<UpdateInfo>,
) -> bool {
    // If there is only one update info, we can avoid cloning the value
    if update_info.len() == 1 {
        match update_info.pop().unwrap() {
            UpdateInfo::SUI(sui) => redis_key.set_value(sui.path, value).unwrap_or(false),
            UpdateInfo::AUI(aui) => redis_key
                .dict_add(aui.path, &aui.key, value)
                .unwrap_or(false),
        }
    } else {
        let mut updated = false;
        for ui in update_info {
            updated = match ui {
                UpdateInfo::SUI(sui) => redis_key
                    .set_value(sui.path, value.clone())
                    .unwrap_or(false),
                UpdateInfo::AUI(aui) => redis_key
                    .dict_add(aui.path, &aui.key, value.clone())
                    .unwrap_or(false),
            } || updated;
        }
        updated
    }
}

fn find_paths<T: SelectValue, F: FnMut(&T) -> bool>(
    path: &str,
    doc: &T,
    mut f: F,
) -> Result<Vec<Vec<String>>, RedisError> {
    let query = match compile(path) {
        Ok(q) => q,
        Err(e) => return Err(RedisError::String(e.to_string())),
    };
    let res = calc_once_with_paths(query, doc);
    Ok(res
        .into_iter()
        .filter(|e| f(e.res))
        .map(|e| e.path_tracker.unwrap().to_string_path())
        .collect())
}

/// Returns tuples of Value and its concrete path which match the given `path`
fn get_all_values_and_paths<'a, T: SelectValue>(
    path: &str,
    doc: &'a T,
) -> Result<Vec<(&'a T, Vec<String>)>, RedisError> {
    let query = match compile(path) {
        Ok(q) => q,
        Err(e) => return Err(RedisError::String(e.to_string())),
    };
    let res = calc_once_with_paths(query, doc);
    Ok(res
        .into_iter()
        .map(|e| (e.res, e.path_tracker.unwrap().to_string_path()))
        .collect())
}

/// Returns a Vec of paths with `None` for Values that do not match the filter
fn filter_paths<T, F>(values_and_paths: Vec<(&T, Vec<String>)>, f: F) -> Vec<Option<Vec<String>>>
where
    F: Fn(&T) -> bool,
{
    values_and_paths
        .into_iter()
        .map(|(v, p)| match f(v) {
            true => Some(p),
            _ => None,
        })
        .collect::<Vec<Option<Vec<String>>>>()
}

/// Returns a Vec of Values with `None` for Values that do not match the filter
fn filter_values<T, F>(values_and_paths: Vec<(&T, Vec<String>)>, f: F) -> Vec<Option<&T>>
where
    F: Fn(&T) -> bool,
{
    values_and_paths
        .into_iter()
        .map(|(v, _)| match f(v) {
            true => Some(v),
            _ => None,
        })
        .collect::<Vec<Option<&T>>>()
}

fn find_all_paths<T: SelectValue, F>(
    path: &str,
    doc: &T,
    f: F,
) -> Result<Vec<Option<Vec<String>>>, RedisError>
where
    F: Fn(&T) -> bool,
{
    let res = get_all_values_and_paths(path, doc)?;
    match res.is_empty() {
        false => Ok(filter_paths(res, f)),
        _ => Ok(vec![]),
    }
}

fn find_all_values<'a, T: SelectValue, F>(
    path: &str,
    doc: &'a T,
    f: F,
) -> Result<Vec<Option<&'a T>>, RedisError>
where
    F: Fn(&T) -> bool,
{
    let res = get_all_values_and_paths(path, doc)?;
    match res.is_empty() {
        false => Ok(filter_values(res, f)),
        _ => Ok(vec![]),
    }
}

fn to_json_value<T>(values: Vec<Option<T>>, none_value: Value) -> Vec<Value>
where
    Value: From<T>,
{
    values
        .into_iter()
        .map(|n| n.map_or_else(|| none_value.clone(), Into::into))
        .collect::<Vec<Value>>()
}

/// Sort the paths so higher indices precede lower indices on the same array,
/// And longer paths precede shorter paths
/// And if a path is a sub-path of the other, then only paths with shallower hierarchy (closer to the top-level) remain
fn prepare_paths_for_deletion(paths: &mut Vec<Vec<String>>) {
    if paths.len() < 2 {
        // No need to reorder when there are less than 2 paths
        return;
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
    let mut string_paths = Vec::new();
    paths.iter().for_each(|v| {
        string_paths.push(v.join(","));
    });
    string_paths.sort();

    paths.retain(|v| {
        let path = v.join(",");
        let found = string_paths.binary_search(&path).unwrap();
        for p in string_paths.iter().take(found) {
            if path.starts_with(p.as_str()) {
                return false;
            }
        }
        true
    });
}

///
/// JSON.DEL <key> [path]
///
pub fn json_del<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = match args.next() {
        None => Path::new(JSON_ROOT_PATH_LEGACY),
        Some(s) => Path::new(s.try_as_str()?),
    };

    let mut redis_key = manager.open_key_write(ctx, key)?;
    let deleted = match redis_key.get_value()? {
        Some(doc) => {
            let res = if path.get_path() == JSON_ROOT_PATH {
                redis_key.delete()?;
                1
            } else {
                let mut paths = find_paths(path.get_path(), doc, |_| true)?;
                prepare_paths_for_deletion(&mut paths);
                let mut changed = 0;
                for p in paths {
                    if redis_key.delete_path(p)? {
                        changed += 1;
                    }
                }
                changed
            };
            if res > 0 {
                redis_key.apply_changes(ctx, "json.del")?;
            }
            res
        }
        None => 0,
    };
    Ok((deleted as i64).into())
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

        let format_options = FormatOptions {
            resp3: is_resp3(ctx),
            ..Default::default()
        };

        let to_string =
            |doc: &M::V| KeyValue::new(doc).to_string_multi(path.get_path(), &format_options);
        let to_string_legacy =
            |doc: &M::V| KeyValue::new(doc).to_string_single(path.get_path(), &format_options);
        let is_legacy = path.is_legacy();

        let results: Result<Vec<RedisValue>, RedisError> = keys
            .iter()
            .map(|key| {
                manager
                    .open_key_read(ctx, key)
                    .map_or(Ok(RedisValue::Null), |json_key| {
                        json_key.get_value().map_or(Ok(RedisValue::Null), |value| {
                            value
                                .map(|doc| {
                                    if is_legacy {
                                        to_string_legacy(doc)
                                    } else {
                                        to_string(doc)
                                    }
                                })
                                .transpose()
                                .map_or(Ok(RedisValue::Null), |v| Ok(v.into()))
                        })
                    })
            })
            .collect();

        Ok(results?.into())
    })
}

///
/// JSON.TYPE <key> [path]
///
pub fn json_type<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = Path::new(args.next_str().unwrap_or(JSON_ROOT_PATH_LEGACY));

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
    let root = redis_key.get_value()?;
    let value = match root {
        Some(root) => KeyValue::new(root)
            .get_values(path)?
            .iter()
            .map(|v| (KeyValue::value_name(*v)).into())
            .collect::<Vec<RedisValue>>()
            .into(),
        None => RedisValue::Null,
    };
    Ok(value)
}

fn json_type_legacy<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let value = redis_key.get_value()?.map_or_else(
        || RedisValue::Null,
        |doc| {
            KeyValue::new(doc)
                .get_type(path)
                .map_or(RedisValue::Null, Into::into)
        },
    );
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

    let mut redis_key = manager.open_key_write(ctx, key)?;

    // check context flags to see if RESP3 is enabled
    if is_resp3(ctx) {
        let res = json_num_op_impl::<M>(&mut redis_key, ctx, path.get_path(), number, op, cmd)?
            .drain(..)
            .map(|v| {
                v.map_or(RedisValue::Null, |v| {
                    if let Some(i) = v.as_i64() {
                        RedisValue::Integer(i)
                    } else {
                        RedisValue::Float(v.as_f64().unwrap_or_default())
                    }
                })
            })
            .collect::<Vec<RedisValue>>()
            .into();
        Ok(res)
    } else if path.is_legacy() {
        json_num_op_legacy::<M>(&mut redis_key, ctx, path.get_path(), number, op, cmd)
    } else {
        let results = json_num_op_impl::<M>(&mut redis_key, ctx, path.get_path(), number, op, cmd)?;

        // Convert to RESP2 format return as one JSON array
        let values = to_json_value::<Number>(results, Value::Null);
        Ok(KeyValue::<M::V>::serialize_object(&values, &FormatOptions::default()).into())
    }
}

fn json_num_op_impl<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    number: &str,
    op: NumOp,
    cmd: &str,
) -> Result<Vec<Option<Number>>, RedisError>
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

    let mut res = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                Some(match op {
                    NumOp::Incr => redis_key.incr_by(p, number)?,
                    NumOp::Mult => redis_key.mult_by(p, number)?,
                    NumOp::Pow => redis_key.pow_by(p, number)?,
                })
            }
            _ => None,
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, cmd)?;
    }
    Ok(res)
}

fn json_num_op_legacy<M>(
    redis_key: &mut M::WriteHolder,
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
        v.get_type() == SelectValueType::Double || v.get_type() == SelectValueType::Long
    })?;
    if !paths.is_empty() {
        let mut res = None;
        for p in paths {
            res = Some(match op {
                NumOp::Incr => redis_key.incr_by(p, number)?,
                NumOp::Mult => redis_key.mult_by(p, number)?,
                NumOp::Pow => redis_key.pow_by(p, number)?,
            });
        }
        redis_key.apply_changes(ctx, cmd)?;
        Ok(res.unwrap().to_string().into())
    } else {
        Err(RedisError::String(
            err_msg_json_path_doesnt_exist_with_param_or(path, "does not contains a number"),
        ))
    }
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
    let path = Path::new(args.next_str()?);
    let mut redis_key = manager.open_key_write(ctx, key)?;

    if path.is_legacy() {
        json_bool_toggle_legacy::<M>(&mut redis_key, ctx, path.get_path())
    } else {
        json_bool_toggle_impl::<M>(&mut redis_key, ctx, path.get_path())
    }
}

fn json_bool_toggle_impl<M>(
    redis_key: &mut M::WriteHolder,
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
    let mut res: Vec<RedisValue> = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                RedisValue::Integer((redis_key.bool_toggle(p)?).into())
            }
            None => RedisValue::Null,
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, "json.toggle")?;
    }
    Ok(res.into())
}

fn json_bool_toggle_legacy<M>(
    redis_key: &mut M::WriteHolder,
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
        redis_key.apply_changes(ctx, "json.toggle")?;
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
        path = Path::new(JSON_ROOT_PATH_LEGACY);
        json = path_or_json;
    }

    let mut redis_key = manager.open_key_write(ctx, key)?;

    if path.is_legacy() {
        json_str_append_legacy::<M>(&mut redis_key, ctx, path.get_path(), json)
    } else {
        json_str_append_impl::<M>(&mut redis_key, ctx, path.get_path(), json)
    }
}

fn json_str_append_impl<M>(
    redis_key: &mut M::WriteHolder,
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

    let mut res: Vec<RedisValue> = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                (redis_key.str_append(p, json.to_string())?).into()
            }
            _ => RedisValue::Null,
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, "json.strappend")?;
    }
    Ok(res.into())
}

fn json_str_append_legacy<M>(
    redis_key: &mut M::WriteHolder,
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
        redis_key.apply_changes(ctx, "json.strappend")?;
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
    let path = Path::new(args.next_str().unwrap_or(JSON_ROOT_PATH_LEGACY));

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
    let mut res: Vec<RedisValue> = vec![];
    for v in values {
        res.push(v.map_or(RedisValue::Null, |v| (v.get_str().len() as i64).into()));
    }
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

    let args = args.try_fold::<_, _, Result<_, RedisError>>(
        Vec::with_capacity(args.len()),
        |mut acc, arg| {
            let json = arg.try_as_str()?;
            acc.push(manager.from_str(json, Format::JSON, true)?);
            Ok(acc)
        },
    )?;

    let mut redis_key = manager.open_key_write(ctx, key)?;

    if path.is_legacy() {
        json_arr_append_legacy::<M>(&mut redis_key, ctx, &path, args)
    } else {
        json_arr_append_impl::<M>(&mut redis_key, ctx, path.get_path(), args)
    }
}

fn json_arr_append_legacy<M>(
    redis_key: &mut M::WriteHolder,
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
        redis_key.apply_changes(ctx, "json.arrappend")?;
        Ok(res.into())
    } else {
        let mut res = 0;
        for p in paths {
            res = redis_key.arr_append(p, args.clone())?;
        }
        redis_key.apply_changes(ctx, "json.arrappend")?;
        Ok(res.into())
    }
}

fn json_arr_append_impl<M>(
    redis_key: &mut M::WriteHolder,
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

    let mut res = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                (redis_key.arr_append(p, args.clone())? as i64).into()
            }
            _ => RedisValue::Null,
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, "json.arrappend")?;
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
    let args = args.try_fold::<_, _, Result<_, RedisError>>(
        Vec::with_capacity(args.len()),
        |mut acc, arg| {
            let json = arg.try_as_str()?;
            acc.push(manager.from_str(json, Format::JSON, true)?);
            Ok(acc)
        },
    )?;
    let mut redis_key = manager.open_key_write(ctx, key)?;
    if path.is_legacy() {
        json_arr_insert_legacy::<M>(&mut redis_key, ctx, path.get_path(), index, args)
    } else {
        json_arr_insert_impl::<M>(&mut redis_key, ctx, path.get_path(), index, args)
    }
}

fn json_arr_insert_impl<M>(
    redis_key: &mut M::WriteHolder,
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

    let mut res: Vec<RedisValue> = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                (redis_key.arr_insert(p, &args, index)? as i64).into()
            }
            _ => RedisValue::Null,
        });
    }

    if need_notify {
        redis_key.apply_changes(ctx, "json.arrinsert")?;
    }
    Ok(res.into())
}

fn json_arr_insert_legacy<M>(
    redis_key: &mut M::WriteHolder,
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
        redis_key.apply_changes(ctx, "json.arrinsert")?;
        Ok(res.unwrap().into())
    }
}

///
/// JSON.ARRLEN <key> [path]
///
pub fn json_arr_len<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = Path::new(args.next_str().unwrap_or(JSON_ROOT_PATH_LEGACY));
    let is_legacy = path.is_legacy();
    let key = manager.open_key_read(ctx, &key)?;
    let root = match key.get_value()? {
        Some(k) => k,
        None if is_legacy => {
            return Ok(RedisValue::Null);
        }
        None => {
            return Err(RedisError::nonexistent_key());
        }
    };
    let values = find_all_values(path.get_path(), root, |v| {
        v.get_type() == SelectValueType::Array
    })?;
    if is_legacy && values.is_empty() {
        return Err(RedisError::String(
            err_msg_json_path_doesnt_exist_with_param(path.get_original()),
        ));
    }
    let mut res = vec![];
    for v in values {
        let cur_val: RedisValue = match v {
            Some(v) => (v.len().unwrap() as i64).into(),
            _ => {
                if is_legacy {
                    return Err(RedisError::String(
                        err_msg_json_path_doesnt_exist_with_param_or(
                            path.get_original(),
                            "not an array",
                        ),
                    ));
                }
                RedisValue::Null
            }
        };
        if is_legacy {
            return Ok(cur_val);
        }
        res.push(cur_val);
    }
    Ok(res.into())
}

///
/// JSON.ARRPOP <key> [path [index]]
///
pub fn json_arr_pop<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;

    let (path, index) = match args.next() {
        None => (Path::new(JSON_ROOT_PATH_LEGACY), i64::MAX),
        Some(s) => {
            let path = Path::new(s.try_as_str()?);
            let index = args.next_i64().unwrap_or(-1);
            (path, index)
        }
    };

    let mut redis_key = manager.open_key_write(ctx, key)?;
    if path.is_legacy() {
        json_arr_pop_legacy::<M>(&mut redis_key, ctx, path.get_path(), index)
    } else {
        json_arr_pop_impl::<M>(&mut redis_key, ctx, path.get_path(), index)
    }
}

fn json_arr_pop_impl<M>(
    redis_key: &mut M::WriteHolder,
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

    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    let mut res: Vec<RedisValue> = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => match redis_key.arr_pop(p, index)? {
                Some(v) => {
                    need_notify = true;
                    v.into()
                }
                _ => RedisValue::Null, // Empty array
            },
            _ => RedisValue::Null, // Not an array
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, "json.arrpop")?;
    }
    Ok(res.into())
}

fn json_arr_pop_legacy<M>(
    redis_key: &mut M::WriteHolder,
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
    if !paths.is_empty() {
        let mut res = None;
        for p in paths {
            res = Some(redis_key.arr_pop(p, index)?);
        }
        match res.unwrap() {
            Some(r) => {
                redis_key.apply_changes(ctx, "json.arrpop")?;
                Ok(r.into())
            }
            None => Ok(().into()),
        }
    } else {
        Err(RedisError::String(
            err_msg_json_path_doesnt_exist_with_param_or(path, "not an array"),
        ))
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

    let mut redis_key = manager.open_key_write(ctx, key)?;

    if path.is_legacy() {
        json_arr_trim_legacy::<M>(&mut redis_key, ctx, path.get_path(), start, stop)
    } else {
        json_arr_trim_impl::<M>(&mut redis_key, ctx, path.get_path(), start, stop)
    }
}
fn json_arr_trim_impl<M>(
    redis_key: &mut M::WriteHolder,
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

    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    let mut res: Vec<RedisValue> = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                (redis_key.arr_trim(p, start, stop)?).into()
            }
            _ => RedisValue::Null,
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, "json.arrtrim")?;
    }
    Ok(res.into())
}

fn json_arr_trim_legacy<M>(
    redis_key: &mut M::WriteHolder,
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
        let mut res = None;
        for p in paths {
            res = Some(redis_key.arr_trim(p, start, stop)?);
        }
        redis_key.apply_changes(ctx, "json.arrtrim")?;
        Ok(res.unwrap().into())
    }
}

///
/// JSON.OBJKEYS <key> [path]
///
pub fn json_obj_keys<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = Path::new(args.next_str().unwrap_or(JSON_ROOT_PATH_LEGACY));

    let mut key = manager.open_key_read(ctx, &key)?;
    if path.is_legacy() {
        json_obj_keys_legacy::<M>(&mut key, path.get_path())
    } else {
        json_obj_keys_impl::<M>(&mut key, path.get_path())
    }
}

fn json_obj_keys_impl<M>(redis_key: &mut M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let res: RedisValue = {
        let values = find_all_values(path, root, |v| v.get_type() == SelectValueType::Object)?;
        let mut res: Vec<RedisValue> = vec![];
        for v in values {
            res.push(v.map_or(RedisValue::Null, |v| {
                v.keys().unwrap().collect::<Vec<&str>>().into()
            }));
        }
        res.into()
    };
    Ok(res)
}

fn json_obj_keys_legacy<M>(redis_key: &mut M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let root = match redis_key.get_value()? {
        Some(v) => v,
        _ => return Ok(RedisValue::Null),
    };
    let value = match KeyValue::new(root).get_first(path) {
        Ok(v) => match v.get_type() {
            SelectValueType::Object => v.keys().unwrap().collect::<Vec<&str>>().into(),
            _ => {
                return Err(RedisError::String(
                    err_msg_json_path_doesnt_exist_with_param_or(path, "not an object"),
                ))
            }
        },
        _ => RedisValue::Null,
    };
    Ok(value)
}

///
/// JSON.OBJLEN <key> [path]
///
pub fn json_obj_len<M: Manager>(manager: M, ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = Path::new(args.next_str().unwrap_or(JSON_ROOT_PATH_LEGACY));

    let key = manager.open_key_read(ctx, &key)?;
    if path.is_legacy() {
        json_obj_len_legacy::<M>(&key, path.get_path())
    } else {
        json_obj_len_impl::<M>(&key, path.get_path())
    }
}

fn json_obj_len_impl<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let root = redis_key.get_value()?;
    let res = match root {
        Some(root) => find_all_values(path, root, |v| v.get_type() == SelectValueType::Object)?
            .iter()
            .map(|v| {
                v.map_or(RedisValue::Null, |v| {
                    RedisValue::Integer(v.len().unwrap() as i64)
                })
            })
            .collect::<Vec<RedisValue>>()
            .into(),
        None => {
            return Err(RedisError::String(
                err_msg_json_path_doesnt_exist_with_param_or(path, "not an object"),
            ))
        }
    };
    Ok(res)
}

fn json_obj_len_legacy<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
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
    let paths = args.try_fold::<_, _, Result<Vec<Path>, RedisError>>(
        Vec::with_capacity(args.len()),
        |mut acc, arg| {
            let s = arg.try_as_str()?;
            acc.push(Path::new(s));
            Ok(acc)
        },
    )?;

    let paths = if paths.is_empty() {
        vec![Path::new(JSON_ROOT_PATH)]
    } else {
        paths
    };

    let path = paths.first().unwrap().get_path();

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
    let mut cleared = 0;
    if !paths.is_empty() {
        for p in paths {
            cleared += redis_key.clear(p)?;
        }
    }
    if cleared > 0 {
        redis_key.apply_changes(ctx, "json.clear")?;
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
            let path = Path::new(args.next_str().unwrap_or(JSON_ROOT_PATH_LEGACY));

            let key = manager.open_key_read(ctx, &key)?;
            if path.is_legacy() {
                Ok(match key.get_value()? {
                    Some(doc) => {
                        manager.get_memory(KeyValue::new(doc).get_first(path.get_path())?)?
                    }
                    None => 0,
                }
                .into())
            } else {
                Ok(match key.get_value()? {
                    Some(doc) => KeyValue::new(doc)
                        .get_values(path.get_path())?
                        .iter()
                        .map(|v| manager.get_memory(v).unwrap())
                        .collect::<Vec<usize>>(),
                    None => vec![],
                }
                .into())
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
        None => Path::new(JSON_ROOT_PATH_LEGACY),
        Some(s) => Path::new(s.try_as_str()?),
    };

    let key = manager.open_key_read(ctx, &key)?;
    key.get_value()?.map_or_else(
        || Ok(RedisValue::Null),
        |doc| KeyValue::new(doc).resp_serialize(path),
    )
}
