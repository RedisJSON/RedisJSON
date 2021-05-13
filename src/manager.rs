use jsonpath_lib::select::select_value::SelectValue;
use jsonpath_lib::select::JsonPathError;
use serde_json::map::Entry;
use serde_json::Value;

use redis_module::key::RedisKeyWritable;
use redis_module::rediserror::RedisError;
use redis_module::Context;

use crate::redisjson::RedisJSON;
use crate::Format;
use crate::REDIS_JSON_TYPE;

use crate::error::Error;
use bson::decode_document;
use std::io::Cursor;

pub struct SetUpdateInfo {
    pub path: Vec<String>,
}

pub struct AddUpdateInfo {
    pub path: Vec<String>,
    pub key: String,
}

pub struct AddRootUpdateInfo {
    pub key: String,
}

pub enum UpdateInfo {
    SUI(SetUpdateInfo),
    AUI(AddUpdateInfo),
    ARUI(AddRootUpdateInfo),
}

pub trait Holder<E: Clone, V: SelectValue> {
    fn get_value(&self) -> Result<Option<&mut V>, RedisError>;
    fn set_root(&mut self, v: E) -> Result<(), RedisError>;
    fn set_value(&mut self, update_info: Vec<UpdateInfo>, v: E) -> Result<bool, RedisError>;
}

pub trait Manager {
    type V: SelectValue;
    type E: Clone;
    type Holder: Holder<Self::E, Self::V>;
    fn open_key_writable(&self, ctx: &Context, key: &str) -> Result<Self::Holder, RedisError>;
    fn from_str(&self, val: &str, format: Format) -> Result<Self::E, Error>;
}

pub struct KeyHolder {
    key: RedisKeyWritable,
}

impl KeyHolder {
    fn update<F: FnMut(Value) -> Option<Value>>(
        &self,
        path: &Vec<String>,
        root: &mut Value,
        mut func: F,
    ) -> Result<(), JsonPathError> {
        let mut target = root;

        let last_index = path.len().saturating_sub(1);
        for (i, token) in path.iter().enumerate() {
            let target_once = target;
            let is_last = i == last_index;
            let target_opt = match *target_once {
                Value::Object(ref mut map) => {
                    if is_last {
                        if let Entry::Occupied(mut e) = map.entry(token) {
                            let v = e.insert(Value::Null);
                            if let Some(res) = (func)(v) {
                                e.insert(res);
                            } else {
                                e.remove();
                            }
                        }
                        return Ok(());
                    }
                    map.get_mut(token)
                }
                Value::Array(ref mut vec) => {
                    if let Ok(x) = token.parse::<usize>() {
                        if is_last {
                            let v = std::mem::replace(&mut vec[x], Value::Null);
                            if let Some(res) = (func)(v) {
                                vec[x] = res;
                            } else {
                                vec.remove(x);
                            }
                            return Ok(());
                        }
                        vec.get_mut(x)
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(t) = target_opt {
                target = t;
            } else {
                break;
            }
        }

        Ok(())
    }
}

impl Holder<Value, Value> for KeyHolder {
    fn get_value(&self) -> Result<Option<&mut Value>, RedisError> {
        let key_value = self.key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?;
        match key_value {
            Some(v) => Ok(Some(&mut v.data)),
            None => Ok(None),
        }
    }

    fn set_value(&mut self, update_info: Vec<UpdateInfo>, v: Value) -> Result<bool, RedisError> {
        let mut updated = false;
        for ui in update_info {
            match ui {
                UpdateInfo::ARUI(aruv) => {
                    let root = self.get_value().unwrap().unwrap();
                    if let Value::Object(ref mut map) = root {
                        if !map.contains_key(&aruv.key) {
                            updated = true;
                            map.insert(aruv.key.to_string(), v.clone());
                        }
                    }
                }
                UpdateInfo::SUI(suv) => {
                    if suv.path.len() == 0 {
                        panic!("update the root should happened with set_root");
                    } else {
                        self.update(&suv.path, self.get_value().unwrap().unwrap(), |_v| {
                            updated = true;
                            Some(v.clone())
                        })?;
                    }
                }
                UpdateInfo::AUI(auv) => {
                    if auv.path.len() == 0 {
                        // todo: handle this, add dict element to the root
                        ();
                    } else {
                        self.update(&auv.path, self.get_value().unwrap().unwrap(), |mut ret| {
                            if let Value::Object(ref mut map) = ret {
                                if !map.contains_key(&auv.key) {
                                    updated = true;
                                    map.insert(auv.key.to_string(), v.clone());
                                }
                            }
                            Some(ret)
                        })?;
                    }
                }
            }
        }
        Ok(updated)
    }

    fn set_root(&mut self, v: Value) -> Result<(), RedisError> {
        self.key.set_value(&REDIS_JSON_TYPE, RedisJSON { data: v })
    }
}

pub struct RedisJsonKeyManager;

impl Manager for RedisJsonKeyManager {
    type Holder = KeyHolder;
    type V = Value;
    type E = Value;

    fn open_key_writable(&self, ctx: &Context, key: &str) -> Result<KeyHolder, RedisError> {
        let key = ctx.open_key_writable(key);
        Ok(KeyHolder { key: key })
    }

    fn from_str(&self, val: &str, format: Format) -> Result<Value, Error> {
        match format {
            Format::JSON => Ok(serde_json::from_str(val)?),
            Format::BSON => decode_document(&mut Cursor::new(val.as_bytes()))
                .map(|docs| {
                    let v = if !docs.is_empty() {
                        docs.iter()
                            .next()
                            .map_or_else(|| Value::Null, |(_, b)| b.clone().into())
                    } else {
                        Value::Null
                    };
                    Ok(v)
                })
                .unwrap_or_else(|e| Err(e.to_string().into())),
        }
    }
}
