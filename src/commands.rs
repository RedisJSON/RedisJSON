use crate::formatter::RedisJsonFormatter;
use crate::manager::{
    AddRootUpdateInfo, AddUpdateInfo, Holder, Manager, SetUpdateInfo, UpdateInfo,
};
use crate::redisjson::{Format, Path};
use jsonpath_lib::select::select_value::{SelectValue, SelectValueType};
use redis_module::{Context, RedisValue};
use redis_module::{NextArg, NotifyEvent, RedisError, RedisResult, REDIS_OK};

use jsonpath_lib::select::Selector;

use crate::nodevisitor::{StaticPathElement, StaticPathParser, VisitStatus};

use crate::error::Error;

use crate::redisjson::SetOptions;

use serde_json::{Map, Value};

use serde::Serialize;

const JSON_ROOT_PATH: &'static str = "$";

pub struct KeyValue<'a, V: SelectValue> {
    val: &'a mut V,
}

impl<'a, V: SelectValue> KeyValue<'a, V> {
    pub fn new(v: &'a mut V) -> KeyValue<'a, V> {
        KeyValue { val: v }
    }

    fn to_value(&self, val: &V) -> Value {
        match val.get_type() {
            SelectValueType::Null => Value::Null,
            SelectValueType::Bool => Value::Bool(val.get_bool()),
            SelectValueType::String => Value::String(val.get_str()),
            SelectValueType::Long => val.get_long().into(),
            SelectValueType::Double => val.get_double().into(),
            SelectValueType::Array => {
                let mut arr = Vec::new();
                for i in 0..val.len().unwrap() {
                    arr.push(self.to_value(val.get_index(i).unwrap()));
                }
                Value::Array(arr)
            }
            SelectValueType::Dict => {
                let mut m = Map::new();
                for k in val.keys().unwrap() {
                    m.insert(k.to_string(), self.to_value(val.get_key(&k).unwrap()));
                }
                Value::Object(m)
            }
            SelectValueType::Undef => panic!("reach undefine value"),
        }
    }

    fn get_first(&'a self, path: &'a str) -> Result<&'a V, Error> {
        let results = self.get_values(path)?;
        match results.first() {
            Some(s) => Ok(s),
            None => Err("ERR path does not exist".into()),
        }
    }

    fn get_values(&'a self, path: &'a str) -> Result<Vec<&'a V>, Error> {
        let mut selector = Selector::new();
        selector.str_path(path)?;
        selector.value(self.val);
        let results = selector.select()?;
        Ok(results)
    }

    fn to_json(
        &'a self,
        paths: &mut Vec<Path>,
        indent: String,
        newline: String,
        space: String,
        format: Format,
    ) -> Result<String, Error> {
        let temp_doc;
        let res = if paths.len() > 1 {
            // TODO: Creating a temp doc here duplicates memory usage. This can be very memory inefficient.
            // A better way would be to create a doc of references to the original doc but no current support
            // in serde_json. I'm going for this implementation anyway because serde_json isn't supposed to be
            // memory efficient and we're using it anyway. See https://github.com/serde-rs/json/issues/635.
            temp_doc = Value::Object(paths.drain(..).fold(Map::new(), |mut acc, path| {
                let mut selector = Selector::new();
                selector.value(self.val);
                if let Err(_) = selector.str_path(&path.fixed) {
                    return acc;
                }
                let value = match selector.select() {
                    Ok(s) => match s.first() {
                        Some(v) => self.to_value(v),
                        None => Value::Null,
                    },
                    Err(_) => Value::Null,
                };
                acc.insert(path.path, value);
                acc
            }));
            temp_doc
        } else {
            self.to_value(self.get_first(&paths[0].fixed)?)
        };

        match format {
            Format::JSON => {
                let formatter = RedisJsonFormatter::new(
                    indent.as_bytes(),
                    space.as_bytes(),
                    newline.as_bytes(),
                );

                let mut out = serde_json::Serializer::with_formatter(Vec::new(), formatter);
                res.serialize(&mut out).unwrap();
                Ok(String::from_utf8(out.into_inner()).unwrap())
            }
            Format::BSON => Err("Soon to come...".into()), //results.into() as Bson,
        }
    }

    fn find_add_paths(&mut self, path: &str) -> Result<Vec<UpdateInfo>, Error> {
        let mut parsed_static_path = StaticPathParser::check(path)?;

        if parsed_static_path.valid != VisitStatus::Valid {
            return Err("Err: wrong static path".into());
        }
        if parsed_static_path.static_path_elements.len() < 2 {
            return Err("Err: path must end with object key to set".into());
        }

        let last = parsed_static_path.static_path_elements.pop().unwrap();

        if let StaticPathElement::ObjectKey(key) = last {
            if let StaticPathElement::Root = parsed_static_path.static_path_elements.last().unwrap()
            {
                // Adding to the root
                Ok(vec![UpdateInfo::ARUI(AddRootUpdateInfo {
                    key: key.to_string(),
                })])
            } else {
                // Adding somewhere in existing object, use jsonpath_lib::replace_with
                let mut selector = Selector::default();
                if let Err(e) = selector.str_path(
                    &parsed_static_path
                        .static_path_elements
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<String>>()
                        .join(""),
                ) {
                    return Err(e.into());
                }
                selector.value(self.val);
                let mut res = selector.select_with_paths()?;
                Ok(res
                    .drain(..)
                    .map(|v| {
                        UpdateInfo::AUI(AddUpdateInfo {
                            path: v.path,
                            key: key.to_string(),
                        })
                    })
                    .collect())
            }
        } else if let StaticPathElement::ArrayIndex(_) = last {
            // if we reach here with array path we must be out of range
            // otherwise the path would be valid to be set and we would not
            // have reached here!!
            Err("array index out of range".into())
        } else {
            Err("path not an object".into())
        }
    }

    pub fn find_paths(
        &mut self,
        path: &str,
        option: &SetOptions,
    ) -> Result<Vec<UpdateInfo>, Error> {
        if SetOptions::NotExists != *option {
            let mut selector = Selector::default();
            let mut res = selector
                .str_path(path)?
                .value(self.val)
                .select_with_paths()?;
            if res.len() > 0 {
                return Ok(res
                    .drain(..)
                    .map(|v| UpdateInfo::SUI(SetUpdateInfo { path: v.path }))
                    .collect());
            }
        }
        if SetOptions::AlreadyExists != *option {
            self.find_add_paths(path)
        } else {
            Ok(Vec::new()) // empty vector means no updates
        }
    }

    pub fn serialize(results: &V, format: Format) -> Result<String, Error> {
        let res = match format {
            Format::JSON => serde_json::to_string(results)?,
            Format::BSON => return Err("Soon to come...".into()), //results.into() as Bson,
        };
        Ok(res)
    }

    pub fn to_string(&self, path: &str, format: Format) -> Result<String, Error> {
        let results = self.get_first(path)?;
        Self::serialize(results, format)
    }

    pub fn get_type(&self, path: &str) -> Result<String, Error> {
        let s = Self::value_name(self.get_first(path)?);
        Ok(s.to_string())
    }

    pub fn value_name(value: &V) -> &str {
        match value.get_type() {
            SelectValueType::Null => "null",
            SelectValueType::Bool => "boolean",
            SelectValueType::Long => "integer",
            SelectValueType::Double => "number",
            SelectValueType::String => "string",
            SelectValueType::Array => "array",
            SelectValueType::Dict => "object",
            SelectValueType::Undef => panic!("undefine value"),
        }
    }

    pub fn str_len(&self, path: &str) -> Result<usize, Error> {
        let first = self.get_first(path)?;
        match first.get_type() {
            SelectValueType::String => Ok(first.get_str().len()),
            _ => Err("ERR wrong type of path value".into()),
        }
    }

    pub fn arr_len(&self, path: &str) -> Result<usize, Error> {
        let first = self.get_first(path)?;
        match first.get_type() {
            SelectValueType::Array => Ok(first.len().unwrap()),
            _ => Err("ERR wrong type of path value".into()),
        }
    }

    pub fn obj_len(&self, path: &str) -> Result<usize, Error> {
        let first = self.get_first(path)?;
        match first.get_type() {
            SelectValueType::Dict => Ok(first.keys().unwrap().len()),
            _ => Err("ERR wrong type of path value".into()),
        }
    }

    pub fn is_eqaul<T1: SelectValue, T2: SelectValue>(&self, a: &T1, b: &T2) -> bool {
        match (a.get_type(), b.get_type()) {
            (SelectValueType::Null, SelectValueType::Null) => true,
            (SelectValueType::Bool, SelectValueType::Bool) => a.get_bool() == b.get_bool(),
            (SelectValueType::Long, SelectValueType::Long) => a.get_long() == b.get_long(),
            (SelectValueType::Double, SelectValueType::Double) => a.get_double() == b.get_double(),
            (SelectValueType::String, SelectValueType::String) => a.get_str() == b.get_str(),
            (SelectValueType::Array, SelectValueType::Array) => {
                if a.len().unwrap() != b.len().unwrap() {
                    false
                } else {
                    for (i, e) in a.values().unwrap().into_iter().enumerate() {
                        if !self.is_eqaul(e, b.get_index(i).unwrap()) {
                            return false;
                        }
                    }
                    true
                }
            }
            (SelectValueType::Dict, SelectValueType::Dict) => {
                if a.keys().unwrap().len() != b.keys().unwrap().len() {
                    false
                } else {
                    for k in a.keys().unwrap() {
                        let temp1 = a.get_key(&k);
                        let temp2 = b.get_key(&k);
                        match (temp1, temp2) {
                            (Some(a1), Some(b1)) => {
                                if !self.is_eqaul(a1, b1) {
                                    return false;
                                }
                            }
                            (_, _) => return false,
                        }
                    }
                    true
                }
            }
            (_, _) => false,
        }
    }

    pub fn arr_index(
        &self,
        path: &str,
        scalar_json: &str,
        start: i64,
        end: i64,
    ) -> Result<i64, Error> {
        let res = self.get_first(path)?;
        if res.get_type() == SelectValueType::Array {
            // end=-1/0 means INFINITY to support backward with RedisJSON
            if res.len().unwrap() == 0 || end < -1 {
                return Ok(-1);
            }
            let v: Value = serde_json::from_str(scalar_json)?;

            let len = res.len().unwrap() as i64;

            // Normalize start
            let start = if start < 0 {
                0.max(len + start)
            } else {
                // start >= 0
                start.min(len - 1)
            };

            // Normalize end
            let end = if end == 0 {
                len
            } else if end < 0 {
                len + end
            } else {
                // end > 0
                end.min(len)
            };

            if end < start {
                // don't search at all
                return Ok(-1);
            }
            let mut i = -1;
            for index in start..end {
                if self.is_eqaul(res.get_index(index as usize).unwrap(), &v) {
                    i = index;
                    break;
                }
            }

            Ok(i)
        } else {
            Ok(-1)
        }
    }

    pub fn obj_keys(&self, path: &str) -> Result<Vec<String>, Error> {
        self.get_first(path)?
            .keys()
            .ok_or_else(|| "ERR wrong type of path value".into())
    }
}

pub fn command_json_get<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;

    let mut paths: Vec<Path> = vec![];
    let mut format = Format::JSON;
    let mut indent = String::new();
    let mut space = String::new();
    let mut newline = String::new();
    while let Ok(arg) = args.next_string() {
        match arg.to_uppercase().as_str() {
            "INDENT" => {
                indent = args.next_string()?;
            }
            "NEWLINE" => {
                newline = args.next_string()?;
            }
            "SPACE" => {
                space = args.next_string()?;
            }
            "NOESCAPE" => {
                // Silently ignore. Compatibility with ReJSON v1.0 which has this option. See #168
                continue;
            } // TODO add support
            "FORMAT" => {
                format = Format::from_str(args.next_string()?.as_str())?;
            }
            _ => {
                paths.push(Path::new(arg));
            }
        };
    }

    // path is optional -> no path found we use root "$"
    if paths.is_empty() {
        paths.push(Path::new("$".to_string()));
    }

    let key = manager.open_key_writable(ctx, &key)?;
    let value = match key.get_value()? {
        Some(doc) => KeyValue::new(doc)
            .to_json(&mut paths, indent, newline, space, format)?
            .into(),
        None => RedisValue::Null,
    };

    Ok(value)
}

pub fn command_json_set<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);
    let value = args.next_string()?;

    let mut format = Format::JSON;
    let mut set_option = SetOptions::None;

    while let Some(s) = args.next() {
        match s.to_uppercase().as_str() {
            "NX" => set_option = SetOptions::NotExists,
            "XX" => set_option = SetOptions::AlreadyExists,
            "FORMAT" => {
                format = Format::from_str(args.next_string()?.as_str())?;
            }
            _ => break,
        };
    }

    let mut redis_key = manager.open_key_writable(ctx, &key)?;
    let current = redis_key.get_value()?;

    let val = manager.from_str(&value, format)?;

    match (current, set_option) {
        (Some(ref mut doc), ref op) => {
            if path == "$" {
                if *op != SetOptions::NotExists {
                    redis_key.set_root(Some(val))?;
                    ctx.notify_keyspace_event(NotifyEvent::MODULE, "json_set", key.as_str());
                    ctx.replicate_verbatim();
                    REDIS_OK
                } else {
                    Ok(RedisValue::Null)
                }
            } else {
                let update_info = KeyValue::new(*doc).find_paths(&path, op)?;
                if update_info.len() > 0 {
                    if redis_key.set_value(update_info, val)? {
                        ctx.notify_keyspace_event(NotifyEvent::MODULE, "json_set", key.as_str());
                        ctx.replicate_verbatim();
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
            if path == "$" {
                redis_key.set_root(Some(val))?;
                ctx.notify_keyspace_event(NotifyEvent::MODULE, "json_set", key.as_str());
                ctx.replicate_verbatim();
                REDIS_OK
            } else {
                Err(RedisError::Str(
                    "ERR new objects must be created at the root",
                ))
            }
        }
    }
}

pub fn command_json_del<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = args
        .next_string()
        .map_or_else(|_| JSON_ROOT_PATH.to_string(), |v| backwards_compat_path(v));

    let mut redis_key = manager.open_key_writable(ctx, &key)?;
    let deleted = match redis_key.get_value()? {
        Some(doc) => {
            let res = if path == "$" {
                redis_key.delete()?;
                1
            } else {
                let mut selector = Selector::default();
                let mut res = selector.str_path(&path)?.value(doc).select_with_paths()?;
                let paths = if res.len() > 0 {
                    res.drain(..).map(|v| v.path).collect()
                } else {
                    Vec::new()
                };
                if paths.len() > 0 {
                    redis_key.delete_paths(paths)?
                } else {
                    0
                }
            };
            if res > 0 {
                ctx.notify_keyspace_event(NotifyEvent::MODULE, "json_del", key.as_str());
                ctx.replicate_verbatim();
            }
            res
        }
        None => 0,
    };
    Ok(deleted.into())
}

pub fn command_json_mget<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    if args.len() < 3 {
        return Err(RedisError::WrongArity);
    }

    args.last().ok_or(RedisError::WrongArity).and_then(|path| {
        let path = backwards_compat_path(path.to_string());
        let keys = &args[1..args.len() - 1];

        let results: Result<Vec<RedisValue>, RedisError> = keys
            .iter()
            .map(|key| {
                let result = manager
                    .open_key_writable(ctx, key)?
                    .get_value()?
                    .map(|doc| KeyValue::new(doc).to_string(&path, Format::JSON))
                    .transpose()?;

                Ok(result.into())
            })
            .collect();

        Ok(results?.into())
    })
}

pub fn command_json_type<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    let key = manager.open_key_writable(ctx, &key)?;

    let value = key.get_value()?.map_or_else(
        || RedisValue::Null,
        |doc| match KeyValue::new(doc).get_type(&path) {
            Ok(s) => s.into(),
            Err(_) => RedisValue::Null,
        },
    );

    Ok(value)
}

enum NumOp {
    INCR,
    MULT,
    POW,
}

fn command_json_num_op<M>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
    cmd: &str,
    op: NumOp,
) -> RedisResult
where
    M: Manager,
{
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);
    let number = args.next_string()?;

    let mut redis_key = manager.open_key_writable(ctx, &key)?;

    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let mut selector = Selector::default();
    let mut res = selector.str_path(&path)?.value(root).select_with_paths()?;
    let paths = if res.len() > 0 {
        res.drain(..)
            .filter(|v| {
                v.n.get_type() == SelectValueType::Double || v.n.get_type() == SelectValueType::Long
            })
            .map(|v| v.path)
            .collect()
    } else {
        Vec::new()
    };
    if paths.len() > 0 {
        let res = Ok({
            match op {
                NumOp::INCR => redis_key.incr_by(paths, number),
                NumOp::MULT => redis_key.mult_by(paths, number),
                NumOp::POW => redis_key.pow_by(paths, number),
            }
        }?
        .to_string()
        .into());
        ctx.notify_keyspace_event(NotifyEvent::MODULE, cmd, key.as_str());
        ctx.replicate_verbatim();
        res
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist",
            path
        )))
    }
}

pub fn command_json_num_incrby<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    command_json_num_op(manager, ctx, args, "json.numincrby", NumOp::INCR)
}

pub fn command_json_num_multby<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    command_json_num_op(manager, ctx, args, "json.nummultby", NumOp::MULT)
}

pub fn command_json_num_powby<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    command_json_num_op(manager, ctx, args, "json.numpowby", NumOp::POW)
}

pub fn command_json_bool_toggle<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);
    let mut redis_key = manager.open_key_writable(ctx, &key)?;

    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let mut selector = Selector::default();
    let mut res = selector.str_path(&path)?.value(root).select_with_paths()?;
    let paths = if res.len() > 0 {
        res.drain(..)
            .filter(|v| v.n.get_type() == SelectValueType::Bool)
            .map(|v| v.path)
            .collect()
    } else {
        Vec::new()
    };
    if paths.len() > 0 {
        let res = Ok(redis_key.bool_toggle(paths)?.to_string().into());
        ctx.notify_keyspace_event(NotifyEvent::MODULE, "json.toggle", key.as_str());
        ctx.replicate_verbatim();
        res
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist",
            path
        )))
    }
}

pub fn command_json_str_append<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path_or_json = args.next_string()?;

    let path;
    let json;

    // path is optional
    if let Ok(val) = args.next_string() {
        path = backwards_compat_path(path_or_json);
        json = val;
    } else {
        path = JSON_ROOT_PATH.to_string();
        json = path_or_json;
    }

    let mut redis_key = manager.open_key_writable(ctx, &key)?;

    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let mut selector = Selector::default();
    let mut res = selector.str_path(&path)?.value(root).select_with_paths()?;
    let paths = if res.len() > 0 {
        res.drain(..)
            .filter(|v| v.n.get_type() == SelectValueType::String)
            .map(|v| v.path)
            .collect()
    } else {
        Vec::new()
    };

    if paths.len() > 0 {
        let res = Ok(redis_key.str_append(paths, json)?.into());
        ctx.notify_keyspace_event(NotifyEvent::MODULE, "json.strappend", key.as_str());
        ctx.replicate_verbatim();
        res
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist",
            path
        )))
    }
}

pub fn command_json_str_len<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    let key = manager.open_key_writable(ctx, &key)?;
    match key.get_value()? {
        Some(doc) => Ok(RedisValue::Integer(
            KeyValue::new(doc).str_len(&path)? as i64
        )),
        None => Ok(RedisValue::Null),
    }
}

pub fn command_json_arr_append<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1).peekable();

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    // We require at least one JSON item to append
    args.peek().ok_or(RedisError::WrongArity)?;

    let mut redis_key = manager.open_key_writable(ctx, &key)?;
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let mut selector = Selector::default();
    let mut res = selector.str_path(&path)?.value(root).select_with_paths()?;
    let paths = if res.len() > 0 {
        res.drain(..)
            .filter(|v| v.n.get_type() == SelectValueType::Array)
            .map(|v| v.path)
            .collect()
    } else {
        Vec::new()
    };

    if paths.len() > 0 {
        let res = Ok(redis_key.arr_append(paths, args)?.into());
        ctx.notify_keyspace_event(NotifyEvent::MODULE, "json.arrappend", key.as_str());
        ctx.replicate_verbatim();
        res
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist",
            path
        )))
    }
}

pub fn command_json_arr_index<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);
    let json_scalar = args.next_string()?;
    let start: i64 = args.next().map(|v| v.parse()).unwrap_or(Ok(0))?;
    let end: i64 = args.next().map(|v| v.parse()).unwrap_or(Ok(0))?;

    args.done()?; // TODO: Add to other functions as well to terminate args list

    let key = manager.open_key_writable(ctx, &key)?;

    let index = key.get_value()?.map_or(Ok(-1), |doc| {
        KeyValue::new(doc).arr_index(&path, &json_scalar, start, end)
    })?;

    Ok(index.into())
}

pub fn command_json_arr_insert<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1).peekable();

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);
    let index = args.next_i64()?;

    // We require at least one JSON item to append
    args.peek().ok_or(RedisError::WrongArity)?;

    let mut redis_key = manager.open_key_writable(ctx, &key)?;

    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let mut selector = Selector::default();
    let mut res = selector.str_path(&path)?.value(root).select_with_paths()?;
    let paths = if res.len() > 0 {
        res.drain(..)
            .filter(|v| v.n.get_type() == SelectValueType::Array)
            .map(|v| v.path)
            .collect()
    } else {
        Vec::new()
    };

    if paths.len() > 0 {
        let res = Ok(redis_key.arr_insert(paths, args, index)?.into());
        ctx.notify_keyspace_event(NotifyEvent::MODULE, "json.arrinsert", key.as_str());
        ctx.replicate_verbatim();
        res
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist",
            path
        )))
    }
}

pub fn command_json_arr_len<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    let key = manager.open_key_writable(ctx, &key)?;
    match key.get_value()? {
        Some(doc) => Ok(RedisValue::Integer(
            KeyValue::new(doc).arr_len(&path)? as i64
        )),
        None => Ok(RedisValue::Null),
    }
}

pub fn command_json_arr_pop<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;

    let (path, index) = args
        .next()
        .map(|p| {
            let path = backwards_compat_path(p);
            let index = args.next_i64().unwrap_or(-1);
            (path, index)
        })
        .unwrap_or((JSON_ROOT_PATH.to_string(), i64::MAX));

    let mut redis_key = manager.open_key_writable(ctx, &key)?;

    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let mut selector = Selector::default();
    let mut res = selector.str_path(&path)?.value(root).select_with_paths()?;
    let paths = if res.len() > 0 {
        res.drain(..)
            .filter(|v| v.n.get_type() == SelectValueType::Array)
            .map(|v| v.path)
            .collect()
    } else {
        Vec::new()
    };

    if paths.len() > 0 {
        let res = redis_key.arr_pop(paths, index)?;
        match res {
            Some(r) => {
                ctx.notify_keyspace_event(NotifyEvent::MODULE, "json.arrpop", key.as_str());
                ctx.replicate_verbatim();
                Ok(r.into())
            }
            None => Ok(().into()),
        }
    } else {
        Ok(().into())
    }
}

pub fn command_json_arr_trim<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);
    let start = args.next_i64()?;
    let stop = args.next_i64()?;

    let mut redis_key = manager.open_key_writable(ctx, &key)?;

    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let mut selector = Selector::default();
    let mut res = selector.str_path(&path)?.value(root).select_with_paths()?;
    let paths = if res.len() > 0 {
        res.drain(..)
            .filter(|v| v.n.get_type() == SelectValueType::Array)
            .map(|v| v.path)
            .collect()
    } else {
        Vec::new()
    };

    if paths.len() > 0 {
        let res = Ok(redis_key.arr_trim(paths, start, stop)?.into());
        ctx.notify_keyspace_event(NotifyEvent::MODULE, "json.arrtrim", key.as_str());
        ctx.replicate_verbatim();
        res
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist",
            path
        )))
    }
}

pub fn command_json_obj_keys<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    let key = manager.open_key_writable(ctx, &key)?;

    let value = match key.get_value()? {
        Some(doc) => KeyValue::new(doc).obj_keys(&path)?.into(),
        None => RedisValue::Null,
    };

    Ok(value)
}

pub fn command_json_obj_len<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<String>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = backwards_compat_path(args.next_string()?);

    let key = manager.open_key_writable(ctx, &key)?;
    match key.get_value()? {
        Some(doc) => Ok(RedisValue::Integer(
            KeyValue::new(doc).obj_len(&path)? as i64
        )),
        None => Ok(RedisValue::Null),
    }
}

pub fn command_json_clear<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let paths = args.map(Path::new).collect::<Vec<_>>();

    let paths = if paths.is_empty() {
        vec![Path::new(JSON_ROOT_PATH.to_string())]
    } else {
        paths
    };

    let path = paths.first().unwrap().fixed.as_str();

    // FIXME: handle multi paths
    let mut redis_key = manager.open_key_writable(ctx, &key)?;

    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let mut selector = Selector::default();
    let mut res = selector.str_path(path)?.value(root).select_with_paths()?;
    let paths = if res.len() > 0 {
        res.drain(..).map(|v| v.path).collect()
    } else {
        Vec::new()
    };

    if paths.len() > 0 {
        let res = Ok(redis_key.clear(paths)?.into());
        ctx.notify_keyspace_event(NotifyEvent::MODULE, "json.clear", key.as_str());
        ctx.replicate_verbatim();
        res
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist",
            path
        )))
    }
}

pub fn command_json_debug<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    match args.next_string()?.to_uppercase().as_str() {
        "MEMORY" => {
            let key = args.next_string()?;
            let path = backwards_compat_path(args.next_string()?);

            let key = manager.open_key_writable(ctx, &key)?;
            let value = match key.get_value()? {
                Some(doc) => manager.get_memory(KeyValue::new(doc).get_first(&path)?)?,
                None => 0,
            };
            Ok(value.into())
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

pub fn command_json_resp<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = args
        .next_string()
        .map_or_else(|_| JSON_ROOT_PATH.to_string(), |v| backwards_compat_path(v));

    let key = manager.open_key_writable(ctx, &key)?;
    match key.get_value()? {
        Some(doc) => Ok(manager.resp_serialize(KeyValue::new(doc).get_first(&path)?)),
        None => Ok(RedisValue::Null),
    }
}

pub fn command_json_cache_info<M: Manager>(
    _manager: M,
    _ctx: &Context,
    _args: Vec<String>,
) -> RedisResult {
    Err(RedisError::Str("Command was not implemented"))
}

pub fn command_json_cache_init<M: Manager>(
    _manager: M,
    _ctx: &Context,
    _args: Vec<String>,
) -> RedisResult {
    Err(RedisError::Str("Command was not implemented"))
}

///
/// Backwards compatibility convertor for RedisJSON 1.x clients
///
fn backwards_compat_path(mut path: String) -> String {
    if !path.starts_with('$') {
        if path == "." {
            path.replace_range(..1, "$");
        } else if path.starts_with('.') {
            path.insert(0, '$');
        } else {
            path.insert_str(0, "$.");
        }
    }
    path
}
