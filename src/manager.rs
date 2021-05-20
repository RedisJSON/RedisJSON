use jsonpath_lib::select::select_value::SelectValue;
use serde_json::map::Entry;
use serde_json::{Number, Value};

use redis_module::key::{RedisKeyWritable, RedisKey};
use redis_module::rediserror::RedisError;
use redis_module::Context;

use crate::redisjson::RedisJSON;
use crate::Format;
use crate::REDIS_JSON_TYPE;

use crate::error::Error;
use bson::decode_document;
use std::io::Cursor;

use crate::array_index::ArrayIndex;

use std::mem;

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

pub trait ReadHolder<E: Clone, V: SelectValue> {
    fn get_value(&self) -> Result<Option<&V>, RedisError>;
}

pub trait WriteHolder<E: Clone, V: SelectValue> {
    fn delete(&mut self) -> Result<(), RedisError>;
    fn get_value(&self) -> Result<Option<&mut V>, RedisError>;
    fn set_root(&mut self, v: Option<E>) -> Result<(), RedisError>;
    fn set_value(&mut self, update_info: Vec<UpdateInfo>, v: E) -> Result<bool, RedisError>;
    fn delete_paths(&mut self, paths: Vec<Vec<String>>) -> Result<usize, RedisError>;
    fn incr_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Number, RedisError>;
    fn mult_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Number, RedisError>;
    fn pow_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Number, RedisError>;
    fn bool_toggle(&mut self, paths: Vec<Vec<String>>) -> Result<bool, RedisError>;
    fn str_append(&mut self, paths: Vec<Vec<String>>, val: String) -> Result<usize, RedisError>;
    fn arr_append<I: Iterator<Item = String>>(
        &mut self,
        paths: Vec<Vec<String>>,
        args: I,
    ) -> Result<usize, RedisError>;
    fn arr_insert<I: Iterator<Item = String>>(
        &mut self,
        paths: Vec<Vec<String>>,
        args: I,
        index: i64,
    ) -> Result<usize, RedisError>;
    fn arr_pop(
        &mut self,
        paths: Vec<Vec<String>>,
        index: i64,
    ) -> Result<Option<String>, RedisError>;
    fn arr_trim(
        &mut self,
        paths: Vec<Vec<String>>,
        start: i64,
        stop: i64,
    ) -> Result<usize, RedisError>;
    fn clear(&mut self, paths: Vec<Vec<String>>) -> Result<usize, RedisError>;
}

pub trait Manager {
    type V: SelectValue;
    type E: Clone;
    type WriteHolder: WriteHolder<Self::E, Self::V>;
    type ReadHolder: ReadHolder<Self::E, Self::V>;
    fn open_key_read(&self, ctx: &Context, key: &str) -> Result<Self::ReadHolder, RedisError>;
    fn open_key_write(&self, ctx: &Context, key: &str) -> Result<Self::WriteHolder, RedisError>;
    fn from_str(&self, val: &str, format: Format) -> Result<Self::E, Error>;
    fn get_memory(&self, v: &Self::V) -> Result<usize, RedisError>;
}

fn err_json(value: &Value, expected_value: &'static str) -> Error {
    Error::from(format!(
        "ERR wrong type of path value - expected {} but found {}",
        expected_value,
        RedisJSON::value_name(value)
    ))
}

pub struct KeyHolderWrite {
    key: RedisKeyWritable,
}

impl KeyHolderWrite {
    fn update<F: FnMut(Value) -> Result<Option<Value>, Error>>(
        &self,
        path: &Vec<String>,
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
                            let v = std::mem::replace(&mut vec[x], Value::Null);
                            if let Some(res) = (func)(v)? {
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
        F: FnMut(Value) -> Result<Option<Value>, Error>,
    {
        let mut new = None;
        for p in paths {
            if p.len() == 0 {
                // updating the root require special treatment
                let root = self.get_value().unwrap().unwrap();
                let res = (op_fun)(root.take())?;
                new = res.clone();
                self.set_root(res)?;
            } else {
                self.update(&p, self.get_value().unwrap().unwrap(), |v| {
                    let res = (op_fun)(v)?;
                    new = res.clone();
                    Ok(res)
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
    ) -> Result<Number, RedisError>
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
                Ok(Some(Value::Number(num_res)))
            })?;
            match res {
                None => Err(RedisError::Str("path does not exists")),
                Some(n) => match n {
                    Value::Number(n) => Ok(n),
                    _ => Err(RedisError::Str("return value is not a number")),
                },
            }
        } else {
            Err(RedisError::Str("bad input number"))
        }
    }
}

impl WriteHolder<Value, Value> for KeyHolderWrite {
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
                            Ok(Some(v.clone()))
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
                            Ok(Some(ret))
                        })?;
                    }
                }
            }
        }
        Ok(updated)
    }

    fn set_root(&mut self, v: Option<Value>) -> Result<(), RedisError> {
        match v {
            Some(inner) => self
                .key
                .set_value(&REDIS_JSON_TYPE, RedisJSON { data: inner }),
            None => {
                self.key.delete()?;
                Ok(())
            }
        }
    }

    fn delete_paths(&mut self, paths: Vec<Vec<String>>) -> Result<usize, RedisError> {
        let mut deleted = 0;
        for p in paths {
            self.update(&p, self.get_value().unwrap().unwrap(), |v| {
                if !v.is_null() {
                    deleted += 1; // might delete more than a single value
                }
                Ok(None)
            })?;
        }
        Ok(deleted)
    }

    fn incr_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Number, RedisError> {
        self.do_num_op(paths, num, |i1, i2| i1 + i2, |f1, f2| f1 + f2)
    }

    fn mult_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Number, RedisError> {
        self.do_num_op(paths, num, |i1, i2| i1 * i2, |f1, f2| f1 * f2)
    }

    fn pow_by(&mut self, paths: Vec<Vec<String>>, num: String) -> Result<Number, RedisError> {
        self.do_num_op(paths, num, |i1, i2| i1.pow(i2 as u32), |f1, f2| f1.powf(f2))
    }

    fn bool_toggle(&mut self, paths: Vec<Vec<String>>) -> Result<bool, RedisError> {
        let res = self.do_op(paths, |v| {
            Ok(Some(Value::Bool(v.as_bool().unwrap() ^ true)))
        })?;
        match res {
            None => Err(RedisError::Str("path does not exists")),
            Some(n) => Ok(n.as_bool().unwrap()),
        }
    }

    fn str_append(&mut self, paths: Vec<Vec<String>>, val: String) -> Result<usize, RedisError> {
        let json = serde_json::from_str(&val)?;
        if let Value::String(s) = json {
            let res = self.do_op(paths, |v| {
                let new_value = [v.as_str().unwrap(), s.as_str()].concat();
                Ok(Some(Value::String(new_value)))
            })?;
            match res {
                None => Err(RedisError::Str("path does not exists")),
                Some(n) => Ok(n.as_str().unwrap().len()),
            }
        } else {
            Err(RedisError::String(format!(
                "ERR wrong type of value - expected string but found {}",
                val
            )))
        }
    }

    fn arr_append<I: Iterator<Item = String>>(
        &mut self,
        paths: Vec<Vec<String>>,
        args: I,
    ) -> Result<usize, RedisError> {
        let items: Vec<Value> = args
            .map(|json| serde_json::from_str(&json))
            .collect::<Result<_, _>>()?;

        let res = self.do_op(paths, |mut v| {
            v.as_array_mut().unwrap().append(&mut items.clone());
            Ok(Some(v))
        })?;
        match res {
            None => Err(RedisError::Str("path does not exists")),
            Some(n) => Ok(n.as_array().unwrap().len()),
        }
    }

    fn arr_insert<I: Iterator<Item = String>>(
        &mut self,
        paths: Vec<Vec<String>>,
        args: I,
        index: i64,
    ) -> Result<usize, RedisError> {
        let items: Vec<Value> = args
            .map(|json| serde_json::from_str(&json))
            .collect::<Result<_, _>>()?;

        let res = self.do_op(paths, |mut v| {
            // Verify legal index in bounds
            let len = v.len().unwrap() as i64;
            let index = if index < 0 { len + index } else { index };
            if !(0..=len).contains(&index) {
                return Err("ERR index out of bounds".into());
            }
            let index = index as usize;
            let mut new_value = v.take();
            let curr = new_value.as_array_mut().unwrap();
            curr.splice(index..index, items.clone());
            Ok(Some(new_value))
        })?;
        match res {
            None => Err(RedisError::Str("path does not exists")),
            Some(n) => Ok(n.as_array().unwrap().len()),
        }
    }

    fn arr_pop(
        &mut self,
        paths: Vec<Vec<String>>,
        index: i64,
    ) -> Result<Option<String>, RedisError> {
        let mut res = None;
        self.do_op(paths, |mut v| {
            if let Some(array) = v.as_array() {
                if array.is_empty() {
                    return Ok(Some(v));
                }
                // Verify legel index in bounds
                let len = array.len() as i64;
                let index = if index < 0 {
                    0.max(len + index)
                } else {
                    index.min(len - 1)
                } as usize;

                let mut new_value = v.take();
                let curr = new_value.as_array_mut().unwrap();
                res = Some(curr.remove(index as usize));
                Ok(Some(new_value))
            } else {
                Err(err_json(&v, "array"))
            }
        })?;
        match res {
            None => Ok(None),
            Some(n) => Ok(Some(RedisJSON::serialize(&n, Format::JSON)?)),
        }
    }

    fn arr_trim(
        &mut self,
        paths: Vec<Vec<String>>,
        start: i64,
        stop: i64,
    ) -> Result<usize, RedisError> {
        let res = self.do_op(paths, |mut v| {
            if let Some(array) = v.as_array() {
                let len = array.len() as i64;
                let stop = stop.normalize(len);

                let range = if start > len || start > stop as i64 {
                    0..0 // Return an empty array
                } else {
                    start.normalize(len)..(stop + 1)
                };

                let mut new_value = v.take();
                let curr = new_value.as_array_mut().unwrap();
                curr.rotate_left(range.start);
                curr.resize(range.end - range.start, Value::Null);

                Ok(Some(new_value))
            } else {
                Err(err_json(&v, "array"))
            }
        })?;
        match res {
            None => Err(RedisError::Str("path does not exists")),
            Some(n) => Ok(n.as_array().unwrap().len()),
        }
    }

    fn clear(&mut self, paths: Vec<Vec<String>>) -> Result<usize, RedisError> {
        let mut cleared = 0;
        self.do_op(paths, |v| match v {
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
            _ => Ok(Some(v)),
        })?;
        Ok(cleared)
    }
}

pub struct KeyHolderRead {
    key: RedisKey,
}

impl ReadHolder<Value, Value> for KeyHolderRead {
    fn get_value(&self) -> Result<Option<&Value>, RedisError> {
        let key_value = self.key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?;
        match key_value {
            Some(v) => Ok(Some(&v.data)),
            None => Ok(None),
        }
    }
}

pub struct RedisJsonKeyManager;

impl Manager for RedisJsonKeyManager {
    type WriteHolder = KeyHolderWrite;
    type ReadHolder = KeyHolderRead;
    type V = Value;
    type E = Value;

    fn open_key_read(&self, ctx: &Context, key: &str) -> Result<KeyHolderRead, RedisError> {
        let key = ctx.open_key(key);
        Ok(KeyHolderRead { key: key })
    }

    fn open_key_write(&self, ctx: &Context, key: &str) -> Result<KeyHolderWrite, RedisError> {
        let key = ctx.open_key_writable(key);
        Ok(KeyHolderWrite { key: key })
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

    fn get_memory(&self, v: &Value) -> Result<usize, RedisError> {
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
}
