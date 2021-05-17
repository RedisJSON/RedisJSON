use jsonpath_lib::select::select_value::SelectValue;
use jsonpath_lib::select::JsonPathError;
use serde_json::map::Entry;
use serde_json::{Number, Value};

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
    fn delete(&mut self) -> Result<(), RedisError>;
    fn get_value(&self) -> Result<Option<&mut V>, RedisError>;
    fn set_root(&mut self, v: E) -> Result<(), RedisError>;
    fn set_value(&mut self, update_info: Vec<UpdateInfo>, v: E) -> Result<bool, RedisError>;
    fn delete_paths(&mut self, paths: Vec<Vec<String>>) -> Result<usize, RedisError>;
    fn incr_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Value, RedisError>;
    fn mult_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Value, RedisError>;
    fn pow_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Value, RedisError>;
    fn bool_toggle(&mut self, paths: Vec<Vec<String>>) -> Result<Value, RedisError>;
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

    fn do_op<F>(
        &mut self,
        paths: Vec<Vec<String>>,
        mut op_fun: F,
    ) -> Result<Option<Value>, RedisError>
    where
        F: FnMut(&mut Value) -> Value,
    {
        let mut new = None;
        for p in paths {
            if p.len() == 0 {
                // updating the root require special treatment
                let root = self.get_value().unwrap().unwrap();
                let res = (op_fun)(root);
                new = Some(res.clone());
                self.set_root(res)?;
            } else {
                self.update(&p, self.get_value().unwrap().unwrap(), |mut v| {
                    let res = (op_fun)(&mut v);
                    new = Some(res.clone());
                    Some(res)
                })?;
            }
        }
        Ok(new)
    }

    fn do_num_op<F1, F2>(
        &mut self,
        paths: Vec<Vec<String>>,
        num: String,
        mut op1_fun: F1,
        mut op2_fun: F2,
    ) -> Result<Value, RedisError>
    where
        F1: FnMut(i64, i64) -> i64,
        F2: FnMut(f64, f64) -> f64,
    {
        let in_value = &serde_json::from_str(&num)?;
        if let Value::Number(in_value) = in_value {
            let res = self.do_op(paths, |v| {
                let num_res = match (v.as_i64(), in_value.as_i64()) {
                    (Some(num1), Some(num2)) => ((op1_fun)(num1, num2)).into(),
                    _ => {
                        let num1 = v.as_f64().unwrap();
                        let num2 = in_value.as_f64().unwrap();
                        Number::from_f64((op2_fun)(num1, num2)).unwrap()
                    }
                };
                Value::Number(num_res)
            })?;
            match res {
                None => Err(RedisError::Str("path does not exists")),
                Some(n) => Ok(n),
            }
        } else {
            Err(RedisError::Str("bad input number"))
        }
    }
}

impl Holder<Value, Value> for KeyHolder {
    fn delete(&mut self) -> Result<(), RedisError> {
        self.key.delete()?;
        Ok(())
    }

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

    fn delete_paths(&mut self, paths: Vec<Vec<String>>) -> Result<usize, RedisError> {
        let mut deleted = 0;
        for p in paths {
            self.update(&p, self.get_value().unwrap().unwrap(), |v| {
                if !v.is_null() {
                    deleted += 1; // might delete more than a single value
                }
                None
            })?;
        }
        Ok(deleted)
    }

    fn incr_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Value, RedisError> {
        self.do_num_op(paths, num, |i1, i2| i1 + i2, |f1, f2| f1 + f2)
    }

    fn mult_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Value, RedisError> {
        self.do_num_op(paths, num, |i1, i2| i1 * i2, |f1, f2| f1 * f2)
    }

    fn pow_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Value, RedisError> {
        self.do_num_op(paths, num, |i1, i2| i1.pow(i2 as u32), |f1, f2| f1.powf(f2))
    }

    fn bool_toggle(&mut self, paths: Vec<Vec<String>>) -> Result<Value, RedisError> {
        let res = self.do_op(paths, |v| Value::Bool(v.as_bool().unwrap() ^ true))?;
        match res {
            None => Err(RedisError::Str("path does not exists")),
            Some(n) => Ok(n),
        }
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
