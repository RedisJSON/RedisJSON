/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use crate::error::Error;
use crate::manager::{err_json, err_msg_json_expected, err_msg_json_path_doesnt_exist};
use crate::manager::{Manager, ReadHolder, WriteHolder};
use crate::redisjson::normalize_arr_start_index;
use crate::Format;
use crate::REDIS_JSON_TYPE;
use bson::{from_document, Document};
use ijson::array::{ArraySliceMut, ArrayTag, IArray};
use ijson::{DestructuredMut, INumber, IObject, IString, IValue};
use json_path::select_value::{SelectValue, SelectValueType};
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

#[derive(Debug)]
pub enum PathValue<'a, 'b: 'a> {
    /// Mutable reference to an IValue
    IValue(&'a mut IValue),
    /// Mutable reference to the IValue array which contains i8s, and its index
    I8(&'b mut IArray, usize),
    /// Mutable reference to the IValue array which contains u8s, and its index
    U8(&'b mut IArray, usize),
    /// Mutable reference to the IValue array which contains i16s, and its index
    I16(&'b mut IArray, usize),
    /// Mutable reference to the IValue array which contains u16s, and its index
    U16(&'b mut IArray, usize),
    /// Mutable reference to the IValue array which contains f16s, and its index
    F16(&'b mut IArray, usize),
    /// Mutable reference to the IValue array which contains bf16s, and its index
    BF16(&'b mut IArray, usize),
    /// Mutable reference to the IValue array which contains i32s, and its index
    I32(&'b mut IArray, usize),
    /// Mutable reference to the IValue array which contains u32s, and its index
    U32(&'b mut IArray, usize),
    /// Mutable reference to the IValue array which contains f32s, and its index
    F32(&'b mut IArray, usize),
    /// Mutable reference to the IValue array which contains i64s, and its index
    I64(&'b mut IArray, usize),
    /// Mutable reference to the IValue array which contains u64s, and its index
    U64(&'b mut IArray, usize),
    /// Mutable reference to the IValue array which contains f64s, and its index
    F64(&'b mut IArray, usize),
}

impl<'a, 'b: 'a> PathValue<'a, 'b> {
    fn get_from_array(array: &'b mut IArray, index: usize) -> Option<Self> {
        if index >= array.len() {
            return None;
        }
        let type_tag = array.as_slice().type_tag();
        let is_heterogeneous = matches!(array.as_mut_slice(), ArraySliceMut::Heterogeneous(_));
        if is_heterogeneous {
            return Some(PathValue::IValue(array.get_mut(index).unwrap()));
        } else {
            match type_tag {
                ArrayTag::I8 => Some(PathValue::I8(array, index)),
                ArrayTag::U8 => Some(PathValue::U8(array, index)),
                ArrayTag::I16 => Some(PathValue::I16(array, index)),
                ArrayTag::U16 => Some(PathValue::U16(array, index)),
                ArrayTag::F16 => Some(PathValue::F16(array, index)),
                ArrayTag::BF16 => Some(PathValue::BF16(array, index)),
                ArrayTag::I32 => Some(PathValue::I32(array, index)),
                ArrayTag::U32 => Some(PathValue::U32(array, index)),
                ArrayTag::F32 => Some(PathValue::F32(array, index)),
                ArrayTag::I64 => Some(PathValue::I64(array, index)),
                ArrayTag::U64 => Some(PathValue::U64(array, index)),
                ArrayTag::F64 => Some(PathValue::F64(array, index)),
                ArrayTag::Heterogeneous => unreachable!(),
            }
        }
    }
}

fn follow_path(path: Vec<String>, root: &mut IValue) -> Option<PathValue<'_, '_>> {
    path.into_iter().try_fold(
        PathValue::IValue(root),
        |target: PathValue<'_, '_>, token: String| {
            let PathValue::IValue(target) = target else {
                return None;
            };

            match target.destructure_mut() {
                DestructuredMut::Object(obj) => obj.get_mut(token.as_str()).map(PathValue::IValue),
                DestructuredMut::Array(array) => {
                    let index = token.parse::<usize>().ok()?;
                    PathValue::get_from_array(array, index)
                }
                _ => None,
            }
        },
    )
}

///
/// Updates a value at a given `path`, starting from `root`
///
/// The value is modified by `func`, which is called on the current value.
/// If the returned value from `func` is [`Err`], the current value remains (although it could be modified by `func`)
///
fn update<F, T>(path: Vec<String>, root: &mut IValue, func: F) -> RedisResult<T>
where
    F: FnOnce(PathValue<'_, '_>) -> RedisResult<T>,
{
    follow_path(path, root).map_or_else(
        || Err(RedisError::String(err_msg_json_path_doesnt_exist())),
        func,
    )
}

///
/// Removes a value at a given `path`, starting from `root`
///
fn remove(mut path: Vec<String>, root: &mut IValue) -> bool {
    let token = path.pop().unwrap();
    follow_path(path, root)
        .and_then(|target| {
            let PathValue::IValue(target) = target else {
                return None;
            };
            match target.destructure_mut() {
                DestructuredMut::Object(obj) => obj.remove(token.as_str()),
                DestructuredMut::Array(arr) => arr.remove(token.parse::<usize>().ok()?),
                _ => None,
            }
        })
        .is_some()
}

impl<'a> IValueKeyHolderWrite<'a> {
    fn do_op<F, T>(&mut self, paths: Vec<String>, op_fun: F) -> RedisResult<T>
    where
        F: FnOnce(PathValue<'_, '_>) -> RedisResult<T>,
    {
        let root = self.get_value()?.unwrap();
        update(paths, root, op_fun)
    }

    fn do_num_op<F1, F2>(
        &mut self,
        path: Vec<String>,
        num: &str,
        op1: F1,
        op2: F2,
    ) -> RedisResult<Number>
    where
        F1: FnOnce(i64, i64) -> i64,
        F2: FnOnce(f64, f64) -> f64,
    {
        let in_value = &serde_json::from_str(num)?;
        use half::{bf16, f16};
        if let serde_json::Value::Number(in_value) = in_value {
            let n = self.do_op(path, |v| match (v, in_value.as_i64()) {
                (PathValue::IValue(v), _) => {
                    let new_val = match (v.get_type(), in_value.as_i64()) {
                        (SelectValueType::Long, Some(num2)) => {
                            let num1 = v.get_long();
                            Ok(op1(num1, num2).into())
                        }
                        _ => {
                            let num1 = v.get_double();
                            let num2 = in_value.as_f64().unwrap();
                            INumber::try_from(op2(num1, num2))
                                .map_err(|_| RedisError::Str("result is not a number"))
                        }
                    }?;
                    *v = IValue::from(new_val.clone());
                    Ok(new_val)
                }
                // SAFETY: index is in bounds and type is checked at creation of PathValue
                (PathValue::I8(num1, index), Some(num2)) => {
                    let num1 = num1
                        .as_mut_slice_of::<i8>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op1(*num1 as i64, num2);
                    *num1 = new_val as i8;
                    Ok(new_val.into())
                }
                (PathValue::U8(num1, index), Some(num2)) => {
                    let num1 = num1
                        .as_mut_slice_of::<u8>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op1(*num1 as i64, num2);
                    *num1 = new_val as u8;
                    Ok(new_val.into())
                }
                (PathValue::I16(num1, index), Some(num2)) => {
                    let num1 = num1
                        .as_mut_slice_of::<i16>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op1(*num1 as i64, num2);
                    *num1 = new_val as i16;
                    Ok(new_val.into())
                }
                (PathValue::U16(num1, index), Some(num2)) => {
                    let num1 = num1
                        .as_mut_slice_of::<u16>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op1(*num1 as i64, num2);
                    *num1 = new_val as u16;
                    Ok(new_val.into())
                }
                (PathValue::F16(num1, index), None) => {
                    let num1 = num1
                        .as_mut_slice_of::<f16>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op2(f64::from(*num1), in_value.as_f64().unwrap());
                    *num1 = f16::from_f64(new_val);
                    Ok(INumber::try_from(*num1)
                        .map_err(|_| RedisError::Str("result is not a number"))?)
                }
                (PathValue::BF16(num1, index), None) => {
                    let num1 = num1
                        .as_mut_slice_of::<bf16>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op2(f64::from(*num1), in_value.as_f64().unwrap());
                    *num1 = bf16::from_f64(new_val);
                    Ok(INumber::try_from(*num1)
                        .map_err(|_| RedisError::Str("result is not a number"))?)
                }
                (PathValue::I32(num1, index), Some(num2)) => {
                    let num1 = num1
                        .as_mut_slice_of::<i32>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op1(*num1 as i64, num2);
                    *num1 = new_val as i32;
                    Ok(new_val.into())
                }
                (PathValue::U32(num1, index), Some(num2)) => {
                    let num1 = num1
                        .as_mut_slice_of::<u32>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op1(*num1 as i64, num2);
                    *num1 = new_val as u32;
                    Ok(new_val.into())
                }
                (PathValue::F32(num1, index), None) => {
                    let num1 = num1
                        .as_mut_slice_of::<f32>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op2(f64::from(*num1), in_value.as_f64().unwrap());
                    *num1 = new_val as f32;
                    Ok(INumber::try_from(*num1)
                        .map_err(|_| RedisError::Str("result is not a number"))?)
                }
                (PathValue::I64(num1, index), Some(num2)) => {
                    let num1 = num1
                        .as_mut_slice_of::<i64>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op1(*num1 as i64, num2);
                    *num1 = new_val as i64;
                    Ok(new_val.into())
                }
                (PathValue::U64(num1, index), Some(num2)) => {
                    let num1 = num1
                        .as_mut_slice_of::<u64>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op1(*num1 as i64, num2);
                    *num1 = new_val as u64;
                    Ok(new_val.into())
                }
                (PathValue::F64(num1, index), None) => {
                    let num1 = num1
                        .as_mut_slice_of::<f64>()
                        .unwrap()
                        .get_mut(index)
                        .unwrap();
                    let new_val = op2(f64::from(*num1), in_value.as_f64().unwrap());
                    *num1 = new_val as f64;
                    Ok(INumber::try_from(*num1)
                        .map_err(|_| RedisError::Str("result is not a number"))?)
                }
                (_, _) => Err(RedisError::Str("result is not a number")),
            })?;
            if n.has_decimal_point() {
                n.to_f64().and_then(serde_json::Number::from_f64)
            } else {
                n.to_i64().map(Into::into)
            }
            .ok_or_else(|| RedisError::Str("result is not a number"))
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

    fn set_root(&mut self, data: IValue) -> RedisResult<bool> {
        self.get_json_holder()?;
        if let Some(val) = &mut self.val {
            val.data = data
        } else {
            self.key.set_value(&REDIS_JSON_TYPE, RedisJSON { data })?
        }
        Ok(true)
    }
}

impl<'a> WriteHolder<IValue, IValue> for IValueKeyHolderWrite<'a> {
    fn notify_keyspace_event(&mut self, ctx: &Context, command: &str) -> Result<(), RedisError> {
        if ctx.notify_keyspace_event(NotifyEvent::MODULE, command, &self.key_name) != Status::Ok {
            Err(RedisError::Str("failed notify key space event"))
        } else {
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

    fn set_value(&mut self, path: Vec<String>, mut v: IValue) -> RedisResult<bool> {
        if path.is_empty() {
            // update the root
            self.set_root(v)
        } else {
            let root = self.get_value()?.unwrap();
            Ok(update(path, root, |val| match val {
                PathValue::IValue(val) => Ok(*val = v.take()),
                PathValue::I8(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set   "))?;
                    Ok(iarray.insert(index, v.take()))},
                PathValue::U8(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set"))?;
                    Ok(iarray.insert(index, v.take()))},
                PathValue::I16(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set"))?;
                    Ok(iarray.insert(index, v.take()))},
                PathValue::U16(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set"))?;
                    Ok(iarray.insert(index, v.take()))},
                PathValue::F16(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set"))?;
                    Ok(iarray.insert(index, v.take()))},
                PathValue::BF16(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set"))?;
                    Ok(iarray.insert(index, v.take()))},
                PathValue::I32(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set"))?;
                    Ok(iarray.insert(index, v.take()))},
                PathValue::U32(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set"))?;
                    Ok(iarray.insert(index, v.take()))},
                PathValue::F32(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set"))?;
                    Ok(iarray.insert(index, v.take()))},
                PathValue::I64(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set"))?;
                    Ok(iarray.insert(index, v.take()))},
                PathValue::U64(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set"))?;
                    Ok(iarray.insert(index, v.take()))},
                PathValue::F64(iarray, index) => {
                    iarray.remove(index).ok_or(RedisError::Str("index out of bounds for array set"))?;
                    Ok(iarray.insert(index, v.take()))},
            })
            .is_ok())
        }
    }

    fn merge_value(&mut self, path: Vec<String>, mut v: IValue) -> RedisResult<bool> {
        let root = self.get_value()?.unwrap();
        Ok(update(path, root, |current| {
            let PathValue::IValue(current) = current else {
                return Err(RedisError::Str("bad object"));
            };
            Ok(merge(current, v.take()))
        })
        .is_ok())
    }

    fn dict_add(&mut self, path: Vec<String>, key: &str, mut v: IValue) -> RedisResult<bool> {
        self.do_op(path, |val| {
            let PathValue::IValue(val) = val else {
                return Err(RedisError::Str("bad object"));
            };
            val.as_object_mut().map_or(Ok(false), |o| {
                let res = !o.contains_key(key);
                if res {
                    o.insert(key.to_string(), v.take());
                }
                Ok(res)
            })
        })
    }

    fn delete_path(&mut self, path: Vec<String>) -> RedisResult<bool> {
        self.get_value().map(|root| remove(path, root.unwrap()))
    }

    fn incr_by(&mut self, path: Vec<String>, num: &str) -> RedisResult<Number> {
        self.do_num_op(path, num, i64::wrapping_add, |f1, f2| f1 + f2)
    }

    fn mult_by(&mut self, path: Vec<String>, num: &str) -> RedisResult<Number> {
        self.do_num_op(path, num, i64::wrapping_mul, |f1, f2| f1 * f2)
    }

    fn pow_by(&mut self, path: Vec<String>, num: &str) -> RedisResult<Number> {
        self.do_num_op(path, num, |i1, i2| i1.pow(i2 as u32), f64::powf)
    }

    fn bool_toggle(&mut self, path: Vec<String>) -> RedisResult<bool> {
        self.do_op(path, |v| {
            let PathValue::IValue(v) = v else {
                return Err(RedisError::Str("bad object"));
            };
            if let DestructuredMut::Bool(mut bool_mut) = v.destructure_mut() {
                //Using DestructuredMut in order to modify a `Bool` variant
                let val = bool_mut.get() ^ true;
                bool_mut.set(val);
                Ok(val)
            } else {
                Err(err_json(v, "bool").into())
            }
        })
    }

    fn str_append(&mut self, path: Vec<String>, val: String) -> RedisResult<usize> {
        match serde_json::from_str(&val)? {
            serde_json::Value::String(s) => self.do_op(path, |v| {
                let PathValue::IValue(v) = v else {
                    return Err(RedisError::Str("bad object"));
                };
                v.as_string_mut()
                    .map(|v_str| {
                        let new_str = [v_str.as_str(), s.as_str()].concat();
                        *v_str = IString::intern(&new_str);
                        Ok(new_str.len())
                    })
                    .unwrap_or_else(|| Err(err_json(v, "string").into()))
            }),
            _ => Err(RedisError::String(err_msg_json_expected(
                "string",
                val.as_str(),
            ))),
        }
    }

    fn arr_append(&mut self, path: Vec<String>, args: Vec<IValue>) -> RedisResult<usize> {
        self.do_op(path, |v| {
            let PathValue::IValue(v) = v else {
                return Err(RedisError::Str("bad object"));
            };
            v.as_array_mut()
                .map(|arr| {
                    arr.extend(args);
                    Ok(arr.len())
                })
                .unwrap_or_else(|| Err(err_json(v, "array").into()))
        })
    }

    fn arr_insert(&mut self, paths: Vec<String>, args: &[IValue], idx: i64) -> RedisResult<usize> {
        self.do_op(paths, |v| {
            let PathValue::IValue(v) = v else {
                return Err(RedisError::Str("bad object"));
            };
            v.as_array_mut()
                .map(|arr| {
                    // Verify legal index in bounds
                    let len = arr.len() as _;
                    let idx = if idx < 0 { len + idx } else { idx };
                    if !(0..=len).contains(&idx) {
                        return Err(RedisError::Str("ERR index out of bounds"));
                    }
                    arr.extend(args.iter().cloned());
                    use ijson::array::ArraySliceMut::*;
                    match arr.as_mut_slice() {
                        Heterogeneous(slice) => slice[idx as _..].rotate_right(args.len()),
                        I8(slice) => slice[idx as _..].rotate_right(args.len()),
                        U8(slice) => slice[idx as _..].rotate_right(args.len()),
                        I16(slice) => slice[idx as _..].rotate_right(args.len()),
                        U16(slice) => slice[idx as _..].rotate_right(args.len()),
                        F16(slice) => slice[idx as _..].rotate_right(args.len()),
                        BF16(slice) => slice[idx as _..].rotate_right(args.len()),
                        I32(slice) => slice[idx as _..].rotate_right(args.len()),
                        U32(slice) => slice[idx as _..].rotate_right(args.len()),
                        F32(slice) => slice[idx as _..].rotate_right(args.len()),
                        I64(slice) => slice[idx as _..].rotate_right(args.len()),
                        U64(slice) => slice[idx as _..].rotate_right(args.len()),
                        F64(slice) => slice[idx as _..].rotate_right(args.len()),
                    };
                    Ok(arr.len())
                })
                .unwrap_or_else(|| Err(err_json(v, "array").into()))
        })
    }

    fn arr_pop<C>(&mut self, path: Vec<String>, index: i64, serialize_callback: C) -> RedisResult
    where
        C: FnOnce(Option<&IValue>) -> RedisResult,
    {
        let res = self.do_op(path, |v| {
            let PathValue::IValue(v) = v else {
                return Err(RedisError::Str("bad object"));
            };
            v.as_array_mut()
                .map(|array| {
                    if array.is_empty() {
                        return None;
                    }
                    // Verify legal index in bounds
                    let len = array.len() as i64;
                    let index = normalize_arr_start_index(index, len) as usize;
                    array.remove(index)
                })
                .ok_or_else(|| err_json(v, "array").into())
        })?;
        serialize_callback(res.as_ref())
    }

    fn arr_trim(&mut self, path: Vec<String>, start: i64, stop: i64) -> RedisResult<usize> {
        self.do_op(path, |v| {
            let PathValue::IValue(v) = v else {
                return Err(RedisError::Str("bad object"));
            };
            v.as_array_mut()
                .map(|array| {
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

                    use ijson::array::ArraySliceMut::*;
                    match array.as_mut_slice() {
                        Heterogeneous(slice) => slice[0..].rotate_left(range.start),
                        I8(slice) => slice[0..].rotate_left(range.start),
                        U8(slice) => slice[0..].rotate_left(range.start),
                        I16(slice) => slice[0..].rotate_left(range.start),
                        U16(slice) => slice[0..].rotate_left(range.start),
                        F16(slice) => slice[0..].rotate_left(range.start),
                        BF16(slice) => slice[0..].rotate_left(range.start),
                        I32(slice) => slice[0..].rotate_left(range.start),
                        U32(slice) => slice[0..].rotate_left(range.start),
                        F32(slice) => slice[0..].rotate_left(range.start),
                        I64(slice) => slice[0..].rotate_left(range.start),
                        U64(slice) => slice[0..].rotate_left(range.start),
                        F64(slice) => slice[0..].rotate_left(range.start),
                    };
                    array.truncate(range.end - range.start);
                    array.len()
                })
                .ok_or_else(|| err_json(v, "array").into())
        })
    }

    fn clear(&mut self, path: Vec<String>) -> RedisResult<usize> {
        self.do_op(path, |v| {
            let PathValue::IValue(v) = v else {
                return Err(RedisError::Str("bad object"));
            };
            match v.destructure_mut() {
                DestructuredMut::Object(obj) => {
                    obj.clear();
                    Ok(1)
                }
                DestructuredMut::Array(arr) => {
                    arr.clear();
                    Ok(1)
                }
                DestructuredMut::Number(n) => {
                    *n = INumber::from(0);
                    Ok(1)
                }
                _ => Ok(0),
            }
        })
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

fn merge(doc: &mut IValue, mut patch: IValue) {
    if !patch.is_object() {
        *doc = patch;
        return;
    }

    if !doc.is_object() {
        *doc = IObject::new().into();
    }
    let map = doc.as_object_mut().unwrap();
    patch
        .as_object_mut()
        .unwrap()
        .into_iter()
        .for_each(|(key, value)| {
            if value.is_null() {
                map.remove(key.as_str());
            } else {
                merge(
                    map.entry(key.as_str()).or_insert(IValue::NULL),
                    value.take(),
                )
            }
        })
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
    /**
     * This function is used to apply changes to the slave and AOF.
     * It is called after the command is executed.
     */
    fn apply_changes(&self, ctx: &Context) {
        ctx.replicate_verbatim();
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
                    let v = docs.iter().next().map_or(IValue::NULL, |(_, b)| {
                        let v: serde_json::Value = b.clone().into();
                        let mut out = serde_json::Serializer::new(Vec::new());
                        v.serialize(&mut out).unwrap();
                        self.from_str(
                            &String::from_utf8(out.into_inner()).unwrap(),
                            Format::JSON,
                            limit_depth,
                        )
                        .unwrap()
                    });
                    Ok(v)
                },
            ),
        }
    }

    fn get_memory(v: &Self::V) -> Result<usize, RedisError> {
        Ok(v.mem_allocated() + size_of::<IValue>())
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

    static SINGLE_THREAD_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_get_memory() {
        let _guard = SINGLE_THREAD_TEST_MUTEX.lock();

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
        assert_eq!(res, 728);
    }

    /// Tests the deserialiser of IValue for a string with unicode
    /// characters, to ensure that the deserialiser can handle
    /// unicode characters well.
    #[test]
    fn test_unicode_characters() {
        let _guard = SINGLE_THREAD_TEST_MUTEX.lock();

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
