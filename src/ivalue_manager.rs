/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use crate::error::Error;
use crate::jsonpath::select_value::{SelectValue, SelectValueType};
use crate::manager::{err_json, err_msg_json_expected, err_msg_json_path_doesnt_exist};
use crate::manager::{Manager, ReadHolder, WriteHolder};
use crate::redisjson::normalize_arr_start_index;
use crate::Format;
use crate::REDIS_JSON_TYPE;
use ijson::object::Entry;
use ijson::{DestructuredMut, DestructuredRef, INumber, IString, IValue, ValueType};
use redis_module::key::{verify_type, RedisKey, RedisKeyWritable};
use redis_module::raw::{RedisModuleKey, Status};
use redis_module::rediserror::RedisError;
use redis_module::{Context, NotifyEvent, RedisString};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::marker::PhantomData;
use std::mem::size_of;

use crate::redisjson::RedisJSON;

use crate::array_index::ArrayIndex;

use bson::{from_document, Document};
use std::io::Cursor;

pub struct IValueKeyHolderWrite<'a> {
    key: RedisKeyWritable,
    key_name: RedisString,
    val: Option<&'a mut RedisJSON<IValue>>,
}

///
/// Replaces a value at a given `path`, starting from `root`
///
/// The new value is the value returned from `func`, which is called on the current value.
///
/// If the returned value from `func` is [`None`], the current value is removed.
/// If the returned value from `func` is [`Err`], the current value remains (although it could be modified by `func`)
///
fn replace<F: FnMut(&mut IValue) -> Result<Option<IValue>, Error>>(
    path: &[String],
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
                        let v = e.get_mut();
                        match (func)(v) {
                            Ok(res) => {
                                if let Some(res) = res {
                                    *v = res;
                                } else {
                                    e.remove();
                                }
                            }
                            Err(err) => return Err(err),
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
                            let v = &mut arr.as_mut_slice()[x];
                            match (func)(v) {
                                Ok(res) => {
                                    if let Some(res) = res {
                                        *v = res;
                                    } else {
                                        arr.remove(x);
                                    }
                                }
                                Err(err) => return Err(err),
                            }
                        }
                        return Ok(());
                    }
                    arr.get_mut(x)
                } else {
                    panic!("Array index should have been parsed successfully before reaching here")
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

///
/// Updates a value at a given `path`, starting from `root`
///
/// The value is modified by `func`, which is called on the current value.
/// If the returned value from `func` is [`None`], the current value is removed.
/// If the returned value from `func` is [`Err`], the current value remains (although it could be modified by `func`)
///
fn update<F: FnMut(&mut IValue) -> Result<Option<()>, Error>>(
    path: &[String],
    root: &mut IValue,
    mut func: F,
) -> Result<(), Error> {
    let mut target = root;

    let last_index = path.len().saturating_sub(1);
    for (i, token) in path.iter().enumerate() {
        let target_once = target;
        let is_last = i == last_index;
        let target_opt = match target_once.type_() {
            ValueType::Object => {
                let obj = target_once.as_object_mut().unwrap();
                if is_last {
                    if let Entry::Occupied(mut e) = obj.entry(token) {
                        let v = e.get_mut();
                        match (func)(v) {
                            Ok(res) => {
                                if res.is_none() {
                                    e.remove();
                                }
                            }
                            Err(err) => return Err(err),
                        }
                    }
                    return Ok(());
                }
                obj.get_mut(token.as_str())
            }
            ValueType::Array => {
                let arr = target_once.as_array_mut().unwrap();
                if let Ok(x) = token.parse::<usize>() {
                    if is_last {
                        if x < arr.len() {
                            let v = &mut arr.as_mut_slice()[x];
                            match (func)(v) {
                                Ok(res) => {
                                    if res.is_none() {
                                        arr.remove(x);
                                    }
                                }
                                Err(err) => return Err(err),
                            }
                        }
                        return Ok(());
                    }
                    arr.get_mut(x)
                } else {
                    panic!("Array index should have been parsed successfully before reaching here")
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
    fn do_op<F>(&mut self, paths: &[String], mut op_fun: F) -> Result<(), RedisError>
    where
        F: FnMut(&mut IValue) -> Result<Option<()>, Error>,
    {
        if paths.is_empty() {
            // updating the root require special treatment
            let root = self.get_value().unwrap().unwrap();
            let res = (op_fun)(root);
            match res {
                Ok(res) => {
                    if res.is_none() {
                        root.take();
                    }
                }
                Err(err) => {
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
            self.do_op(&path, |v| {
                let num_res = match (v.get_type(), in_value.as_i64()) {
                    (SelectValueType::Long, Some(num2)) => {
                        let num1 = v.get_long();
                        let res = op1_fun(num1, num2);
                        Ok(res.into())
                    }
                    _ => {
                        let num1 = v.get_double();
                        let num2 = in_value.as_f64().unwrap();
                        INumber::try_from(op2_fun(num1, num2))
                            .map_err(|_| RedisError::Str("result is not a number"))
                    }
                };
                let new_val = IValue::from(num_res?);
                *v = new_val.clone();
                res = Some(new_val);
                Ok(Some(()))
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
                            Err(RedisError::Str("result is not a number"))
                        }
                    } else {
                        Err(RedisError::Str("result is not a number"))
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
            Some(v) => Ok(Some(&mut v.data)),
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
            replace(&path, self.get_value().unwrap().unwrap(), |_v| {
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
        } else {
            update(&path, self.get_value().unwrap().unwrap(), |val| {
                if val.is_object() {
                    let o = val.as_object_mut().unwrap();
                    if !o.contains_key(key) {
                        updated = true;
                        o.insert(key.to_string(), v.take());
                    }
                }
                Ok(Some(()))
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
        self.do_num_op(path, num, i64::wrapping_add, |f1, f2| f1 + f2)
    }

    fn mult_by(&mut self, path: Vec<String>, num: &str) -> Result<Number, RedisError> {
        self.do_num_op(path, num, i64::wrapping_mul, |f1, f2| f1 * f2)
    }

    fn pow_by(&mut self, path: Vec<String>, num: &str) -> Result<Number, RedisError> {
        self.do_num_op(path, num, |i1, i2| i1.pow(i2 as u32), f64::powf)
    }

    fn bool_toggle(&mut self, path: Vec<String>) -> Result<bool, RedisError> {
        let mut res = None;
        self.do_op(&path, |v| {
            if let DestructuredMut::Bool(mut bool_mut) = v.destructure_mut() {
                //Using DestructuredMut in order to modify a `Bool` variant
                let val = bool_mut.get() ^ true;
                bool_mut.set(val);
                res = Some(val);
            }
            Ok(Some(()))
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
            self.do_op(&path, |v| {
                let v_str = v.as_string_mut().unwrap();
                let new_str = [v_str.as_str(), s.as_str()].concat();
                res = Some(new_str.len());
                *v_str = IString::intern(&new_str);
                Ok(Some(()))
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
        self.do_op(&path, |v| {
            let arr = v.as_array_mut().unwrap();
            for a in &args {
                arr.push(a.clone());
            }
            res = Some(arr.len());
            Ok(Some(()))
        })?;
        match res {
            None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Some(n) => Ok(n),
        }
    }

    fn arr_insert(
        &mut self,
        paths: Vec<String>,
        args: &[IValue],
        index: i64,
    ) -> Result<usize, RedisError> {
        let mut res = None;
        self.do_op(&paths, |v: &mut IValue| {
            // Verify legal index in bounds
            let len = v.len().unwrap() as i64;
            let index = if index < 0 { len + index } else { index };
            if !(0..=len).contains(&index) {
                return Err("ERR index out of bounds".into());
            }
            let mut index = index as usize;
            let curr = v.as_array_mut().unwrap();
            curr.reserve(args.len());
            for a in args {
                curr.insert(index, a.clone());
                index += 1;
            }
            res = Some(curr.len());
            Ok(Some(()))
        })?;
        match res {
            None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Some(l) => Ok(l),
        }
    }

    fn arr_pop(&mut self, path: Vec<String>, index: i64) -> Result<Option<String>, RedisError> {
        let mut res = None;
        self.do_op(&path, |v| {
            if let Some(array) = v.as_array_mut() {
                if array.is_empty() {
                    return Ok(Some(()));
                }
                // Verify legel index in bounds
                let len = array.len() as i64;
                let index = normalize_arr_start_index(index, len) as usize;
                res = Some(array.remove(index).unwrap());
                Ok(Some(()))
            } else {
                Err(err_json(v, "array"))
            }
        })?;
        match res {
            None => Ok(None),
            Some(n) => Ok(Some(Self::serialize(&n, Format::JSON)?)),
        }
    }

    fn arr_trim(&mut self, path: Vec<String>, start: i64, stop: i64) -> Result<usize, RedisError> {
        let mut res = None;
        self.do_op(&path, |v| {
            if let Some(array) = v.as_array_mut() {
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

                array.rotate_left(range.start);
                array.truncate(range.end - range.start);
                res = Some(array.len());
                Ok(Some(()))
            } else {
                Err(err_json(v, "array"))
            }
        })?;
        match res {
            None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Some(l) => Ok(l),
        }
    }

    fn clear(&mut self, path: Vec<String>) -> Result<usize, RedisError> {
        let mut cleared = 0;
        self.do_op(&path, |v| match v.type_() {
            ValueType::Object => {
                let obj = v.as_object_mut().unwrap();
                obj.clear();
                cleared += 1;
                Ok(Some(()))
            }
            ValueType::Array => {
                let arr = v.as_array_mut().unwrap();
                arr.clear();
                cleared += 1;
                Ok(Some(()))
            }
            ValueType::Number => {
                *v = IValue::from(0);
                cleared += 1;
                Ok(Some(()))
            }
            _ => Ok(Some(())),
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
        key_value.map_or(Ok(None), |v| Ok(Some(&v.data)))
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

    fn from_str(&self, val: &str, format: Format, limit_depth: bool) -> Result<Self::O, Error> {
        match format {
            Format::JSON => {
                let mut deserializer = serde_json::Deserializer::from_str(val);
                if !limit_depth {
                    deserializer.disable_recursion_limit();
                }
                IValue::deserialize(&mut deserializer).map_err(|e| e.into())
            }
            Format::BSON => from_document(
                Document::from_reader(&mut Cursor::new(val.as_bytes()))
                    .map_err(|e| e.to_string())?,
            )
            .map_or_else(
                |e| Err(e.to_string().into()),
                |docs: Document| {
                    let v = if docs.is_empty() {
                        IValue::NULL
                    } else {
                        docs.iter().next().map_or_else(
                            || IValue::NULL,
                            |(_, b)| {
                                let v: serde_json::Value = b.clone().into();
                                let mut out = serde_json::Serializer::new(Vec::new());
                                v.serialize(&mut out).unwrap();
                                self.from_str(
                                    &String::from_utf8(out.into_inner()).unwrap(),
                                    Format::JSON,
                                    limit_depth,
                                )
                                .unwrap()
                            },
                        )
                    };
                    Ok(v)
                },
            ),
        }
    }

    ///
    /// following https://github.com/Diggsey/ijson/issues/23#issuecomment-1377270111
    ///
    fn get_memory(v: &Self::V) -> Result<usize, RedisError> {
        Ok(match v.destructure_ref() {
            DestructuredRef::Null | DestructuredRef::Bool(_) => 0,
            DestructuredRef::Number(num) => {
                const STATIC_LO: i32 = -1 << 7; // INumber::STATIC_LOWER
                const STATIC_HI: i32 = 0b11 << 7; // INumber::STATIC_UPPER
                const SHORT_LO: i32 = -1 << 23; // INumber::SHORT_LOWER
                const SHORT_HI: i32 = 1 << 23; // INumber::SHORT_UPPER

                if num.has_decimal_point() {
                    16 // 64bit float
                } else if &INumber::from(STATIC_LO) <= num && num < &INumber::from(STATIC_HI) {
                    0 // 8bit
                } else if &INumber::from(SHORT_LO) <= num && num < &INumber::from(SHORT_HI) {
                    4 // 24bit
                } else {
                    16 // 64bit
                }
            }
            DestructuredRef::String(s) => s.len(),
            DestructuredRef::Array(arr) => match arr.capacity() {
                0 => 0,
                capacity => {
                    arr.into_iter() // IValueManager::get_memory() always returns OK, safe to unwrap here
                        .map(|val| Self::get_memory(val).unwrap())
                        .sum::<usize>()
                        + (capacity + 2) * size_of::<usize>()
                }
            },
            DestructuredRef::Object(obj) => match obj.capacity() {
                0 => 0,
                capacity => {
                    obj.into_iter() // IValueManager::get_memory() always returns OK, safe to unwrap here
                        .map(|(s, val)| s.len() + Self::get_memory(val).unwrap())
                        .sum::<usize>()
                    + (capacity * 3 + 2) * size_of::<usize>()
                }
            },
        } + size_of::<IValue>())
    }


    fn is_json(&self, key: *mut RedisModuleKey) -> Result<bool, RedisError> {
        match verify_type(key, &REDIS_JSON_TYPE) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

// a unit test for get_memory
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_memory() {
        let json = r#"{
                            "a": 100.12, 
                            "b": "foo", 
                            "c": true, 
                            "d": 126, 
                            "e": -112, 
                            "f": 7388608,
                            "g": -6388608,
                            "h": 9388608,
                            "i": -9485608,
                            "j": [],
                            "k": {},
                            "l": [1, "asas", {"a": 1}],
                            "m": {"t": "f"}
                        }"#;
        let value = serde_json::from_str(json).unwrap();
        let res = RedisIValueJsonKeyManager::get_memory(&value).unwrap();
        assert_eq!(res, 903);
    }
}
