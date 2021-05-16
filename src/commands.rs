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
