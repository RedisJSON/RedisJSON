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
    let path = crate::backwards_compat_path(args.next_string()?);
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
                    redis_key.set_root(val)?;
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
                redis_key.set_root(val)?;
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
    let path = args.next_string().map_or_else(
        |_| crate::JSON_ROOT_PATH.to_string(),
        |v| crate::backwards_compat_path(v),
    );

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
        let path = crate::backwards_compat_path(path.to_string());
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
    let path = crate::backwards_compat_path(args.next_string()?);

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

enum NumOp{
    INCR,
    MULT,
    POW,
}

fn command_json_num_op<M>(manager: M, ctx: &Context, args: Vec<String>, cmd: &str, op: NumOp) -> RedisResult 
where
M: Manager,
{
    let mut args = args.into_iter().skip(1);

    let key = args.next_string()?;
    let path = crate::backwards_compat_path(args.next_string()?);
    let number = args.next_string()?;

    let mut redis_key = manager.open_key_writable(ctx, &key)?;

    let root = redis_key.get_value()?.ok_or_else(RedisError::nonexistent_key)?;
    let mut selector = Selector::default();
    let mut res = selector.str_path(&path)?.value(root).select_with_paths()?;
    let paths = if res.len() > 0 {
        res.drain(..).filter(|v| v.n.get_type() == SelectValueType::Double || v.n.get_type() == SelectValueType::Long)
        .map(|v| v.path).collect()
    } else {
        Vec::new()
    };
    if paths.len() > 0 {

        let res = Ok(
            {match op{
                    NumOp::INCR => redis_key.incr_by(paths, number),
                    NumOp::MULT => redis_key.mult_by(paths, number),
                    NumOp::POW => redis_key.pow_by(paths, number),
                }
            }?
            .to_string().into());
        ctx.notify_keyspace_event(NotifyEvent::MODULE, cmd, key.as_str());
        ctx.replicate_verbatim();
        res
    } else {
        Err(RedisError::String(format!("Path '{}' does not exist", path)))
    }
}

pub fn command_json_num_incrby<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    command_json_num_op(manager, ctx, args, "json.numincrby", NumOp::INCR)
}

pub fn command_json_num_multby<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    command_json_num_op(manager, ctx, args, "json.nummultby", NumOp::MULT)
}

pub fn command_json_num_powby<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    command_json_num_op(manager, ctx, args, "json.numpowby", NumOp::POW)
}

pub fn command_json_bool_toggle<M: Manager>(manager: M, ctx: &Context, args: Vec<String>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_string()?;
    let path = crate::backwards_compat_path(args.next_string()?);
    let mut redis_key = manager.open_key_writable(ctx, &key)?;

    let root = redis_key.get_value()?.ok_or_else(RedisError::nonexistent_key)?;
    let mut selector = Selector::default();
    let mut res = selector.str_path(&path)?.value(root).select_with_paths()?;
    let paths = if res.len() > 0 {
        res.drain(..).filter(|v| v.n.get_type() == SelectValueType::Bool)
        .map(|v| v.path).collect()
    } else {
        Vec::new()
    };
    if paths.len() > 0 {
        let res = Ok(redis_key.bool_toggle(paths)?.to_string().into());
        ctx.notify_keyspace_event(NotifyEvent::MODULE, "json.toggle", key.as_str());
        ctx.replicate_verbatim();
        res
    } else {
        Err(RedisError::String(format!("Path '{}' does not exist", path)))
    }
}