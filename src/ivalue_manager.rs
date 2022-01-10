use crate::error::Error;
use crate::manager::{err_json, err_msg_json_expected, err_msg_json_path_doesnt_exist};
use crate::manager::{Manager, ReadHolder, WriteHolder};
use crate::redisjson::normalize_arr_start_index;
use crate::Format;
use crate::REDIS_JSON_TYPE;
use ijson::object::Entry;
use ijson::{INumber, IValue, ValueType};
use redis_module::key::{verify_type, RedisKey, RedisKeyWritable};
use redis_module::raw::{RedisModuleKey, Status};
use redis_module::rediserror::RedisError;
use redis_module::{Context, NotifyEvent, RedisString};
use serde::Serialize;
use serde_json::Number;
use std::marker::PhantomData;

use crate::redisjson::RedisJSON;

use crate::array_index::ArrayIndex;

use std::mem;

use bson::decode_document;
use std::io::Cursor;

pub struct IValueKeyHolderWrite<'a> {
    key: RedisKeyWritable,
    key_name: RedisString,
    val: Option<&'a mut RedisJSON<IValue>>,
}

fn update<F: FnMut(IValue) -> Result<Option<IValue>, Error>>(
    path: &Vec<String>,
    root: &mut IValue,
    mut func: F,
) -> Result<(), Error> {
    let mut target = root;

    let last_index = path.len().saturating_sub(1);
    for (i, token) in path.iter().enumerate() {
        let target_once = target;
        let is_last = i == last_index;
        let target_opt = match target_once.type_() {
            // ValueType::Object(ref mut map) => {
            ValueType::Object => {
                let obj = target_once.as_object_mut().unwrap();
                if is_last {
                    if let Entry::Occupied(mut e) = obj.entry(token) {
                        let v = e.insert(IValue::NULL);
                        if let Some(res) = (func)(v)? {
                            e.insert(res);
                        } else {
                            e.remove();
                        }
                    }
                    return Ok(());
                }
                obj.get_mut(token.as_str())
            }
            // Value::Array(ref mut vec) => {
            ValueType::Array => {
                let arr = target_once.as_array_mut().unwrap();
                if let Ok(x) = token.parse::<usize>() {
                    if is_last {
                        if x < arr.len() {
                            let v = std::mem::replace(&mut arr[x], IValue::NULL);
                            if let Some(res) = (func)(v)? {
                                arr[x] = res;
                            } else {
                                arr.remove(x);
                            }
                        }
                        return Ok(());
                    }
                    arr.get_mut(x)
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

impl<'a> IValueKeyHolderWrite<'a> {
    fn do_op<F>(&mut self, paths: Vec<String>, mut op_fun: F) -> Result<(), RedisError>
    where
        F: FnMut(IValue) -> Result<Option<IValue>, Error>,
    {
        if paths.is_empty() {
            // updating the root require special treatment
            let root = self.get_value().unwrap().unwrap();
            let res = (op_fun)(root.take())?;
            self.set_root(res)?;
        } else {
            update(&paths, self.get_value().unwrap().unwrap(), op_fun)?;
        }

        Ok(())
    }

    fn do_num_op<F1, F2>(
        &mut self,
        path: Vec<String>,
        num: &str,
        mut op1_fun: F1,
        mut op2_fun: F2,
    ) -> Result<Number, RedisError>
    where
        F1: FnMut(i64, i64) -> i64,
        F2: FnMut(f64, f64) -> f64,
    {
        let in_value = &serde_json::from_str(num)?;
        if let serde_json::Value::Number(in_value) = in_value {
            let mut res = None;
            self.do_op(path, |v| {
                let num_res = match (
                    v.as_number().unwrap().has_decimal_point(),
                    in_value.as_i64(),
                ) {
                    (false, Some(num2)) => ((op1_fun)(v.to_i64().unwrap(), num2)).into(),
                    _ => {
                        let num1 = v.to_f64().unwrap();
                        let num2 = in_value.as_f64().unwrap();
                        INumber::try_from((op2_fun)(num1, num2)).unwrap()
                    }
                };
                res = Some(IValue::from(num_res));
                Ok(res.clone())
            })?;
            match res {
                None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
                Some(n) => {
                    if let Some(n) = n.as_number() {
                        if !n.has_decimal_point() {
                            Ok(n.to_i64().unwrap().into())
                        } else if let Some(f) = n.to_f64() {
                            Ok(serde_json::Number::from_f64(f).unwrap())
                        } else {
                            Err(RedisError::Str("return value is not a number"))
                        }
                    } else {
                        Err(RedisError::Str("return value is not a number"))
                    }
                }
            }
        } else {
            Err(RedisError::Str("bad input number"))
        }
    }

    fn get_json_holder(&mut self) -> Result<(), RedisError> {
        if self.val.is_none() {
            self.val = self.key.get_value::<RedisJSON<IValue>>(&REDIS_JSON_TYPE)?;
        }
        Ok(())
    }

    fn set_root(&mut self, v: Option<IValue>) -> Result<(), RedisError> {
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

    fn serialize(results: &IValue, format: Format) -> Result<String, Error> {
        let res = match format {
            Format::JSON => serde_json::to_string(results)?,
            Format::BSON => return Err("ERR Soon to come...".into()), //results.into() as Bson,
        };
        Ok(res)
    }
}

impl<'a> WriteHolder<IValue, IValue> for IValueKeyHolderWrite<'a> {
    fn apply_changes(&mut self, ctx: &Context, command: &str) -> Result<(), RedisError> {
        if ctx.notify_keyspace_event(NotifyEvent::MODULE, command, &self.key_name) != Status::Ok {
            Err(RedisError::Str("failed notify key space event"))
        } else {
            ctx.replicate_verbatim();
            Ok(())
        }
    }

    fn delete(&mut self) -> Result<(), RedisError> {
        self.key.delete()?;
        Ok(())
    }

    fn get_value(&mut self) -> Result<Option<&mut IValue>, RedisError> {
        self.get_json_holder()?;

        match &mut self.val {
            Some(v) => Ok(Some(&mut (*v).data)),
            None => Ok(None),
        }
    }

    fn set_value(&mut self, path: Vec<String>, mut v: IValue) -> Result<bool, RedisError> {
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

    fn dict_add(
        &mut self,
        path: Vec<String>,
        key: &str,
        mut v: IValue,
    ) -> Result<bool, RedisError> {
        let mut updated = false;
        if path.is_empty() {
            // update the root
            let root = self.get_value().unwrap().unwrap();
            if let Some(o) = root.as_object_mut() {
                if !o.contains_key(key) {
                    updated = true;
                    o.insert(key.to_string(), v.take());
                }
            }
            let val = root.take();
            self.set_root(Some(val))?;
        } else {
            update(&path, self.get_value().unwrap().unwrap(), |mut val| {
                if let Some(o) = val.as_object_mut() {
                    if !o.contains_key(key) {
                        updated = true;
                        o.insert(key.to_string(), v.take());
                    }
                }
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
        self.do_num_op(path, num, |i1, i2| i1 + i2, |f1, f2| f1 + f2)
    }

    fn mult_by(&mut self, path: Vec<String>, num: &str) -> Result<Number, RedisError> {
        self.do_num_op(path, num, |i1, i2| i1 * i2, |f1, f2| f1 * f2)
    }

    fn pow_by(&mut self, path: Vec<String>, num: &str) -> Result<Number, RedisError> {
        self.do_num_op(path, num, |i1, i2| i1.pow(i2 as u32), |f1, f2| f1.powf(f2))
    }

    fn bool_toggle(&mut self, path: Vec<String>) -> Result<bool, RedisError> {
        let mut res = None;
        self.do_op(path, |v| {
            let val = v.to_bool().unwrap() ^ true;
            res = Some(val);
            Ok(Some(IValue::from(val)))
        })?;
        match res {
            None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Some(n) => Ok(n),
        }
    }

    fn str_append(&mut self, path: Vec<String>, val: String) -> Result<usize, RedisError> {
        let json = serde_json::from_str(&val)?;
        if let serde_json::Value::String(s) = json {
            let mut res = None;
            self.do_op(path, |v| {
                let new_str = [v.as_string().unwrap(), s.as_str()].concat();
                res = Some(new_str.len());
                Ok(Some(IValue::from(new_str)))
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

    fn arr_append(&mut self, path: Vec<String>, args: Vec<IValue>) -> Result<usize, RedisError> {
        let mut res = None;
        self.do_op(path, |mut v| {
            let arr = v.as_array_mut().unwrap();
            for a in args.iter() {
                arr.push(a.clone());
            }
            res = Some(arr.len());
            Ok(Some(v))
        })?;
        match res {
            None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Some(n) => Ok(n),
        }
    }

    fn arr_insert(
        &mut self,
        paths: Vec<String>,
        args: &Vec<IValue>,
        index: i64,
    ) -> Result<usize, RedisError> {
        let mut res = None;
        self.do_op(paths, |mut v| {
            // Verify legal index in bounds
            let len = v.len().unwrap() as i64;
            let index = if index < 0 { len + index } else { index };
            if !(0..=len).contains(&index) {
                return Err("ERR index out of bounds".into());
            }
            let mut index = index as usize;
            let mut new_value = v.take();
            let curr = new_value.as_array_mut().unwrap();
            curr.reserve(args.len());
            for a in args {
                curr.insert(index, a.clone());
                index += 1;
            }
            res = Some(curr.len());
            Ok(Some(new_value))
        })?;
        match res {
            None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Some(l) => Ok(l),
        }
    }

    fn arr_pop(&mut self, path: Vec<String>, index: i64) -> Result<Option<String>, RedisError> {
        let mut res = None;
        self.do_op(path, |mut v| {
            if let Some(array) = v.as_array() {
                if array.is_empty() {
                    return Ok(Some(v));
                }
                // Verify legel index in bounds
                let len = array.len() as i64;
                let index = normalize_arr_start_index(index, len) as usize;

                let mut new_value = v.take();
                let curr = new_value.as_array_mut().unwrap();
                res = Some(curr.remove(index as usize).unwrap());
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
        self.do_op(path, |mut v| {
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
                curr.truncate(range.end - range.start);
                res = Some(curr.len());
                Ok(Some(new_value))
            } else {
                Err(err_json(&v, "array"))
            }
        })?;
        match res {
            None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Some(l) => Ok(l),
        }
    }

    fn clear(&mut self, path: Vec<String>) -> Result<usize, RedisError> {
        let mut cleared = 0;
        self.do_op(path, |mut v| match v.type_() {
            ValueType::Object => {
                let obj = v.as_object_mut().unwrap();
                obj.clear();
                cleared += 1;
                Ok(Some(v))
            }
            ValueType::Array => {
                let arr = v.as_array_mut().unwrap();
                arr.clear();
                cleared += 1;
                Ok(Some(v))
            }
            ValueType::Number => {
                cleared += 1;
                Ok(Some(IValue::from(0)))
            }
            _ => Ok(Some(v)),
        })?;
        Ok(cleared)
    }
}

pub struct IValueKeyHolderRead {
    key: RedisKey,
}

impl ReadHolder<IValue> for IValueKeyHolderRead {
    fn get_value(&self) -> Result<Option<&IValue>, RedisError> {
        let key_value = self.key.get_value::<RedisJSON<IValue>>(&REDIS_JSON_TYPE)?;
        match key_value {
            Some(v) => Ok(Some(&v.data)),
            None => Ok(None),
        }
    }
}

pub struct RedisIValueJsonKeyManager<'a> {
    pub phantom: PhantomData<&'a u64>,
}

impl<'a> Manager for RedisIValueJsonKeyManager<'a> {
    type WriteHolder = IValueKeyHolderWrite<'a>;
    type ReadHolder = IValueKeyHolderRead;
    type V = IValue;
    type O = IValue;

    fn open_key_read(
        &self,
        ctx: &Context,
        key: &RedisString,
    ) -> Result<IValueKeyHolderRead, RedisError> {
        let key = ctx.open_key(key);
        Ok(IValueKeyHolderRead { key })
    }

    fn open_key_write(
        &self,
        ctx: &Context,
        key: RedisString,
    ) -> Result<IValueKeyHolderWrite<'a>, RedisError> {
        let key_ptr = ctx.open_key_writable(&key);
        Ok(IValueKeyHolderWrite {
            key: key_ptr,
            key_name: key,
            val: None,
        })
    }

    fn from_str(&self, val: &str, format: Format) -> Result<Self::O, Error> {
        match format {
            Format::JSON => Ok(serde_json::from_str(val)?),
            Format::BSON => decode_document(&mut Cursor::new(val.as_bytes()))
                .map(|docs| {
                    let v = if !docs.is_empty() {
                        docs.iter().next().map_or_else(
                            || IValue::NULL,
                            |(_, b)| {
                                let v: serde_json::Value = b.clone().into();
                                let mut out = serde_json::Serializer::new(Vec::new());
                                v.serialize(&mut out).unwrap();
                                self.from_str(
                                    &String::from_utf8(out.into_inner()).unwrap(),
                                    Format::JSON,
                                )
                                .unwrap()
                            },
                        )
                    } else {
                        IValue::NULL
                    };
                    Ok(v)
                })
                .unwrap_or_else(|e| Err(e.to_string().into())),
        }
    }

    fn get_memory(&self, v: &Self::V) -> Result<usize, RedisError> {
        // todo: implement
        let res = match v.type_() {
            ValueType::Null => 0,
            ValueType::Bool => mem::size_of_val(v),
            ValueType::Number => mem::size_of_val(v),
            ValueType::String => mem::size_of_val(v),
            ValueType::Array => mem::size_of_val(v),
            ValueType::Object => mem::size_of_val(v),
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
