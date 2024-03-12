/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use crate::error::Error;
use crate::manager::{err_json, err_msg_json_expected, err_msg_json_path_doesnt_exist};
use crate::manager::{Manager, ReadHolder, WriteHolder};
use crate::redisjson::normalize_arr_start_index;
use crate::Format;
use crate::REDIS_JSON_TYPE;
use bson::{from_document, Document};
use ijson::object::Entry;
use ijson::{DestructuredMut, INumber, IObject, IString, IValue, ValueType};
use redis_module::key::{verify_type, KeyFlags, RedisKey, RedisKeyWritable};
use redis_module::raw::{RedisModuleKey, Status};
use redis_module::rediserror::RedisError;
use redis_module::{Context, NotifyEvent, RedisResult, RedisString};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::io::Cursor;
use std::marker::PhantomData;
use std::mem::size_of;

use crate::redisjson::RedisJSON;

use crate::array_index::ArrayIndex;

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
            ValueType::Object => {
                let obj = target_once.as_object_mut().unwrap();
                if is_last {
                    if let Entry::Occupied(mut e) = obj.entry(token) {
                        let v = e.get_mut();
                        if let Some(res) = (func)(v)? {
                            *v = res;
                        } else {
                            e.remove();
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
                            if let Some(res) = (func)(v)? {
                                *v = res;
                            } else {
                                arr.remove(x);
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
                let num_res = match (
                    v.as_number().unwrap().has_decimal_point(),
                    in_value.as_i64(),
                ) {
                    (false, Some(num2)) => Ok(((op1_fun)(v.to_i64().unwrap(), num2)).into()),
                    _ => {
                        let num1 = v.to_f64().unwrap();
                        let num2 = in_value.as_f64().unwrap();
                        INumber::try_from((op2_fun)(num1, num2))
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
            replace(&path, self.get_value()?.unwrap(), |_v| {
                updated = true;
                Ok(Some(v.take()))
            })?;
        }
        Ok(updated)
    }

    fn merge_value(&mut self, path: Vec<String>, v: IValue) -> Result<bool, RedisError> {
        let mut updated = false;
        if path.is_empty() {
            merge(self.get_value()?.unwrap(), &v);
            // update the root
            updated = true;
        } else {
            replace(&path, self.get_value()?.unwrap(), |current| {
                updated = true;
                merge(current, &v);
                Ok(Some(current.take()))
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
        self.do_num_op(path, num, |i1, i2| i1 + i2, |f1, f2| f1 + f2)
    }

    fn mult_by(&mut self, path: Vec<String>, num: &str) -> Result<Number, RedisError> {
        self.do_num_op(path, num, |i1, i2| i1 * i2, |f1, f2| f1 * f2)
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
        res.ok_or_else(|| RedisError::String(err_msg_json_path_doesnt_exist()))
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
            res.ok_or_else(|| RedisError::String(err_msg_json_path_doesnt_exist()))
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
        res.ok_or_else(|| RedisError::String(err_msg_json_path_doesnt_exist()))
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
        res.ok_or_else(|| RedisError::String(err_msg_json_path_doesnt_exist()))
    }

    fn arr_pop<C: FnOnce(Option<&IValue>) -> RedisResult>(
        &mut self,
        path: Vec<String>,
        index: i64,
        serialize_callback: C,
    ) -> RedisResult {
        let mut res = None;
        self.do_op(&path, |v| {
            if let Some(array) = v.as_array_mut() {
                if array.is_empty() {
                    return Ok(Some(()));
                }
                // Verify legal index in bounds
                let len = array.len() as i64;
                let index = normalize_arr_start_index(index, len) as usize;
                res = Some(array.remove(index).unwrap());
                Ok(Some(()))
            } else {
                Err(err_json(v, "array"))
            }
        })?;
        serialize_callback(res.as_ref())
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
        res.ok_or_else(|| RedisError::String(err_msg_json_path_doesnt_exist()))
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

fn merge(doc: &mut IValue, patch: &IValue) {
    if !patch.is_object() {
        *doc = patch.clone();
        return;
    }

    if !doc.is_object() {
        *doc = IObject::new().into();
    }
    let map = doc.as_object_mut().unwrap();
    for (key, value) in patch.as_object().unwrap() {
        if value.is_null() {
            map.remove(key.as_str());
        } else {
            merge(map.entry(key.as_str()).or_insert(IValue::NULL), value);
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

    fn open_key_read_with_flags(
        &self,
        ctx: &Context,
        key: &RedisString,
        flags: KeyFlags,
    ) -> Result<Self::ReadHolder, RedisError> {
        let key = ctx.open_key_with_flags(key, flags);
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
            Format::JSON | Format::STRING => {
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
    fn get_memory(&self, v: &Self::V) -> Result<usize, RedisError> {
        let res = size_of::<IValue>()
            + match v.type_() {
                ValueType::Null | ValueType::Bool => 0,
                ValueType::Number => {
                    let num = v.as_number().unwrap();
                    if num.has_decimal_point() {
                        // 64bit float
                        16
                    } else if num >= &INumber::from(-128) && num <= &INumber::from(383) {
                        // 8bit
                        0
                    } else if num > &INumber::from(-8_388_608) && num <= &INumber::from(8_388_607) {
                        // 24bit
                        4
                    } else {
                        // 64bit
                        16
                    }
                }
                ValueType::String => v.as_string().unwrap().len(),
                ValueType::Array => {
                    let arr = v.as_array().unwrap();
                    let capacity = arr.capacity();
                    if capacity == 0 {
                        0
                    } else {
                        size_of::<usize>() * (capacity + 2)
                            + arr
                                .into_iter()
                                .map(|v| self.get_memory(v).unwrap())
                                .sum::<usize>()
                    }
                }
                ValueType::Object => {
                    let val = v.as_object().unwrap();
                    let capacity = val.capacity();
                    if capacity == 0 {
                        0
                    } else {
                        size_of::<usize>() * (capacity * 3 + 2)
                            + val
                                .into_iter()
                                .map(|(s, v)| s.len() + self.get_memory(v).unwrap())
                                .sum::<usize>()
                    }
                }
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

// a unit test for get_memory
#[cfg(test)]
mod tests {
    use super::*;

    static single_thread_test_mutex: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_get_memory() {
        let _guard = single_thread_test_mutex.lock();

        let manager = RedisIValueJsonKeyManager {
            phantom: PhantomData,
        };
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
        let res = manager.get_memory(&value).unwrap();
        assert_eq!(res, 903);
    }

    /// Tests the deserialiser of IValue for a string with unicode
    /// characters, to ensure that the deserialiser can handle
    /// unicode characters well.
    #[test]
    fn test_unicode_characters() {
        let _guard = single_thread_test_mutex.lock();

        let json = r#""\u00a0\u00a0\u00a0\u00a0\u00a0\u00a0\u00a0""#;
        let value: IValue = serde_json::from_str(json).expect("IValue parses fine.");
        assert_eq!(
            value.as_string().unwrap(),
            "\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}"
        );

        let json = r#"{"\u00a0\u00a0\u00a0\u00a0\u00a0\u00a0\u00a0":"\u00a0\u00a0\u00a0\u00a0\u00a0\u00a0\u00a0"}"#;
        let value: IValue = serde_json::from_str(json).expect("IValue parses fine.");
        assert_eq!(
            value
                .as_object()
                .unwrap()
                .get("\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}")
                .unwrap()
                .as_string()
                .unwrap(),
            "\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}"
        );
    }
}
