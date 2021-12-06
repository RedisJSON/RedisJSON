use jsonpath_lib::select::select_value::SelectValue;
// use serde_json::map::Entry;
use serde_json::{Number, Value};

use redis_module::key::{verify_type, RedisKey, RedisKeyWritable};
use redis_module::raw::{RedisModuleKey, Status};
use redis_module::rediserror::RedisError;
use redis_module::{Context, NotifyEvent, RedisString};
use ijson::{IValue, IArray, INumber, ValueType, IObject};
use ijson::object::Entry;

use std::marker::PhantomData;

use crate::redisjson::{normalize_arr_start_index, RedisJSON};
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

pub enum UpdateInfo {
    SUI(SetUpdateInfo),
    AUI(AddUpdateInfo),
}

pub trait ReadHolder<V: SelectValue> {
    fn get_value(&self) -> Result<Option<&V>, RedisError>;
}

pub trait WriteHolder<O: Clone, V: SelectValue> {
    fn delete(&mut self) -> Result<(), RedisError>;
    fn get_value(&mut self) -> Result<Option<&mut V>, RedisError>;
    fn set_value(&mut self, path: Vec<String>, v: O) -> Result<bool, RedisError>;
    fn dict_add(&mut self, path: Vec<String>, key: &str, v: O) -> Result<bool, RedisError>;
    fn delete_path(&mut self, path: Vec<String>) -> Result<bool, RedisError>;
    fn incr_by(&mut self, path: Vec<String>, num: &str) -> Result<Number, RedisError>;
    fn mult_by(&mut self, path: Vec<String>, num: &str) -> Result<Number, RedisError>;
    fn pow_by(&mut self, path: Vec<String>, num: &str) -> Result<Number, RedisError>;
    fn bool_toggle(&mut self, path: Vec<String>) -> Result<bool, RedisError>;
    fn str_append(&mut self, path: Vec<String>, val: String) -> Result<usize, RedisError>;
    fn arr_append(&mut self, path: Vec<String>, args: Vec<O>) -> Result<usize, RedisError>;
    fn arr_insert(
        &mut self,
        path: Vec<String>,
        args: &Vec<O>,
        index: i64,
    ) -> Result<usize, RedisError>;
    fn arr_pop(&mut self, path: Vec<String>, index: i64) -> Result<Option<String>, RedisError>;
    fn arr_trim(&mut self, path: Vec<String>, start: i64, stop: i64) -> Result<usize, RedisError>;
    fn clear(&mut self, path: Vec<String>) -> Result<usize, RedisError>;
    fn apply_changes(&mut self, ctx: &Context, command: &str) -> Result<(), RedisError>;
}

pub trait Manager {
    /* V - SelectValue that the json path library can work on
     * O - SelectValue Holder
     * Naiv implementation is that V and O are from the same type but its not
     * always possible so they are seperated
     */
    type V: SelectValue;
    type O: Clone;
    type WriteHolder: WriteHolder<Self::O, Self::V>;
    type ReadHolder: ReadHolder<Self::V>;
    fn open_key_read(
        &self,
        ctx: &Context,
        key: &RedisString,
    ) -> Result<Self::ReadHolder, RedisError>;
    fn open_key_write(
        &self,
        ctx: &Context,
        key: RedisString,
    ) -> Result<Self::WriteHolder, RedisError>;
    fn from_str(&self, val: &str, format: Format) -> Result<Self::O, Error>;
    fn get_memory(&self, v: &Self::V) -> Result<usize, RedisError>;
    fn is_json(&self, key: *mut RedisModuleKey) -> Result<bool, RedisError>;
}

fn err_json(value: &Value, expected_value: &'static str) -> Error {
    Error::from(err_msg_json_expected(
        expected_value,
        RedisJSON::value_name(value),
    ))
}

pub(crate) fn err_msg_json_expected<'a>(expected_value: &'static str, found: &str) -> String {
    format!(
        "WRONGTYPE wrong type of path value - expected {} but found {}",
        expected_value, found
    )
}

pub(crate) fn err_msg_json_path_doesnt_exist_with_param(path: &str) -> String {
    format!("ERR Path '{}' does not exist", path)
}

pub(crate) fn err_msg_json_path_doesnt_exist() -> String {
    "ERR Path does not exist".to_string()
}

pub(crate) fn err_msg_json_path_doesnt_exist_with_param_or(path: &str, or: &str) -> String {
    format!("ERR Path '{}' does not exist or {}", path, or)
}

pub struct KeyHolderWrite<'a> {
    key: RedisKeyWritable,
    key_name: RedisString,
    val: Option<&'a mut RedisJSON>,
}

fn update<F: FnMut(IValue) -> Result<Option<IValue>, Error>>(
    path: &Vec<String>,
    root: &mut IValue,
    mut func: F,
) -> Result<(), Error> {
    // let mut target = root;

    // let last_index = path.len().saturating_sub(1);
    // for (i, token) in path.iter().enumerate() {
    //     let target_once = target;
    //     let is_last = i == last_index;
    //     let target_opt = 
    //          if target_once.is_object() {
    //             let map = target_once.as_object_mut().unwrap();
    //             if is_last {
    //                 if let Entry::Occupied(mut e) = map.entry(token) {
    //                     let v = e.insert(IValue::NULL);
    //                     if let Some(res) = (func)(v)? {
    //                         e.insert(res);
    //                     } else {
    //                         e.remove();
    //                     }
    //                 }
    //                 return Ok(());
    //             }
    //             map.get_mut(token)
    //         } else if target_once.is_array() {
    //             let vec = target_once.as_array_mut().unwrap();
    //             if let Ok(x) = token.parse::<usize>() {
    //                 if is_last {
    //                     if x < vec.len() {
    //                         let v = std::mem::replace(&mut vec[x], IValue::NULL);
    //                         if let Some(res) = (func)(v)? {
    //                             vec[x] = res;
    //                         } else {
    //                             vec.remove(x);
    //                         }
    //                     }
    //                     return Ok(());
    //                 }
    //                 vec.get_mut(x)
    //             } else {
    //                 None
    //             }
    //         } else {
    //             None
    //         };

    //     if let Some(t) = target_opt {
    //         target = t;
    //     } else {
    //         break;
    //     }
    // }

    Ok(())
}

impl<'a> KeyHolderWrite<'a> {
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
        if let Value::Number(in_value) = in_value {
            let mut res = None;
            self.do_op(path, |v| {
                let num_res : IValue = match (v.to_i64(), in_value.as_i64()) {
                    (Some(num1), Some(num2)) => ((op1_fun)(num1, num2)).into(),
                    _ => {
                        let num1 = v.to_f64().unwrap();
                        let num2 = in_value.as_f64().unwrap();
                        ((op2_fun)(num1, num2)).into()
                    }
                };
                // res = Some(Value::Number(num_res));
                Ok(Some(num_res))
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
            self.val = self.key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?;
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

impl<'a> WriteHolder<IValue, IValue> for KeyHolderWrite<'a> {
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

    fn dict_add(&mut self, path: Vec<String>, key: &str, mut v: IValue) -> Result<bool, RedisError> {
        let mut updated = false;
        // if path.is_empty() {
        //     // update the root
        //     let root = self.get_value().unwrap().unwrap();
        //     let val = if let Some(mut o) = root.take().as_object_mut() {
        //         if !o.contains_key(key) {
        //             updated = true;
        //             o.insert(key.to_string(), v.take());
        //         }
        //         IValue::Object(o)
        //     } else {
        //         root.take()
        //     };
        //     self.set_root(Some(val))?;
        // } else {
        //     update(&path, self.get_value().unwrap().unwrap(), |val| {
        //         if let Some(mut o) = val.as_object_mut() {
        //             if !o.contains_key(key) {
        //                 updated = true;
        //                 o.insert(key.to_string(), v.take());
        //             }
        //         }
        //         Ok(Some(val))
        //     })?;
        // }
        Ok(updated)
    }

    fn delete_path(&mut self, path: Vec<String>) -> Result<bool, RedisError> {
        let mut deleted = false;
        // update(&path, self.get_value().unwrap().unwrap(), |_v| {
        //     deleted = true; // might delete more than a single value
        //     Ok(None)
        // })?;
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
        let json : IValue = serde_json::from_str(&val)?;
        if let Some(s) = json.as_string() {
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

    fn arr_append(&mut self, path: Vec<String>, mut args: Vec<IValue>) -> Result<usize, RedisError> {
        let mut res = None;
        // self.do_op(path, |mut v| {
        //     let arr = v.as_array_mut().unwrap();
        //     arr.append(&mut args);
        //     res = Some(arr.len());
        //     Ok(Some(v))
        // })?;
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
        // self.do_op(paths, |mut v| {
        //     // Verify legal index in bounds
        //     let len = v.len().unwrap() as i64;
        //     let index = if index < 0 { len + index } else { index };
        //     if !(0..=len).contains(&index) {
        //         return Err("ERR index out of bounds".into());
        //     }
        //     let index = index as usize;
        //     let mut new_value = v.take();
        //     let curr = new_value.as_array_mut().unwrap();
        //     curr.splice(index..index, args.clone());
        //     res = Some(curr.len());
        //     Ok(Some(new_value))
        // })?;
        match res {
            None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Some(l) => Ok(l),
        }
    }

    fn arr_pop(&mut self, path: Vec<String>, index: i64) -> Result<Option<String>, RedisError> {
        // let mut res = None;
        // self.do_op(path, |mut v| {
        //     if let Some(array) = v.as_array() {
        //         if array.is_empty() {
        //             return Ok(Some(v));
        //         }
        //         // Verify legel index in bounds
        //         let len = array.len() as i64;
        //         let index = normalize_arr_start_index(index, len) as usize;

        //         let mut new_value = v.take();
        //         let curr = new_value.as_array_mut().unwrap();
        //         res = Some(curr.remove(index as usize));
        //         Ok(Some(new_value))
        //     } else {
        //         Err(err_json(&v, "array"))
        //     }
        // })?;
        // match res {
        //     None => Ok(None),
        //     Some(n) => Ok(Some(RedisJSON::serialize(&n, Format::JSON)?)),
        // }

        Ok(None)
    }

    fn arr_trim(&mut self, path: Vec<String>, start: i64, stop: i64) -> Result<usize, RedisError> {
        let mut res = None;
        // self.do_op(path, |mut v| {
        //     if let Some(array) = v.as_array() {
        //         let len = array.len() as i64;
        //         let stop = stop.normalize(len);
        //         let start = if start < 0 || start < len {
        //             start.normalize(len)
        //         } else {
        //             stop + 1 //  start >=0 && start >= len
        //         };
        //         let range = if start > stop || len == 0 {
        //             0..0 // Return an empty array
        //         } else {
        //             start..(stop + 1)
        //         };

        //         let mut new_value = v.take();
        //         let curr = new_value.as_array_mut().unwrap();
        //         curr.rotate_left(range.start);
        //         curr.resize(range.end - range.start, Value::Null);
        //         res = Some(curr.len());
        //         Ok(Some(new_value))
        //     } else {
        //         Err(err_json(&v, "array"))
        //     }
        // })?;
        match res {
            None => Err(RedisError::String(err_msg_json_path_doesnt_exist())),
            Some(l) => Ok(l),
        }
    }

    fn clear(&mut self, path: Vec<String>) -> Result<usize, RedisError> {
        let mut cleared = 0;
        // self.do_op(path, |v| {
        //     if let Some(mut obj) = v.as_object_mut() {
        //         obj.clear();
        //         cleared += 1;
        //         Ok(Some(IValue::from(obj)))
        //     } else if let Some(mut arr) = v.as_array_mut() {
        //         arr.clear();
        //         cleared += 1;
        //         Ok(Some(IValue::from(arr)))
        //     } else {
        //         Ok(Some(v))
        //     }
        // })?;
        Ok(cleared)
    }
}

pub struct KeyHolderRead {
    key: RedisKey,
}

impl ReadHolder<IValue> for KeyHolderRead {
    fn get_value(&self) -> Result<Option<&IValue>, RedisError> {
        let key_value = self.key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)?;
        match key_value {
            Some(v) => Ok(Some(&v.data)),
            None => Ok(None),
        }
    }
}

pub struct RedisJsonKeyManager<'a> {
    pub phantom: PhantomData<&'a u64>,
}

impl<'a> Manager for RedisJsonKeyManager<'a> {
    type WriteHolder = KeyHolderWrite<'a>;
    type ReadHolder = KeyHolderRead;
    type V = IValue;
    type O = IValue;

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

    fn from_str(&self, val: &str, format: Format) -> Result<IValue, Error> {
        match format {
            Format::JSON => Ok(serde_json::from_str(val)?),
            Format::BSON => 
                Err("TODO".into()),
            //   decode_document(&mut Cursor::new(val.as_bytes()))
            //     .map(|docs| {
            //         let v = if !docs.is_empty() {
            //             docs.iter()
            //                 .next()
            //                 .map_or_else(|| IValue::NULL, |(_, b)| b.clone().into())
            //         } else {
            //             IValue::NULL
            //         };
            //         Ok(v)
            //     })
            //     .unwrap_or_else(|e| Err(e.to_string().into())),
        }
    }

    fn get_memory(&self, v: &IValue) -> Result<usize, RedisError> {
        // let res = match v.type_() {
        //     IValue::Null => 0,
        //     IValue::Bool(v) => mem::size_of_val(v),
        //     IValue::Number(v) => mem::size_of_val(v),
        //     IValue::String(v) => mem::size_of_val(v),
        //     IValue::Array(v) => mem::size_of_val(v),
        //     IValue::Object(v) => mem::size_of_val(v),
        // };
        // Ok(res)
        Ok(0)
    }

    fn is_json(&self, key: *mut RedisModuleKey) -> Result<bool, RedisError> {
        match verify_type(key, &REDIS_JSON_TYPE) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}
