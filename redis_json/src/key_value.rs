use itertools::Itertools;
use std::collections::HashMap;

use json_path::{
    calc_once, calc_once_paths, compile,
    json_path::JsonPathToken,
    select_value::{SelectValue, SelectValueType},
};
use redis_module::{redisvalue::RedisValueKey, RedisError, RedisResult, RedisValue};
use serde::Serialize;
use serde_json::Value;

use crate::{
    commands::{prepare_paths_for_updating, FoundIndex, ObjectLen, Values},
    formatter::{RedisJsonFormatter, ReplyFormatOptions},
    manager::{err_json, path_doesnt_exist_with_param, AddUpdateInfo, SetUpdateInfo, UpdateInfo},
    redisjson::{normalize_arr_indices, Path, ReplyFormat, SetOptions},
};

pub struct KeyValue<'a, V: SelectValue> {
    val: &'a V,
}

impl<'a, V: SelectValue + 'a> KeyValue<'a, V> {
    pub const fn new(v: &'a V) -> KeyValue<'a, V> {
        KeyValue { val: v }
    }

    pub fn get_first<'b>(&'a self, path: &'b str) -> RedisResult<&'a V> {
        let results = self.get_values(path)?;
        results
            .first()
            .copied()
            .ok_or_else(|| path_doesnt_exist_with_param(path))
    }

    pub fn resp_serialize(&self, path: Path) -> RedisResult {
        if path.is_legacy() {
            let v = self.get_first(path.get_path())?;
            Ok(Self::resp_serialize_inner(v))
        } else {
            let v = self.get_values(path.get_path())?;
            Ok(v.into_iter()
                .map(Self::resp_serialize_inner)
                .collect_vec()
                .into())
        }
    }

    fn resp_serialize_inner(v: &V) -> RedisValue {
        match v.get_type() {
            SelectValueType::Null => RedisValue::Null,
            SelectValueType::Bool => match v.get_bool() {
                true => RedisValue::SimpleStringStatic("true"),
                false => RedisValue::SimpleStringStatic("false"),
            },
            SelectValueType::Long => v.get_long().into(),
            SelectValueType::Double => v.get_double().into(),
            SelectValueType::String => v.get_str().into(),
            SelectValueType::Array => std::iter::once(RedisValue::SimpleStringStatic("["))
                .chain(v.values().unwrap().map(Self::resp_serialize_inner))
                .collect_vec()
                .into(),
            SelectValueType::Object => std::iter::once(RedisValue::SimpleStringStatic("{"))
                .chain(
                    v.items()
                        .unwrap()
                        .flat_map(|(k, v)| [k.to_string().into(), Self::resp_serialize_inner(v)]),
                )
                .collect_vec()
                .into(),
        }
    }

    pub fn get_values<'b>(&'a self, path: &'b str) -> RedisResult<Vec<&'a V>> {
        let query = compile(path)?;
        Ok(calc_once(query, self.val))
    }

    pub fn serialize_object<O: Serialize>(o: O, format: ReplyFormatOptions) -> String {
        // When using the default formatting, we can use serde_json's default serializer
        if format.no_formatting() {
            serde_json::to_string(&o).unwrap()
        } else {
            let formatter = RedisJsonFormatter::new(format);
            let mut out = serde_json::Serializer::with_formatter(Vec::new(), formatter);
            o.serialize(&mut out).unwrap();
            String::from_utf8(out.into_inner()).unwrap()
        }
    }

    fn to_json_multi(
        &self,
        paths: Vec<Path>,
        format: ReplyFormatOptions,
        is_legacy: bool,
    ) -> RedisResult {
        // TODO: Creating a temp doc here duplicates memory usage. This can be very memory inefficient.
        // A better way would be to create a doc of references to the original doc but no current support
        // in serde_json. I'm going for this implementation anyway because serde_json isn't supposed to be
        // memory efficient and we're using it anyway. See https://github.com/serde-rs/json/issues/635.
        let temp_doc: HashMap<_, _> = paths
            .into_iter()
            .filter_map(|path| {
                // If we can't compile the path, we can't continue
                compile(path.get_path()).ok().map(|query| {
                    let results = calc_once(query, self.val);

                    let value = if is_legacy {
                        if results.is_empty() {
                            return Err(path_doesnt_exist_with_param(path.get_original()));
                        }
                        Values::Single(results[0])
                    } else {
                        Values::Multi(results)
                    };

                    Ok((path.get_original(), value))
                })
            })
            .try_collect()?;

        // If we're using RESP3, we need to convert the HashMap to a RedisValue::Map unless we're using the legacy format
        let res = if format.is_resp3_reply() {
            temp_doc
                .into_iter()
                .map(|(key, v)| {
                    let value = match v {
                        Values::Single(value) => Self::value_to_resp3(value, format),
                        Values::Multi(values) => Self::values_to_resp3(values, format),
                    };
                    (key.into(), value)
                })
                .collect::<HashMap<RedisValueKey, _>>()
                .into()
        } else {
            Self::serialize_object(temp_doc, format).into()
        };
        Ok(res)
    }

    fn to_resp3(&self, paths: Vec<Path>, format: ReplyFormatOptions) -> RedisResult {
        Ok(paths
            .into_iter()
            .map(|path| self.to_resp3_path(path, format))
            .collect_vec()
            .into())
    }

    pub fn to_resp3_path(&self, path: Path, format: ReplyFormatOptions) -> RedisValue {
        compile(path.get_path()).map_or(RedisValue::Array(vec![]), |q| {
            Self::values_to_resp3(calc_once(q, self.val), format)
        })
    }

    fn to_json_single(
        &self,
        path: &str,
        format: ReplyFormatOptions,
        is_legacy: bool,
    ) -> RedisResult {
        if is_legacy {
            self.to_string_single(path, format).map(Into::into)
        } else if format.is_resp3_reply() {
            let values = self.get_values(path)?;
            Ok(Self::values_to_resp3(values, format))
        } else {
            self.to_string_multi(path, format).map(Into::into)
        }
    }

    fn values_to_resp3(values: Vec<&V>, format: ReplyFormatOptions) -> RedisValue {
        values
            .into_iter()
            .map(|v| Self::value_to_resp3(v, format))
            .collect_vec()
            .into()
    }

    pub fn value_to_resp3(value: &V, format: ReplyFormatOptions) -> RedisValue {
        use SelectValueType as SVT;
        match value.get_type() {
            SVT::Null => RedisValue::Null,
            SVT::Bool => value.get_bool().into(),
            SVT::Long => value.get_long().into(),
            SVT::Double => value.get_double().into(),
            SVT::String if format.format == ReplyFormat::EXPAND => value.get_str().into(),
            SVT::Array if format.format == ReplyFormat::EXPAND => value
                .values()
                .unwrap()
                .map(|value| Self::value_to_resp3(value, format))
                .collect_vec()
                .into(),
            SVT::Object if format.format == ReplyFormat::EXPAND => value
                .items()
                .unwrap()
                .map(|(key, value)| (key.into(), Self::value_to_resp3(value, format)))
                .collect::<HashMap<RedisValueKey, _>>()
                .into(),
            _ => Self::serialize_object(value, format).into(),
        }
    }

    pub fn to_json(&self, paths: Vec<Path>, format: ReplyFormatOptions) -> RedisResult {
        let is_legacy = paths.iter().all(Path::is_legacy);

        // If we're using RESP3, we need to reply with an array of values
        if format.is_resp3_reply() {
            self.to_resp3(paths, format)
        } else if paths.len() > 1 {
            self.to_json_multi(paths, format, is_legacy)
        } else {
            self.to_json_single(paths[0].get_path(), format, is_legacy)
        }
    }

    fn find_add_paths(&mut self, path: &str) -> RedisResult<Vec<UpdateInfo>> {
        let mut query = compile(path)?;
        if !query.is_static() {
            return Err(RedisError::Str("Err wrong static path"));
        }

        if query.size() < 1 {
            return Err(RedisError::Str("Err path must end with object key to set"));
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
                    Ok(calc_once_paths(query, self.val)
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
                    Err(RedisError::Str("ERR array index out of range"))
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }

    pub fn find_paths(&mut self, path: &str, option: SetOptions) -> RedisResult<Vec<UpdateInfo>> {
        if option != SetOptions::NotExists {
            let query = compile(path)?;
            let mut paths = calc_once_paths(query, self.val);
            if option != SetOptions::MergeExisting {
                paths = prepare_paths_for_updating(paths);
            }
            let res = paths
                .into_iter()
                .map(|v| UpdateInfo::SUI(SetUpdateInfo { path: v }))
                .collect_vec();
            if !res.is_empty() {
                return Ok(res);
            }
        }
        if option == SetOptions::AlreadyExists {
            Ok(Vec::new()) // empty vector means no updates
        } else {
            self.find_add_paths(path)
        }
    }

    pub fn to_string_single(&self, path: &str, fmt: ReplyFormatOptions) -> RedisResult<String> {
        self.get_first(path).map(|o| Self::serialize_object(o, fmt))
    }

    pub fn to_string_multi(&self, path: &str, fmt: ReplyFormatOptions) -> RedisResult<String> {
        self.get_values(path)
            .map(|o| Self::serialize_object(o, fmt))
    }

    pub fn get_type(&self, path: &str) -> RedisResult<&'static str> {
        self.get_first(path).map(Self::value_name)
    }

    pub fn value_name(value: &V) -> &'static str {
        match value.get_type() {
            SelectValueType::Null => "null",
            SelectValueType::Bool => "boolean",
            SelectValueType::Long => "integer",
            // For dealing with u64 values over i64::MAX, get_type() replies
            // that they are SelectValueType::Double to prevent panics from
            // incorrect casts. However when querying the type of such a value,
            // any response other than 'integer' is a breaking change
            SelectValueType::Double => match value.is_double() {
                true => "number",
                false => "integer",
            },
            SelectValueType::String => "string",
            SelectValueType::Array => "array",
            SelectValueType::Object => "object",
        }
    }

    pub fn str_len(&self, path: &str) -> RedisResult<usize> {
        let first = self.get_first(path)?;
        match first.get_type() {
            SelectValueType::String => Ok(first.get_str().len()),
            _ => Err(err_json(first, "string")),
        }
    }

    pub fn obj_len(&self, path: &str) -> RedisResult<ObjectLen> {
        self.get_first(path)
            .map_or(Ok(ObjectLen::NoneExisting), |first| {
                match first.get_type() {
                    SelectValueType::Object => Ok(ObjectLen::Len(first.len().unwrap())),
                    _ => Err(err_json(first, "object")),
                }
            })
    }

    pub fn is_equal<T1: SelectValue, T2: SelectValue>(a: &T1, b: &T2) -> bool {
        a.get_type() == b.get_type()
            && match a.get_type() {
                SelectValueType::Null => true,
                SelectValueType::Bool => a.get_bool() == b.get_bool(),
                SelectValueType::Long => a.get_long() == b.get_long(),
                SelectValueType::Double => a.get_double() == b.get_double(),
                SelectValueType::String => a.get_str() == b.get_str(),
                SelectValueType::Array => {
                    a.len().unwrap() == b.len().unwrap()
                        && a.values()
                            .unwrap()
                            .zip(b.values().unwrap())
                            .all(|(a, b)| Self::is_equal(a, b))
                }
                SelectValueType::Object => {
                    a.len().unwrap() == b.len().unwrap()
                        && a.keys()
                            .unwrap()
                            .all(|k| match (a.get_key(k), b.get_key(k)) {
                                (Some(a), Some(b)) => Self::is_equal(a, b),
                                _ => false,
                            })
                }
            }
    }

    pub fn arr_index(&self, path: &str, v: Value, start: i64, end: i64) -> RedisResult {
        let values = self.get_values(path)?;
        Ok(values
            .into_iter()
            .map(|arr| Self::arr_first_index_single(arr, &v, start, end))
            .map(RedisValue::from)
            .collect_vec()
            .into())
    }

    pub fn arr_index_legacy(&self, path: &str, v: Value, start: i64, end: i64) -> RedisResult {
        let first = self.get_first(path)?;
        match Self::arr_first_index_single(first, &v, start, end) {
            FoundIndex::NotArray => Err(err_json(first, "array")),
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
