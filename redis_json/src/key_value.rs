use std::collections::HashMap;

use json_path::{
    calc_once, calc_once_paths, compile,
    json_path::JsonPathToken,
    select_value::{SelectValue, SelectValueType},
};
use redis_module::{redisvalue::RedisValueKey, RedisResult, RedisValue};
use serde::Serialize;
use serde_json::Value;

use crate::{
    commands::{FoundIndex, ObjectLen, Values},
    error::Error,
    formatter::{FormatOptions, RedisJsonFormatter},
    manager::{
        err_msg_json_expected, err_msg_json_path_doesnt_exist_with_param, AddUpdateInfo,
        SetUpdateInfo, UpdateInfo,
    },
    redisjson::{normalize_arr_indices, Format, Path, SetOptions},
};

pub struct KeyValue<'a, V: SelectValue> {
    val: &'a V,
}

impl<'a, V: SelectValue + 'a> KeyValue<'a, V> {
    pub const fn new(v: &'a V) -> KeyValue<'a, V> {
        KeyValue { val: v }
    }

    pub fn get_first<'b>(&'a self, path: &'b str) -> Result<&'a V, Error> {
        let results = self.get_values(path)?;
        match results.first() {
            Some(s) => Ok(s),
            None => Err(err_msg_json_path_doesnt_exist_with_param(path)
                .as_str()
                .into()),
        }
    }

    pub fn resp_serialize(&self, path: Path) -> RedisResult {
        if path.is_legacy() {
            let v = self.get_first(path.get_path())?;
            Ok(Self::resp_serialize_inner(v))
        } else {
            Ok(self
                .get_values(path.get_path())?
                .iter()
                .map(|v| Self::resp_serialize_inner(v))
                .collect::<Vec<RedisValue>>()
                .into())
        }
    }

    fn resp_serialize_inner(v: &V) -> RedisValue {
        match v.get_type() {
            SelectValueType::Null => RedisValue::Null,

            SelectValueType::Bool => {
                let bool_val = v.get_bool();
                match bool_val {
                    true => RedisValue::SimpleString("true".to_string()),
                    false => RedisValue::SimpleString("false".to_string()),
                }
            }

            SelectValueType::Long => RedisValue::Integer(v.get_long()),

            SelectValueType::Double => RedisValue::Float(v.get_double()),

            SelectValueType::String => RedisValue::BulkString(v.get_str()),

            SelectValueType::Array => {
                let mut res: Vec<RedisValue> = Vec::with_capacity(v.len().unwrap() + 1);
                res.push(RedisValue::SimpleStringStatic("["));
                v.values()
                    .unwrap()
                    .for_each(|v| res.push(Self::resp_serialize_inner(v)));
                RedisValue::Array(res)
            }

            SelectValueType::Object => {
                let mut res: Vec<RedisValue> = Vec::with_capacity(v.len().unwrap() + 1);
                res.push(RedisValue::SimpleStringStatic("{"));
                for (k, v) in v.items().unwrap() {
                    res.push(RedisValue::BulkString(k.to_string()));
                    res.push(Self::resp_serialize_inner(v));
                }
                RedisValue::Array(res)
            }
        }
    }

    pub fn get_values<'b>(&'a self, path: &'b str) -> Result<Vec<&'a V>, Error> {
        let query = compile(path)?;
        let results = calc_once(query, self.val);
        Ok(results)
    }

    pub fn serialize_object<O: Serialize>(o: &O, format: &FormatOptions) -> String {
        // When using the default format, we can use serde_json's default serializer
        if format == &FormatOptions::default() {
            serde_json::to_string(o).unwrap()
        } else {
            let formatter = RedisJsonFormatter::new(format);
            let mut out = serde_json::Serializer::with_formatter(Vec::new(), formatter);
            o.serialize(&mut out).unwrap();
            String::from_utf8(out.into_inner()).unwrap()
        }
    }

    fn to_json_multi(
        &self,
        paths: &mut Vec<Path>,
        format: &FormatOptions,
        is_legacy: bool,
    ) -> Result<RedisValue, Error> {
        // TODO: Creating a temp doc here duplicates memory usage. This can be very memory inefficient.
        // A better way would be to create a doc of references to the original doc but no current support
        // in serde_json. I'm going for this implementation anyway because serde_json isn't supposed to be
        // memory efficient and we're using it anyway. See https://github.com/serde-rs/json/issues/635.
        let mut missing_path = None;
        let path_len = paths.len();
        let temp_doc =
            paths
                .drain(..)
                .fold(HashMap::with_capacity(path_len), |mut acc, path: Path| {
                    let query = compile(path.get_path());

                    // If we can't compile the path, we can't continue
                    if query.is_err() {
                        return acc;
                    }

                    let query = query.unwrap();
                    let results = calc_once(query, self.val);

                    let value = if is_legacy {
                        if results.is_empty() {
                            None
                        } else {
                            Some(Values::Single(results[0]))
                        }
                    } else {
                        Some(Values::Multi(results))
                    };

                    if value.is_none() && missing_path.is_none() {
                        missing_path = Some(path.get_original().to_string());
                    }
                    acc.insert(path.get_original(), value);
                    acc
                });
        if let Some(p) = missing_path {
            return Err(err_msg_json_path_doesnt_exist_with_param(p.as_str()).into());
        }
        let res = if format.resp3 {
            let map = temp_doc
                .iter()
                .map(|(k, v)| {
                    let key = RedisValueKey::String((*k).to_string());
                    let value = match v {
                        Some(Values::Single(value)) => Self::value_to_resp3(value, format),
                        Some(Values::Multi(values)) => Self::values_to_resp3(values, format),
                        None => RedisValue::Null,
                    };
                    (key, value)
                })
                .collect::<HashMap<RedisValueKey, RedisValue>>();
            RedisValue::Map(map)
        } else {
            Self::serialize_object(&temp_doc, format).into()
        };
        Ok(res)
    }

    fn to_resp3(&self, paths: &mut Vec<Path>, format: &FormatOptions) -> Result<RedisValue, Error> {
        let results = paths
            .drain(..)
            .map(|path: Path| {
                compile(path.get_path()).map_or_else(
                    |_| RedisValue::Array(vec![]),
                    |q| Self::values_to_resp3(&calc_once(q, self.val), format),
                )
            })
            .collect::<Vec<RedisValue>>();

        Ok(RedisValue::Array(results))
    }

    fn to_json_single(
        &self,
        path: &str,
        format: &FormatOptions,
        is_legacy: bool,
    ) -> Result<RedisValue, Error> {
        let res = if is_legacy {
            self.to_string_single(path, format)?.into()
        } else if format.resp3 {
            let values = self.get_values(path)?;
            Self::values_to_resp3(&values, format)
        } else {
            self.to_string_multi(path, format)?.into()
        };
        Ok(res)
    }

    fn values_to_resp3(values: &[&V], format: &FormatOptions) -> RedisValue {
        values
            .iter()
            .map(|v| Self::value_to_resp3(v, format))
            .collect::<Vec<RedisValue>>()
            .into()
    }

    fn value_to_resp3(value: &V, format: &FormatOptions) -> RedisValue {
        if format.format == Format::EXPAND {
            match value.get_type() {
                SelectValueType::Null => RedisValue::Null,
                SelectValueType::Bool => RedisValue::Bool(value.get_bool()),
                SelectValueType::Long => RedisValue::Integer(value.get_long()),
                SelectValueType::Double => RedisValue::Float(value.get_double()),
                SelectValueType::String => RedisValue::BulkString(value.get_str()),
                SelectValueType::Array => RedisValue::Array(
                    value
                        .values()
                        .unwrap()
                        .map(|v| Self::value_to_resp3(v, format))
                        .collect::<Vec<RedisValue>>(),
                ),
                SelectValueType::Object => RedisValue::Map(
                    value
                        .items()
                        .unwrap()
                        .map(|(k, v)| {
                            (
                                RedisValueKey::String(k.to_string()),
                                Self::value_to_resp3(v, format),
                            )
                        })
                        .collect::<HashMap<RedisValueKey, RedisValue>>(),
                ),
            }
        } else {
            match value.get_type() {
                SelectValueType::Null => RedisValue::Null,
                SelectValueType::Bool => RedisValue::Bool(value.get_bool()),
                SelectValueType::Long => RedisValue::Integer(value.get_long()),
                SelectValueType::Double => RedisValue::Float(value.get_double()),
                _ => RedisValue::BulkString(Self::serialize_object(value, format)),
            }
        }
    }

    pub fn to_json(
        &self,
        paths: &mut Vec<Path>,
        format: &FormatOptions,
    ) -> Result<RedisValue, Error> {
        if format.format == Format::BSON {
            return Err("ERR Soon to come...".into());
        }
        let is_legacy = !paths.iter().any(|p| !p.is_legacy());
        if format.resp3 {
            self.to_resp3(paths, format)
        } else if paths.len() > 1 {
            self.to_json_multi(paths, format, is_legacy)
        } else {
            self.to_json_single(paths[0].get_path(), format, is_legacy)
        }
    }

    fn find_add_paths(&mut self, path: &str) -> Result<Vec<UpdateInfo>, Error> {
        let mut query = compile(path)?;
        if !query.is_static() {
            return Err("Err wrong static path".into());
        }

        if query.size() < 1 {
            return Err("Err path must end with object key to set".into());
        }

        let (last, token_type) = query.pop_last().unwrap();

        match token_type {
            JsonPathToken::String => {
                if query.size() == 1 {
                    // Adding to the root
                    Ok(vec![UpdateInfo::AUI(AddUpdateInfo {
                        path: Vec::new(),
                        key: last,
                    })])
                } else {
                    // Adding somewhere in existing object
                    let res = calc_once_paths(query, self.val);

                    Ok(res
                        .into_iter()
                        .map(|v| {
                            UpdateInfo::AUI(AddUpdateInfo {
                                path: v,
                                key: last.to_string(),
                            })
                        })
                        .collect())
                }
            }
            JsonPathToken::Number => {
                // if we reach here with array path we are either out of range
                // or no-oping an NX where the value is already present

                let query = compile(path)?;
                let res = calc_once_paths(query, self.val);

                if res.is_empty() {
                    Err("ERR array index out of range".into())
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }

    pub fn find_paths(
        &mut self,
        path: &str,
        option: &SetOptions,
    ) -> Result<Vec<UpdateInfo>, Error> {
        if SetOptions::NotExists != *option {
            let query = compile(path)?;
            let res = calc_once_paths(query, self.val);

            if !res.is_empty() {
                return Ok(res
                    .into_iter()
                    .map(|v| UpdateInfo::SUI(SetUpdateInfo { path: v }))
                    .collect());
            }
        }
        if SetOptions::AlreadyExists == *option {
            Ok(Vec::new()) // empty vector means no updates
        } else {
            self.find_add_paths(path)
        }
    }

    pub fn to_string_single(&self, path: &str, format: &FormatOptions) -> Result<String, Error> {
        let result = self.get_first(path)?;
        Ok(Self::serialize_object(&result, format))
    }

    pub fn to_string_multi(&self, path: &str, format: &FormatOptions) -> Result<String, Error> {
        let results = self.get_values(path)?;
        Ok(Self::serialize_object(&results, format))
    }

    pub fn get_type(&self, path: &str) -> Result<String, Error> {
        let s = Self::value_name(self.get_first(path)?);
        Ok(s.to_string())
    }

    pub fn value_name(value: &V) -> &str {
        match value.get_type() {
            SelectValueType::Null => "null",
            SelectValueType::Bool => "boolean",
            SelectValueType::Long => "integer",
            SelectValueType::Double => "number",
            SelectValueType::String => "string",
            SelectValueType::Array => "array",
            SelectValueType::Object => "object",
        }
    }

    pub fn str_len(&self, path: &str) -> Result<usize, Error> {
        let first = self.get_first(path)?;
        match first.get_type() {
            SelectValueType::String => Ok(first.get_str().len()),
            _ => Err(
                err_msg_json_expected("string", self.get_type(path).unwrap().as_str())
                    .as_str()
                    .into(),
            ),
        }
    }

    pub fn obj_len(&self, path: &str) -> Result<ObjectLen, Error> {
        match self.get_first(path) {
            Ok(first) => match first.get_type() {
                SelectValueType::Object => Ok(ObjectLen::Len(first.len().unwrap())),
                _ => Err(
                    err_msg_json_expected("object", self.get_type(path).unwrap().as_str())
                        .as_str()
                        .into(),
                ),
            },
            _ => Ok(ObjectLen::NoneExisting),
        }
    }

    pub fn is_equal<T1: SelectValue, T2: SelectValue>(a: &T1, b: &T2) -> bool {
        match (a.get_type(), b.get_type()) {
            (SelectValueType::Null, SelectValueType::Null) => true,
            (SelectValueType::Bool, SelectValueType::Bool) => a.get_bool() == b.get_bool(),
            (SelectValueType::Long, SelectValueType::Long) => a.get_long() == b.get_long(),
            (SelectValueType::Double, SelectValueType::Double) => a.get_double() == b.get_double(),
            (SelectValueType::String, SelectValueType::String) => a.get_str() == b.get_str(),
            (SelectValueType::Array, SelectValueType::Array) => {
                if a.len().unwrap() == b.len().unwrap() {
                    for (i, e) in a.values().unwrap().enumerate() {
                        if !Self::is_equal(e, b.get_index(i).unwrap()) {
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            }
            (SelectValueType::Object, SelectValueType::Object) => {
                if a.len().unwrap() == b.len().unwrap() {
                    for k in a.keys().unwrap() {
                        let temp1 = a.get_key(k);
                        let temp2 = b.get_key(k);
                        match (temp1, temp2) {
                            (Some(a1), Some(b1)) => {
                                if !Self::is_equal(a1, b1) {
                                    return false;
                                }
                            }
                            (_, _) => return false,
                        }
                    }
                    true
                } else {
                    false
                }
            }
            (_, _) => false,
        }
    }

    pub fn arr_index(
        &self,
        path: &str,
        json_value: Value,
        start: i64,
        end: i64,
    ) -> Result<RedisValue, Error> {
        let res = self
            .get_values(path)?
            .iter()
            .map(|value| Self::arr_first_index_single(value, &json_value, start, end).into())
            .collect::<Vec<RedisValue>>();
        Ok(res.into())
    }

    pub fn arr_index_legacy(
        &self,
        path: &str,
        json_value: Value,
        start: i64,
        end: i64,
    ) -> Result<RedisValue, Error> {
        let arr = self.get_first(path)?;
        match Self::arr_first_index_single(arr, &json_value, start, end) {
            FoundIndex::NotArray => Err(Error::from(err_msg_json_expected(
                "array",
                self.get_type(path).unwrap().as_str(),
            ))),
            i => Ok(i.into()),
        }
    }

    /// Returns first array index of `v` in `arr`, or `NotFound` if not found in `arr`, or `NotArray` if `arr` is not an array
    fn arr_first_index_single(arr: &V, v: &Value, start: i64, end: i64) -> FoundIndex {
        if !arr.is_array() {
            return FoundIndex::NotArray;
        }

        let len = arr.len().unwrap() as i64;
        if len == 0 {
            return FoundIndex::NotFound;
        }
        // end=0 means INFINITY to support backward with RedisJSON
        let (start, end) = normalize_arr_indices(start, end, len);

        if end < start {
            // don't search at all
            return FoundIndex::NotFound;
        }

        for index in start..end {
            if Self::is_equal(arr.get_index(index as usize).unwrap(), v) {
                return FoundIndex::Index(index);
            }
        }

        FoundIndex::NotFound
    }
}
