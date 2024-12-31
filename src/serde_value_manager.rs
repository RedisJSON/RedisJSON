/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use crate::jsonpath::select_value::SelectValue;
use serde::Deserialize;
use serde_json::map::Entry;
use serde_json::{Number, Value};

use crate::manager::{err_json, err_msg_json_expected, err_msg_json_path_doesnt_exist};
use crate::manager::{Manager, ReadHolder, WriteHolder};
use redis_module::key::{verify_type, RedisKey, RedisKeyWritable};
use redis_module::raw::{RedisModuleKey, Status};
use redis_module::rediserror::RedisError;
use redis_module::{Context, NotifyEvent, RedisString};

use std::marker::PhantomData;

use crate::redisjson::{normalize_arr_start_index, RedisJSON};
use crate::Format;
use crate::REDIS_JSON_TYPE;

use crate::error::Error;
use bson::{from_document, Document};
use std::io::Cursor;

use crate::array_index::ArrayIndex;

use std::mem;

pub struct KeyHolderWrite<'a> {
    key: RedisKeyWritable,
    key_name: RedisString,
    val: Option<&'a mut RedisJSON<Value>>,
}

fn update<F: FnMut(Value) -> Result<Option<Value>, Error>>(
    path: &[String],
    root: &mut Value,
    mut func: F,
) -> Result<(), Error> {
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
                        if let Some(res) = (func)(v)? {
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
                        if x < vec.len() {
                            let v = std::mem::replace(&mut vec[x], Value::Null);
                            if let Some(res) = (func)(v)? {
                                vec[x] = res;
                            } else {
                                vec.remove(x);
                            }
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

impl<'a> KeyHolderWrite<'a> {
    fn do_op<F>(&mut self, paths: &[String], mut op_fun: F) -> Result<(), RedisError>
    where
        F: FnMut(Value) -> Result<Option<Value>, Error>,
    {
        if paths.is_empty() {
            // updating the root require special treatment
            let root = self.get_value().unwrap().unwrap();
            let prev_val = root.take();
            let rollback_val = prev_val.clone();
            let res = (op_fun)(prev_val);
            match res {
                Ok(res) => self.set_root(res)?,
                Err(err) => {
                    self.set_root(Some(rollback_val))?;
                    return Err(RedisError::String(err.msg));
                }
            }
        } else {
            update(paths, self.get_value().unwrap().unwrap(), op_fun)?;
        }

        Ok(())
    }

    fn do_num_op<F1, F2>(
        &mut self,
        path: &[String],
        num: &str,
        mut op1_fun: F1,
        mut op2_fun: F2,
    ) -> Result<Number, RedisError>
    where
        F1: FnMut(i64, i64) -> i64,
        F2: FnMut(f64, f64) -> f64,
    {
        let in_value = &serde_json::from_str(num)?;
        if let Value::Number(in_value) = in_value {
            let mut res = None;
            self.do_op(path, |v| {
                let num_res = match (v.as_i64(), in_value.as_i64()) {
                    (Some(num1), Some(num2)) => Ok(((op1_fun)(num1, num2)).into()),
                    _ => {
                        let num1 = v.as_f64().unwrap();
                        let num2 = in_value.as_f64().unwrap();
                        Number::from_f64((op2_fun)(num1, num2))
                            .ok_or(RedisError::Str("result is not a number"))
                    }
                };
                res = Some(Value::Number(num_res?));
                Ok(res.clone())
            })?;
            match res {
                None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
                Some(n) => match n {
                    Value::Number(n) => Ok(n),
                    _ => Err(RedisError::Str("return value is not a number")),
                },
            }
        } else {
            Err(RedisError::Str("bad input number"))
        }
    }

    fn get_json_holder(&mut self) -> Result<(), RedisError> {
        if self.val.is_none() {
            self.val = self.key.get_value::<RedisJSON<Value>>(&REDIS_JSON_TYPE)?;
        }
        Ok(())
    }

    fn set_root(&mut self, v: Option<Value>) -> Result<(), RedisError> {
        match v {
            Some(inner) => {
                self.get_json_holder()?;
                match &mut self.val {
                    Some(v) => v.data = inner,
                    None => self
                        .key
                        .set_value(&REDIS_JSON_TYPE, RedisJSON { data: inner })?,
                }
            }
            None => {
                self.val = None;
                self.key.delete()?;
            }
        }
        Ok(())
    }

    fn serialize(results: &Value, format: Format) -> Result<String, Error> {
        let res = match format {
            Format::JSON => serde_json::to_string(results)?,
            Format::BSON => return Err("ERR Soon to come...".into()), //results.into() as Bson,
        };
        Ok(res)
    }
}

impl<'a> WriteHolder<Value, Value> for KeyHolderWrite<'a> {
    fn apply_changes(&mut self, ctx: &Context, command: &str) -> Result<(), RedisError> {
        if ctx.notify_keyspace_event(NotifyEvent::MODULE, command, &self.key_name) == Status::Ok {
            ctx.replicate_verbatim();
            Ok(())
        } else {
            Err(RedisError::Str("failed notify key space event"))
        }
    }

    fn delete(&mut self) -> Result<(), RedisError> {
        self.key.delete()?;
        Ok(())
    }

    fn get_value(&mut self) -> Result<Option<&mut Value>, RedisError> {
        self.get_json_holder()?;

        match &mut self.val {
            Some(v) => Ok(Some(&mut v.data)),
            None => Ok(None),
        }
    }

    fn set_value(&mut self, path: Vec<String>, mut v: Value) -> Result<bool, RedisError> {
        let mut updated = false;
        if path.is_empty() {
            // update the root
            self.set_root(Some(v))?;
            updated = true;
        } else {
            update(&path, self.get_value().unwrap().unwrap(), |_v| {
                updated = true;
                Ok(Some(v.take()))
            })?;
        }
        Ok(updated)
    }

    fn dict_add(&mut self, path: Vec<String>, key: &str, mut v: Value) -> Result<bool, RedisError> {
        let mut updated = false;
        if path.is_empty() {
            // update the root
            let root = self.get_value().unwrap().unwrap();
            let val = if let Value::Object(mut o) = root.take() {
                if !o.contains_key(key) {
                    updated = true;
                    o.insert(key.to_string(), v.take());
                }
                Value::Object(o)
            } else {
                root.take()
            };
            self.set_root(Some(val))?;
        } else {
            update(&path, self.get_value().unwrap().unwrap(), |val| {
                let val = if let Value::Object(mut o) = val {
                    if !o.contains_key(key) {
                        updated = true;
                        o.insert(key.to_string(), v.take());
                    }
                    Value::Object(o)
                } else {
                    val
                };
                Ok(Some(val))
            })?;
        }
        Ok(updated)
    }

    fn delete_path(&mut self, path: Vec<String>) -> Result<bool, RedisError> {
        let mut deleted = false;
        update(&path, self.get_value().unwrap().unwrap(), |_v| {
            deleted = true; // might delete more than a single value
            Ok(None)
        })?;
        Ok(deleted)
    }

    fn incr_by(&mut self, path: Vec<String>, num: &str) -> Result<Number, RedisError> {
        self.do_num_op(&path, num, |i1, i2| i1 + i2, |f1, f2| f1 + f2)
    }

    fn mult_by(&mut self, path: Vec<String>, num: &str) -> Result<Number, RedisError> {
        self.do_num_op(&path, num, |i1, i2| i1 * i2, |f1, f2| f1 * f2)
    }

    fn pow_by(&mut self, path: Vec<String>, num: &str) -> Result<Number, RedisError> {
        self.do_num_op(&path, num, |i1, i2| i1.pow(i2 as u32), f64::powf)
    }

    fn bool_toggle(&mut self, path: Vec<String>) -> Result<bool, RedisError> {
        let mut res = None;
        self.do_op(&path, |v| {
            let val = v.as_bool().unwrap() ^ true;
            res = Some(val);
            Ok(Some(Value::Bool(val)))
        })?;
        match res {
            None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Some(n) => Ok(n),
        }
    }

    fn str_append(&mut self, path: Vec<String>, val: String) -> Result<usize, RedisError> {
        let json = serde_json::from_str(&val)?;
        if let Value::String(s) = json {
            let mut res = None;
            self.do_op(&path, |v| {
                let new_str = [v.as_str().unwrap(), s.as_str()].concat();
                res = Some(new_str.len());
                Ok(Some(Value::String(new_str)))
            })?;
            match res {
                None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
                Some(l) => Ok(l),
            }
        } else {
            Err(RedisError::String(err_msg_json_expected(
                "string",
                val.as_str(),
            )))
        }
    }

    fn arr_append(&mut self, path: Vec<String>, mut args: Vec<Value>) -> Result<usize, RedisError> {
        let mut res = None;
        self.do_op(&path, |mut v| {
            let arr = v.as_array_mut().unwrap();
            arr.append(&mut args);
            res = Some(arr.len());
            Ok(Some(v))
        })?;
        res.map_or_else(
            || Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Ok,
        )
    }

    fn arr_insert(
        &mut self,
        paths: Vec<String>,
        args: &[Value],
        index: i64,
    ) -> Result<usize, RedisError> {
        let mut res = None;
        self.do_op(&paths, |mut v| {
            // Verify legal index in bounds
            let len = v.len().unwrap() as i64;
            let index = if index < 0 { len + index } else { index };
            if !(0..=len).contains(&index) {
                return Err("ERR index out of bounds".into());
            }
            let index = index as usize;
            let mut new_value = v.take();
            let curr = new_value.as_array_mut().unwrap();
            curr.splice(index..index, args.to_owned());
            res = Some(curr.len());
            Ok(Some(new_value))
        })?;
        res.map_or_else(
            || Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Ok,
        )
    }

    fn arr_pop(&mut self, path: Vec<String>, index: i64) -> Result<Option<String>, RedisError> {
        let mut res = None;
        self.do_op(&path, |mut v| {
            if let Some(array) = v.as_array() {
                if array.is_empty() {
                    return Ok(Some(v));
                }
                // Verify legel index in bounds
                let len = array.len() as i64;
                let index = normalize_arr_start_index(index, len) as usize;

                let mut new_value = v.take();
                let curr = new_value.as_array_mut().unwrap();
                res = Some(curr.remove(index));
                Ok(Some(new_value))
            } else {
                Err(err_json(&v, "array"))
            }
        })?;
        match res {
            None => Ok(None),
            Some(n) => Ok(Some(Self::serialize(&n, Format::JSON)?)),
        }
    }

    fn arr_trim(&mut self, path: Vec<String>, start: i64, stop: i64) -> Result<usize, RedisError> {
        let mut res = None;
        self.do_op(&path, |mut v| {
            if let Some(array) = v.as_array() {
                let len = array.len() as i64;
                let stop = stop.normalize(len);
                let start = if start < 0 || start < len {
                    start.normalize(len)
                } else {
                    stop + 1 //  start >=0 && start >= len
                };
                let range = if start > stop || len == 0 {
                    0..0 // Return an empty array
                } else {
                    start..(stop + 1)
                };

                let mut new_value = v.take();
                let curr = new_value.as_array_mut().unwrap();
                curr.rotate_left(range.start);
                curr.resize(range.end - range.start, Value::Null);
                res = Some(curr.len());
                Ok(Some(new_value))
            } else {
                Err(err_json(&v, "array"))
            }
        })?;
        res.map_or_else(
            || Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Ok,
        )
    }

    fn clear(&mut self, path: Vec<String>) -> Result<usize, RedisError> {
        let mut cleared = 0;
        self.do_op(&path, |v| match v {
            Value::Object(mut obj) => {
                obj.clear();
                cleared += 1;
                Ok(Some(Value::from(obj)))
            }
            Value::Array(mut arr) => {
                arr.clear();
                cleared += 1;
                Ok(Some(Value::from(arr)))
            }
            Value::Number(mut _num) => {
                cleared += 1;
                Ok(Some(Value::from(0)))
            }
            _ => Ok(Some(v)),
        })?;
        Ok(cleared)
    }
}

pub struct KeyHolderRead {
    key: RedisKey,
}

impl ReadHolder<Value> for KeyHolderRead {
    fn get_value(&self) -> Result<Option<&Value>, RedisError> {
        let key_value = self.key.get_value::<RedisJSON<Value>>(&REDIS_JSON_TYPE)?;
        key_value.map_or(Ok(None), |v| Ok(Some(&v.data)))
    }
}

pub struct RedisJsonKeyManager<'a> {
    pub phantom: PhantomData<&'a u64>,
}

impl<'a> Manager for RedisJsonKeyManager<'a> {
    type WriteHolder = KeyHolderWrite<'a>;
    type ReadHolder = KeyHolderRead;
    type V = Value;
    type O = Value;

    fn open_key_read(&self, ctx: &Context, key: &RedisString) -> Result<KeyHolderRead, RedisError> {
        let key = ctx.open_key(key);
        Ok(KeyHolderRead { key })
    }

    fn open_key_write(
        &self,
        ctx: &Context,
        key: RedisString,
    ) -> Result<KeyHolderWrite<'a>, RedisError> {
        let key_ptr = ctx.open_key_writable(&key);
        Ok(KeyHolderWrite {
            key: key_ptr,
            key_name: key,
            val: None,
        })
    }

    fn from_str(&self, val: &str, format: Format, limit_depth: bool) -> Result<Value, Error> {
        match format {
            Format::JSON => {
                let mut deserializer = serde_json::Deserializer::from_str(val);
                if !limit_depth {
                    deserializer.disable_recursion_limit();
                }
                Value::deserialize(&mut deserializer).map_err(|e| e.into())
            }
            Format::BSON => from_document(
                Document::from_reader(&mut Cursor::new(val.as_bytes()))
                    .map_err(|e| e.to_string())?,
            )
            .map(|docs: Document| {
                let v = if docs.is_empty() {
                    Value::Null
                } else {
                    docs.iter()
                        .next()
                        .map_or_else(|| Value::Null, |(_, b)| b.clone().into())
                };
                Ok(v)
            })
            .unwrap_or_else(|e| Err(e.to_string().into())),
        }
    }

    fn get_memory(v: &Value) -> Result<usize, RedisError> {
        let res = match v {
            Value::Null => 0,
            Value::Bool(v) => mem::size_of_val(v),
            Value::Number(v) => mem::size_of_val(v),
            Value::String(v) => mem::size_of_val(v),
            Value::Array(v) => mem::size_of_val(v),
            Value::Object(v) => mem::size_of_val(v),
        };
        Ok(res)
    }

    fn is_json(&self, key: *mut RedisModuleKey) -> Result<bool, RedisError> {
        match verify_type(key, &REDIS_JSON_TYPE) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}
