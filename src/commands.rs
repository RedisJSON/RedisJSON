use crate::formatter::RedisJsonFormatter;
use crate::manager::{AddUpdateInfo, Manager, ReadHolder, SetUpdateInfo, UpdateInfo, WriteHolder};
use crate::redisjson::{normalize_arr_indices, Format, Path};
use jsonpath_lib::select::select_value::{SelectValue, SelectValueType};
use redis_module::{Context, RedisValue};
use redis_module::{NextArg, RedisError, RedisResult, RedisString, REDIS_OK};

use jsonpath_lib::select::Selector;

use crate::nodevisitor::{StaticPathElement, StaticPathParser, VisitStatus};

use crate::error::Error;

use crate::redisjson::SetOptions;

use serde_json::{Number, Value};

use serde::Serialize;
use std::collections::HashMap;
const JSON_ROOT_PATH: &str = "$";
const JSON_ROOT_PATH_LEGACY: &str = ".";
const CMD_ARG_NOESCAPE: &str = "NOESCAPE";
const CMD_ARG_INDENT: &str = "INDENT";
const CMD_ARG_NEWLINE: &str = "NEWLINE";
const CMD_ARG_SPACE: &str = "SPACE";
const CMD_ARG_FORMAT: &str = "FORMAT";

// Compile time evaluation of the max len() of all elements of the array
const fn max_strlen(arr: &[&str]) -> usize {
    let mut max_strlen = 0;
    let arr_len = arr.len();
    if arr_len < 1 {
        return max_strlen;
    }
    let mut pos = 0;
    while pos < arr_len {
        let curr_strlen = arr[pos].len();
        if max_strlen < curr_strlen {
            max_strlen = curr_strlen;
        }
        pos += 1;
    }
    max_strlen
}

// We use this constant to further optimize json_get command, by calculating the max subcommand length
// Any subcommand added to JSON.GET should be included on the following array
const JSONGET_SUBCOMMANDS_MAXSTRLEN: usize = max_strlen(&[
    CMD_ARG_NOESCAPE,
    CMD_ARG_INDENT,
    CMD_ARG_NEWLINE,
    CMD_ARG_SPACE,
    CMD_ARG_FORMAT,
]);

pub struct KeyValue<'a, V: SelectValue> {
    val: &'a V,
}

impl<'a, V: SelectValue> KeyValue<'a, V> {
    pub fn new(v: &'a V) -> KeyValue<'a, V> {
        KeyValue { val: v }
    }

    fn get_first<'b>(&'a self, path: &'b str) -> Result<&'a V, Error> {
        let results = self.get_values(path)?;
        match results.first() {
            Some(s) => Ok(s),
            None => Err("ERR path does not exist".into()),
        }
    }

    fn resp_serialize(&'a self, path: Path) -> RedisResult {
        if path.is_legacy() {
            let v = self.get_first(path.get_path())?;
            Ok(self.resp_serialize_inner(v))
        } else {
            let res = self.get_values(path.get_path())?;
            if !res.is_empty() {
                Ok(res
                    .iter()
                    .map(|v| self.resp_serialize_inner(v))
                    .collect::<Vec<RedisValue>>()
                    .into())
            } else {
                Err(RedisError::String(format!(
                    "Path '{}' does not exist",
                    path.get_path()
                )))
            }
        }
    }

    fn resp_serialize_inner(&'a self, v: &V) -> RedisValue {
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
                    .for_each(|v| res.push(self.resp_serialize_inner(v)));
                RedisValue::Array(res)
            }

            SelectValueType::Object => {
                let mut res: Vec<RedisValue> = Vec::with_capacity(v.len().unwrap() + 1);
                res.push(RedisValue::SimpleStringStatic("{"));
                for (k, v) in v.items().unwrap() {
                    res.push(RedisValue::BulkString(k.to_string()));
                    res.push(self.resp_serialize_inner(v));
                }
                RedisValue::Array(res)
            }
        }
    }

    fn get_values<'b>(&'a self, path: &'b str) -> Result<Vec<&'a V>, Error> {
        let mut selector = Selector::new();
        selector.str_path(path)?;
        selector.value(self.val);
        let results = selector.select()?;
        Ok(results)
    }

    pub fn serialize_object<O: Serialize>(
        o: &O,
        indent: Option<&str>,
        newline: Option<&str>,
        space: Option<&str>,
    ) -> String {
        let formatter = RedisJsonFormatter::new(indent, space, newline);

        let mut out = serde_json::Serializer::with_formatter(Vec::new(), formatter);
        o.serialize(&mut out).unwrap();
        String::from_utf8(out.into_inner()).unwrap()
    }

    fn to_json_legacy(
        &'a self,
        paths: &mut Vec<Path>,
        indent: Option<&str>,
        newline: Option<&str>,
        space: Option<&str>,
    ) -> Result<RedisValue, Error> {
        if paths.len() > 1 {
            // TODO: Creating a temp doc here duplicates memory usage. This can be very memory inefficient.
            // A better way would be to create a doc of references to the original doc but no current support
            // in serde_json. I'm going for this implementation anyway because serde_json isn't supposed to be
            // memory efficient and we're using it anyway. See https://github.com/serde-rs/json/issues/635.
            let mut missing_path = None;
            let temp_doc = paths.drain(..).fold(HashMap::new(), |mut acc, path| {
                let mut selector = Selector::new();
                selector.value(self.val);
                if selector.str_path(path.get_path()).is_err() {
                    return acc;
                }
                let value = match selector.select() {
                    Ok(s) => s.first().copied(),
                    Err(_) => None,
                };
                if value.is_none() && missing_path.is_none() {
                    missing_path = Some(path.get_original().to_string());
                }
                acc.insert(path.get_original(), value);
                acc
            });
            if let Some(p) = missing_path {
                return Err(format!("ERR path {} does not exist", p).into());
            }

            Ok(Self::serialize_object(&temp_doc, indent, newline, space).into())
        } else {
            Ok(
                Self::serialize_object(
                    self.get_first(paths[0].get_path())?,
                    indent,
                    newline,
                    space,
                )
                .into(),
            )
        }
    }

    fn to_json_multi(
        &'a self,
        paths: &Vec<Path>,
        indent: Option<&str>,
        newline: Option<&str>,
        space: Option<&str>,
    ) -> Result<RedisValue, Error> {
        if paths.len() > 1 {
            let mut res: Vec<Vec<&V>> = vec![];
            for path in paths {
                let values = self.get_values(path.get_path())?;
                res.push(values);
            }
            Ok(Self::serialize_object(&res, indent, newline, space).into())
        } else {
            Ok(self
                .to_string_multi(paths[0].get_path(), indent, newline, space)?
                .into())
        }
    }

    fn to_json(
        &'a self,
        paths: &mut Vec<Path>,
        indent: Option<&str>,
        newline: Option<&str>,
        space: Option<&str>,
        format: Format,
    ) -> Result<RedisValue, Error> {
        if format == Format::BSON {
            return Err("Soon to come...".into());
        }
        let is_legacy = !paths.iter().any(|p| !p.is_legacy());
        if is_legacy {
            self.to_json_legacy(paths, indent, newline, space)
        } else {
            self.to_json_multi(paths, indent, newline, space)
        }
    }

    fn find_add_paths(&mut self, path: &str) -> Result<Vec<UpdateInfo>, Error> {
        let mut parsed_static_path = StaticPathParser::check(path)?;

        if parsed_static_path.valid != VisitStatus::Valid {
            return Err("Err: wrong static path".into());
        }
        if parsed_static_path.static_path_elements.len() < 2 {
            return Err("Err: path must end with object key to set".into());
        }

        let last = parsed_static_path.static_path_elements.pop().unwrap();

        if let StaticPathElement::ObjectKey(key) = last {
            if let StaticPathElement::Root = parsed_static_path.static_path_elements.last().unwrap()
            {
                // Adding to the root
                Ok(vec![UpdateInfo::AUI(AddUpdateInfo {
                    path: Vec::new(),
                    key,
                })])
            } else {
                // Adding somewhere in existing object, use jsonpath_lib::replace_with
                let p = parsed_static_path
                    .static_path_elements
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("");
                let mut selector = Selector::default();
                if let Err(e) = selector.str_path(&p) {
                    return Err(e.into());
                }
                selector.value(self.val);
                let mut res = selector.select_with_paths(|_| true)?;
                Ok(res
                    .drain(..)
                    .map(|v| {
                        UpdateInfo::AUI(AddUpdateInfo {
                            path: v,
                            key: key.to_string(),
                        })
                    })
                    .collect())
            }
        } else if let StaticPathElement::ArrayIndex(_) = last {
            // if we reach here with array path we must be out of range
            // otherwise the path would be valid to be set and we would not
            // have reached here!!
            Err("array index out of range".into())
        } else {
            Err("path not an object or array".into())
        }
    }

    pub fn find_paths(
        &mut self,
        path: &str,
        option: &SetOptions,
    ) -> Result<Vec<UpdateInfo>, Error> {
        if SetOptions::NotExists != *option {
            let mut selector = Selector::default();
            let mut res = selector
                .str_path(path)?
                .value(self.val)
                .select_with_paths(|_| true)?;
            if !res.is_empty() {
                return Ok(res
                    .drain(..)
                    .map(|v| UpdateInfo::SUI(SetUpdateInfo { path: v }))
                    .collect());
            }
        }
        if SetOptions::AlreadyExists != *option {
            self.find_add_paths(path)
        } else {
            Ok(Vec::new()) // empty vector means no updates
        }
    }

    pub fn serialize(results: &V, format: Format) -> Result<String, Error> {
        let res = match format {
            Format::JSON => serde_json::to_string(results)?,
            Format::BSON => return Err("Soon to come...".into()), //results.into() as Bson,
        };
        Ok(res)
    }

    pub fn to_string(&self, path: &str, format: Format) -> Result<String, Error> {
        let results = self.get_first(path)?;
        Self::serialize(results, format)
    }

    pub fn to_string_multi(
        &self,
        path: &str,
        indent: Option<&str>,
        newline: Option<&str>,
        space: Option<&str>,
    ) -> Result<String, Error> {
        let results = self.get_values(path)?;
        Ok(Self::serialize_object(&results, indent, newline, space))
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
            _ => Err("ERR wrong type of path value".into()),
        }
    }

    pub fn arr_len(&self, path: &str) -> Result<usize, Error> {
        let first = self.get_first(path)?;
        match first.get_type() {
            SelectValueType::Array => Ok(first.len().unwrap()),
            _ => Err("ERR wrong type of path value".into()),
        }
    }

    pub fn obj_len(&self, path: &str) -> Result<usize, Error> {
        let first = self.get_first(path)?;
        match first.get_type() {
            SelectValueType::Object => Ok(first.len().unwrap()),
            _ => Err("ERR wrong type of path value".into()),
        }
    }

    pub fn is_equal<T1: SelectValue, T2: SelectValue>(&self, a: &T1, b: &T2) -> bool {
        match (a.get_type(), b.get_type()) {
            (SelectValueType::Null, SelectValueType::Null) => true,
            (SelectValueType::Bool, SelectValueType::Bool) => a.get_bool() == b.get_bool(),
            (SelectValueType::Long, SelectValueType::Long) => a.get_long() == b.get_long(),
            (SelectValueType::Double, SelectValueType::Double) => a.get_double() == b.get_double(),
            (SelectValueType::String, SelectValueType::String) => a.get_str() == b.get_str(),
            (SelectValueType::Array, SelectValueType::Array) => {
                if a.len().unwrap() != b.len().unwrap() {
                    false
                } else {
                    for (i, e) in a.values().unwrap().into_iter().enumerate() {
                        if !self.is_equal(e, b.get_index(i).unwrap()) {
                            return false;
                        }
                    }
                    true
                }
            }
            (SelectValueType::Object, SelectValueType::Object) => {
                if a.len().unwrap() != b.len().unwrap() {
                    false
                } else {
                    for k in a.keys().unwrap() {
                        let temp1 = a.get_key(k);
                        let temp2 = b.get_key(k);
                        match (temp1, temp2) {
                            (Some(a1), Some(b1)) => {
                                if !self.is_equal(a1, b1) {
                                    return false;
                                }
                            }
                            (_, _) => return false,
                        }
                    }
                    true
                }
            }
            (_, _) => false,
        }
    }

    pub fn arr_index(
        &self,
        path: &str,
        scalar_value: Value,
        start: i64,
        end: i64,
    ) -> Result<RedisValue, Error> {
        let res = self
            .get_values(path)?
            .iter()
            .map(|value| {
                self.arr_first_index_single(value, &scalar_value, start, end)
                    .into()
            })
            .collect::<Vec<RedisValue>>();
        Ok(res.into())
    }

    pub fn arr_index_legacy(
        &self,
        path: &str,
        scalar_value: Value,
        start: i64,
        end: i64,
    ) -> Result<RedisValue, Error> {
        let arr = self.get_first(path)?;
        Ok(
            match self.arr_first_index_single(arr, &scalar_value, start, end) {
                FoundIndex::NotArray => RedisValue::Integer(-1),
                i => i.into(),
            },
        )
    }

    /// Returns first array index of `v` in `arr`, or NotFound if not found in `arr`, or NotArray if `arr` is not an array
    fn arr_first_index_single(&self, arr: &V, v: &Value, start: i64, end: i64) -> FoundIndex {
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
            if self.is_equal(arr.get_index(index as usize).unwrap(), v) {
                return FoundIndex::Index(index);
            }
        }

        FoundIndex::NotFound
    }

    pub fn obj_keys(&self, path: &str) -> Result<Box<dyn Iterator<Item = &'_ str> + '_>, Error> {
        self.get_first(path)?
            .keys()
            .ok_or_else(|| "ERR wrong type of path value".into())
    }
}

pub fn command_json_get<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;

    // Set Capcity to 1 assumiung the common case has one path
    let mut paths: Vec<Path> = Vec::with_capacity(1);
    let mut format = Format::JSON;
    let mut indent = None;
    let mut space = None;
    let mut newline = None;
    while let Ok(arg) = args.next_str() {
        match arg {
            // fast way to consider arg a path by using the max length of all possible subcommands
            // See #390 for the comparison of this function with/without this optimization
            arg if arg.len() > JSONGET_SUBCOMMANDS_MAXSTRLEN => paths.push(Path::new(arg)),
            arg if arg.eq_ignore_ascii_case(CMD_ARG_INDENT) => indent = Some(args.next_str()?),
            arg if arg.eq_ignore_ascii_case(CMD_ARG_NEWLINE) => newline = Some(args.next_str()?),
            arg if arg.eq_ignore_ascii_case(CMD_ARG_SPACE) => space = Some(args.next_str()?),
            // Silently ignore. Compatibility with ReJSON v1.0 which has this option. See #168 TODO add support
            arg if arg.eq_ignore_ascii_case(CMD_ARG_NOESCAPE) => continue,
            arg if arg.eq_ignore_ascii_case(CMD_ARG_FORMAT) => {
                format = Format::from_str(args.next_str()?)?
            }
            _ => paths.push(Path::new(arg)),
        };
    }

    // path is optional -> no path found we use root "$"
    if paths.is_empty() {
        paths.push(Path::new(JSON_ROOT_PATH_LEGACY));
    }

    let key = manager.open_key_read(ctx, &key)?;
    let value = match key.get_value()? {
        Some(doc) => KeyValue::new(doc).to_json(&mut paths, indent, newline, space, format)?,
        None => RedisValue::Null,
    };

    Ok(value)
}

pub fn command_json_set<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let value = args.next_str()?;

    let mut format = Format::JSON;
    let mut set_option = SetOptions::None;

    while let Some(s) = args.next() {
        match s.try_as_str()? {
            arg if arg.eq_ignore_ascii_case("NX") && set_option == SetOptions::None => {
                set_option = SetOptions::NotExists
            }
            arg if arg.eq_ignore_ascii_case("XX") && set_option == SetOptions::None => {
                set_option = SetOptions::AlreadyExists
            }
            arg if arg.eq_ignore_ascii_case("FORMAT") => {
                format = Format::from_str(args.next_str()?)?;
            }
            _ => return Err(RedisError::Str("ERR syntax error")),
        };
    }

    let mut redis_key = manager.open_key_write(ctx, key)?;
    let current = redis_key.get_value()?;

    let val = manager.from_str(value, format)?;

    match (current, set_option) {
        (Some(ref mut doc), ref op) => {
            if path.get_path() == JSON_ROOT_PATH {
                if *op != SetOptions::NotExists {
                    redis_key.set_value(Vec::new(), val)?;
                    redis_key.apply_changes(ctx, "json.set")?;
                    REDIS_OK
                } else {
                    Ok(RedisValue::Null)
                }
            } else {
                let mut update_info = KeyValue::new(*doc).find_paths(path.get_path(), op)?;
                if !update_info.is_empty() {
                    let mut res = false;
                    if update_info.len() == 1 {
                        res = match update_info.pop().unwrap() {
                            UpdateInfo::SUI(sui) => redis_key.set_value(sui.path, val)?,
                            UpdateInfo::AUI(aui) => redis_key.dict_add(aui.path, &aui.key, val)?,
                        }
                    } else {
                        for ui in update_info {
                            res = match ui {
                                UpdateInfo::SUI(sui) => {
                                    redis_key.set_value(sui.path, val.clone())?
                                }
                                UpdateInfo::AUI(aui) => {
                                    redis_key.dict_add(aui.path, &aui.key, val.clone())?
                                }
                            }
                        }
                    }
                    if res {
                        redis_key.apply_changes(ctx, "json.set")?;
                        REDIS_OK
                    } else {
                        Ok(RedisValue::Null)
                    }
                } else {
                    Ok(RedisValue::Null)
                }
            }
        }
        (None, SetOptions::AlreadyExists) => Ok(RedisValue::Null),
        (None, _) => {
            if path.get_path() == JSON_ROOT_PATH {
                redis_key.set_value(Vec::new(), val)?;
                redis_key.apply_changes(ctx, "json.set")?;
                REDIS_OK
            } else {
                Err(RedisError::Str(
                    "ERR new objects must be created at the root",
                ))
            }
        }
    }
}

fn find_paths<T: SelectValue, F: FnMut(&T) -> bool>(
    path: &str,
    doc: &T,
    f: F,
) -> Result<Vec<Vec<String>>, RedisError> {
    Ok(Selector::default()
        .str_path(path)?
        .value(doc)
        .select_with_paths(f)?)
}

/// Returns tuples of Value and its concrete path which match the given `path`
fn get_all_values_and_paths<'a, T: SelectValue>(
    path: &str,
    doc: &'a T,
) -> Result<Vec<(&'a T, Vec<String>)>, RedisError> {
    Ok(Selector::default()
        .str_path(path)?
        .value(doc)
        .select_values_with_paths()?)
}

/// Returns a Vec of paths with `None` for Values that do not match the filter
fn filter_paths<T, F>(
    mut values_and_paths: Vec<(&T, Vec<String>)>,
    f: F,
) -> Vec<Option<Vec<String>>>
where
    F: Fn(&T) -> bool,
{
    values_and_paths
        .drain(..)
        .map(|(v, p)| match f(v) {
            true => Some(p),
            _ => None,
        })
        .collect::<Vec<Option<Vec<String>>>>()
}

/// Returns a Vec of Values with `None` for Values that do not match the filter
fn filter_values<T, F>(mut values_and_paths: Vec<(&T, Vec<String>)>, f: F) -> Vec<Option<&T>>
where
    F: Fn(&T) -> bool,
{
    values_and_paths
        .drain(..)
        .map(|(v, _)| match f(v) {
            true => Some(v),
            _ => None,
        })
        .collect::<Vec<Option<&T>>>()
}

fn find_all_paths<T: SelectValue, F: FnMut(&T) -> bool>(
    path: &str,
    doc: &T,
    f: F,
) -> Result<Vec<Option<Vec<String>>>, RedisError>
where
    F: Fn(&T) -> bool,
{
    let res = get_all_values_and_paths(path, doc)?;
    match res.is_empty() {
        false => Ok(filter_paths(res, f)),
        _ => Err(RedisError::String(format!(
            "Path '{}' does not exist",
            path
        ))),
    }
}

fn find_all_values<'a, T: SelectValue, F: FnMut(&T) -> bool>(
    path: &str,
    doc: &'a T,
    f: F,
) -> Result<Vec<Option<&'a T>>, RedisError>
where
    F: Fn(&T) -> bool,
{
    let res = get_all_values_and_paths(path, doc)?;
    match res.is_empty() {
        false => Ok(filter_values(res, f)),
        _ => Err(RedisError::String(format!(
            "Path '{}' does not exist",
            path
        ))),
    }
}

fn to_json_value<T>(values: Vec<Option<T>>, none_value: Value) -> Vec<Value>
where
    Value: From<T>,
{
    values
        .into_iter()
        .map(|n| match n {
            Some(t) => t.into(),
            _ => none_value.clone(),
        })
        .collect::<Vec<Value>>()
}

pub fn command_json_del<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = match args.next() {
        None => Path::new(JSON_ROOT_PATH_LEGACY),
        Some(s) => Path::new(s.try_as_str()?),
    };

    let mut redis_key = manager.open_key_write(ctx, key)?;
    let deleted = match redis_key.get_value()? {
        Some(doc) => {
            let res = if path.get_path() == JSON_ROOT_PATH {
                redis_key.delete()?;
                1
            } else {
                let paths = find_paths(path.get_path(), doc, |_| true)?;
                let mut changed = 0;
                for p in paths {
                    if redis_key.delete_path(p)? {
                        changed += 1;
                    }
                }
                changed
            };
            if res > 0 {
                redis_key.apply_changes(ctx, "json.del")?;
            }
            res
        }
        None => 0,
    };
    Ok((deleted as i64).into())
}

pub fn command_json_mget<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    if args.len() < 3 {
        return Err(RedisError::WrongArity);
    }

    args.last().ok_or(RedisError::WrongArity).and_then(|path| {
        let path = Path::new(path.try_as_str()?);
        let keys = &args[1..args.len() - 1];

        let to_string =
            |doc: &M::V| KeyValue::new(doc).to_string_multi(path.get_path(), None, None, None);
        let to_string_legacy =
            |doc: &M::V| KeyValue::new(doc).to_string(path.get_path(), Format::JSON);
        let is_legacy = path.is_legacy();

        let results: Result<Vec<RedisValue>, RedisError> = keys
            .iter()
            .map(|key| {
                manager
                    .open_key_read(ctx, key)?
                    .get_value()?
                    .map(|doc| {
                        if !is_legacy {
                            to_string(doc)
                        } else {
                            to_string_legacy(doc)
                        }
                    })
                    .transpose()
                    .map_or_else(|_| Ok(RedisValue::Null), |v| Ok(v.into()))
            })
            .collect();

        Ok(results?.into())
    })
}

pub fn command_json_type<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = Path::new(args.next_str().unwrap_or(JSON_ROOT_PATH_LEGACY));

    let key = manager.open_key_read(ctx, &key)?;

    if !path.is_legacy() {
        json_type::<M>(&key, path.get_path())
    } else {
        json_type_legacy::<M>(&key, path.get_path())
    }
}

fn json_type<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let root = redis_key.get_value()?;
    let res = match root {
        Some(root) => KeyValue::new(root)
            .get_values(path)?
            .iter()
            .map(|v| (KeyValue::value_name(*v)).into())
            .collect::<Vec<RedisValue>>()
            .into(),
        None => RedisValue::Null,
    };
    Ok(res)
}

fn json_type_legacy<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let value = redis_key.get_value()?.map_or_else(
        || RedisValue::Null,
        |doc| match KeyValue::new(doc).get_type(path) {
            Ok(s) => s.into(),
            Err(_) => RedisValue::Null,
        },
    );

    Ok(value)
}

enum NumOp {
    Incr,
    Mult,
    Pow,
}

fn command_json_num_op<M>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
    cmd: &str,
    op: NumOp,
) -> RedisResult
where
    M: Manager,
{
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let number = args.next_str()?;

    let mut redis_key = manager.open_key_write(ctx, key)?;

    if !path.is_legacy() {
        json_num_op::<M>(&mut redis_key, ctx, path.get_path(), number, op, cmd)
    } else {
        json_num_op_legacy::<M>(&mut redis_key, ctx, path.get_path(), number, op, cmd)
    }
}

fn json_num_op<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    number: &str,
    op: NumOp,
    cmd: &str,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let paths = find_all_paths(path, root, |v| match v.get_type() {
        SelectValueType::Double | SelectValueType::Long => true,
        _ => false,
    })?;

    let mut res = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                Some(match op {
                    NumOp::Incr => redis_key.incr_by(p, number)?,
                    NumOp::Mult => redis_key.mult_by(p, number)?,
                    NumOp::Pow => redis_key.pow_by(p, number)?,
                })
            }
            _ => None,
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, cmd)?;
    }

    let res = to_json_value::<Number>(res, Value::Null);
    Ok(KeyValue::<M::V>::serialize_object(&res, None, None, None).into())
}

fn json_num_op_legacy<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    number: &str,
    op: NumOp,
    cmd: &str,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let paths = find_paths(path, root, |v| {
        v.get_type() == SelectValueType::Double || v.get_type() == SelectValueType::Long
    })?;
    if !paths.is_empty() {
        let mut res = None;
        for p in paths {
            res = Some(match op {
                NumOp::Incr => redis_key.incr_by(p, number)?,
                NumOp::Mult => redis_key.mult_by(p, number)?,
                NumOp::Pow => redis_key.pow_by(p, number)?,
            });
        }
        redis_key.apply_changes(ctx, cmd)?;
        Ok(res.unwrap().to_string().into())
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist or does not contains a number",
            path
        )))
    }
}

pub fn command_json_num_incrby<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    command_json_num_op(manager, ctx, args, "json.numincrby", NumOp::Incr)
}

pub fn command_json_num_multby<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    command_json_num_op(manager, ctx, args, "json.nummultby", NumOp::Mult)
}

pub fn command_json_num_powby<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    command_json_num_op(manager, ctx, args, "json.numpowby", NumOp::Pow)
}

pub fn command_json_bool_toggle<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let mut redis_key = manager.open_key_write(ctx, key)?;

    if !path.is_legacy() {
        json_bool_toggle::<M>(&mut redis_key, ctx, path.get_path())
    } else {
        json_bool_toggle_legacy::<M>(&mut redis_key, ctx, path.get_path())
    }
}

fn json_bool_toggle<M>(redis_key: &mut M::WriteHolder, ctx: &Context, path: &str) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Bool)?;
    let mut res: Vec<RedisValue> = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                RedisValue::Integer((redis_key.bool_toggle(p)?).into())
            }
            None => RedisValue::Null,
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, "json.arrpop")?;
    }
    Ok(res.into())
}

fn json_bool_toggle_legacy<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let paths = find_paths(path, root, |v| v.get_type() == SelectValueType::Bool)?;
    if !paths.is_empty() {
        let mut res = false;
        for p in paths {
            res = redis_key.bool_toggle(p)?;
        }
        redis_key.apply_changes(ctx, "json.toggle")?;
        Ok(res.to_string().into())
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist or not a bool",
            path
        )))
    }
}

pub fn command_json_str_append<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path_or_json = args.next_str()?;

    let path;
    let json;

    // path is optional
    if let Ok(val) = args.next_arg() {
        path = Path::new(path_or_json);
        json = val.try_as_str()?;
    } else {
        path = Path::new(JSON_ROOT_PATH_LEGACY);
        json = path_or_json;
    }

    let mut redis_key = manager.open_key_write(ctx, key)?;

    if !path.is_legacy() {
        json_str_append::<M>(&mut redis_key, ctx, path.get_path(), json)
    } else {
        json_str_append_legacy::<M>(&mut redis_key, ctx, path.get_path(), json)
    }
}

fn json_str_append<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    json: &str,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::String)?;

    let mut res: Vec<RedisValue> = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                (redis_key.str_append(p, json.to_string())?).into()
            }
            _ => RedisValue::Null,
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, "json.strappend")?;
    }
    Ok(res.into())
}

fn json_str_append_legacy<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    json: &str,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_paths(path, root, |v| v.get_type() == SelectValueType::String)?;
    if !paths.is_empty() {
        let mut res = None;
        for p in paths {
            res = Some(redis_key.str_append(p, json.to_string())?);
        }
        redis_key.apply_changes(ctx, "json.strappend")?;
        Ok(res.unwrap().into())
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist or not a string",
            path
        )))
    }
}

pub fn command_json_str_len<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);

    let key = manager.open_key_read(ctx, &key)?;

    if !path.is_legacy() {
        json_str_len::<M>(&key, path.get_path())
    } else {
        json_str_len_legacy::<M>(&key, path.get_path())
    }
}

fn json_str_len<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let values = find_all_values(path, root, |v| v.get_type() == SelectValueType::String)?;
    let mut res: Vec<RedisValue> = vec![];
    for v in values {
        res.push(match v {
            Some(v) => (v.get_str().len() as i64).into(),
            _ => RedisValue::Null,
        });
    }
    Ok(res.into())
}

fn json_str_len_legacy<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    match redis_key.get_value()? {
        Some(doc) => Ok(RedisValue::Integer(KeyValue::new(doc).str_len(path)? as i64)),
        None => Ok(RedisValue::Null),
    }
}

pub fn command_json_arr_append<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1).peekable();

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);

    // We require at least one JSON item to append
    args.peek().ok_or(RedisError::WrongArity)?;

    let args = args.try_fold::<_, _, Result<_, RedisError>>(
        Vec::with_capacity(args.len()),
        |mut acc, arg| {
            let json = arg.try_as_str()?;
            acc.push(manager.from_str(json, Format::JSON)?);
            Ok(acc)
        },
    )?;

    let mut redis_key = manager.open_key_write(ctx, key)?;

    if !path.is_legacy() {
        json_arr_append::<M>(&mut redis_key, ctx, path.get_path(), args)
    } else {
        json_arr_append_legacy::<M>(&mut redis_key, ctx, path.get_path(), args)
    }
}

fn json_arr_append_legacy<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    args: Vec<M::O>,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let mut paths = find_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    if paths.is_empty() {
        Err(RedisError::String(format!(
            "Path '{}' does not exist",
            path
        )))
    } else if paths.len() == 1 {
        let res = redis_key.arr_append(paths.pop().unwrap(), args)?;
        redis_key.apply_changes(ctx, "json.arrappend")?;
        Ok(res.into())
    } else {
        let mut res = 0;
        for p in paths {
            res = redis_key.arr_append(p, args.clone())?;
        }
        redis_key.apply_changes(ctx, "json.arrappend")?;
        Ok(res.into())
    }
}

fn json_arr_append<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    args: Vec<M::O>,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;
    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;

    let mut res = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                (redis_key.arr_append(p, args.clone())? as i64).into()
            }
            _ => RedisValue::Null,
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, "json.arrappend")?;
    }
    Ok(res.into())
}

enum FoundIndex {
    Index(i64),
    NotFound,
    NotArray,
}

impl From<FoundIndex> for RedisValue {
    fn from(e: FoundIndex) -> Self {
        match e {
            FoundIndex::NotFound => RedisValue::Integer(-1),
            FoundIndex::NotArray => RedisValue::Null,
            FoundIndex::Index(i) => RedisValue::Integer(i),
        }
    }
}

pub fn command_json_arr_index<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let json_scalar = args.next_str()?;
    let start: i64 = args.next().map(|v| v.parse_integer()).unwrap_or(Ok(0))?;
    let end: i64 = args.next().map(|v| v.parse_integer()).unwrap_or(Ok(0))?;

    args.done()?; // TODO: Add to other functions as well to terminate args list

    let key = manager.open_key_read(ctx, &key)?;

    let is_legacy = path.is_legacy();
    let scalar_value: Value = serde_json::from_str(json_scalar)?;
    if !is_legacy && (scalar_value.is_array() || scalar_value.is_object()) {
        return Err(RedisError::String(format!(
            "ERR expected scalar but found {}",
            json_scalar
        )));
    }

    let res = key
        .get_value()?
        .map_or(Ok(RedisValue::Integer(-1)), |doc| {
            if path.is_legacy() {
                KeyValue::new(doc).arr_index_legacy(path.get_path(), scalar_value, start, end)
            } else {
                KeyValue::new(doc).arr_index(path.get_path(), scalar_value, start, end)
            }
        })?;

    Ok(res)
}

pub fn command_json_arr_insert<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1).peekable();

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let index = args.next_i64()?;

    // We require at least one JSON item to insert
    args.peek().ok_or(RedisError::WrongArity)?;
    let args = args.try_fold::<_, _, Result<_, RedisError>>(
        Vec::with_capacity(args.len()),
        |mut acc, arg| {
            let json = arg.try_as_str()?;
            acc.push(manager.from_str(json, Format::JSON)?);
            Ok(acc)
        },
    )?;
    let mut redis_key = manager.open_key_write(ctx, key)?;
    if !path.is_legacy() {
        json_arr_insert::<M>(&mut redis_key, ctx, path.get_path(), index, args)
    } else {
        json_arr_insert_legacy::<M>(&mut redis_key, ctx, path.get_path(), index, args)
    }
}

fn json_arr_insert<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    index: i64,
    args: Vec<M::O>,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;

    let mut res: Vec<RedisValue> = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                (redis_key.arr_insert(p, &args, index)? as i64).into()
            }
            _ => RedisValue::Null,
        });
    }

    if need_notify {
        redis_key.apply_changes(ctx, "json.arrinsert")?;
    }
    Ok(res.into())
}

fn json_arr_insert_legacy<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    index: i64,
    args: Vec<M::O>,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    if !paths.is_empty() {
        let mut res = None;
        for p in paths {
            res = Some(redis_key.arr_insert(p, &args, index)?);
        }
        redis_key.apply_changes(ctx, "json.arrinsert")?;
        Ok(res.unwrap().into())
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist or not an array",
            path
        )))
    }
}

pub fn command_json_arr_len<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = Path::new(args.next_str().unwrap_or(JSON_ROOT_PATH_LEGACY));
    let is_legacy = path.is_legacy();
    let key = manager.open_key_read(ctx, &key)?;
    let root = match key.get_value()? {
        Some(k) => k,
        None if is_legacy => {
            return Ok(RedisValue::Null);
        }
        None => {
            return Err(RedisError::nonexistent_key());
        }
    };
    // let root = key
    //     .get_value()?
    //     .ok_or_else(RedisError::nonexistent_key)?;
    let values = find_all_values(path.get_path(), root, |v| {
        v.get_type() == SelectValueType::Array
    })?;

    let mut res = vec![];
    for v in values {
        let cur_val: RedisValue = match v {
            Some(v) => (v.len().unwrap() as i64).into(),
            _ => RedisValue::Null,
        };
        if !is_legacy {
            res.push(cur_val);
        } else {
            return Ok(cur_val);
        }
    }
    Ok(res.into())
}

pub fn command_json_arr_pop<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;

    let (path, index) = match args.next() {
        None => (Path::new(JSON_ROOT_PATH_LEGACY), i64::MAX),
        Some(s) => {
            let path = Path::new(s.try_as_str()?);
            let index = args.next_i64().unwrap_or(-1);
            (path, index)
        }
    };

    let mut redis_key = manager.open_key_write(ctx, key)?;
    if !path.is_legacy() {
        json_arr_pop::<M>(&mut redis_key, ctx, path.get_path(), index)
    } else {
        json_arr_pop_legacy::<M>(&mut redis_key, ctx, path.get_path(), index)
    }
}

fn json_arr_pop<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    index: i64,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    let mut res: Vec<RedisValue> = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => match redis_key.arr_pop(p, index)? {
                Some(v) => {
                    need_notify = true;
                    v.into()
                }
                _ => RedisValue::Null, // Empty array
            },
            _ => RedisValue::Null, // Not an array
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, "json.arrpop")?;
    }
    Ok(res.into())
}

fn json_arr_pop_legacy<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    index: i64,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    if !paths.is_empty() {
        let mut res = None;
        for p in paths {
            res = Some(redis_key.arr_pop(p, index)?);
        }
        match res.unwrap() {
            Some(r) => {
                redis_key.apply_changes(ctx, "json.arrpop")?;
                Ok(r.into())
            }
            None => Ok(().into()),
        }
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist or not an array",
            path
        )))
    }
}

pub fn command_json_arr_trim<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);
    let start = args.next_i64()?;
    let stop = args.next_i64()?;

    let mut redis_key = manager.open_key_write(ctx, key)?;

    if !path.is_legacy() {
        json_arr_trim::<M>(&mut redis_key, ctx, path.get_path(), start, stop)
    } else {
        json_arr_trim_legacy::<M>(&mut redis_key, ctx, path.get_path(), start, stop)
    }
}
fn json_arr_trim<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    start: i64,
    stop: i64,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_all_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    let mut res: Vec<RedisValue> = vec![];
    let mut need_notify = false;
    for p in paths {
        res.push(match p {
            Some(p) => {
                need_notify = true;
                (redis_key.arr_trim(p, start, stop)?).into()
            }
            _ => RedisValue::Null,
        });
    }
    if need_notify {
        redis_key.apply_changes(ctx, "json.arrtrim")?;
    }
    Ok(res.into())
}

fn json_arr_trim_legacy<M>(
    redis_key: &mut M::WriteHolder,
    ctx: &Context,
    path: &str,
    start: i64,
    stop: i64,
) -> RedisResult
where
    M: Manager,
{
    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_paths(path, root, |v| v.get_type() == SelectValueType::Array)?;
    if !paths.is_empty() {
        let mut res = None;
        for p in paths {
            res = Some(redis_key.arr_trim(p, start, stop)?);
        }
        redis_key.apply_changes(ctx, "json.arrtrim")?;
        Ok(res.unwrap().into())
    } else {
        Err(RedisError::String(format!(
            "Path '{}' does not exist or not an array",
            path
        )))
    }
}

pub fn command_json_obj_keys<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);

    let mut key = manager.open_key_read(ctx, &key)?;
    if !path.is_legacy() {
        json_obj_keys::<M>(&mut key, path.get_path())
    } else {
        json_obj_keys_legacy::<M>(&mut key, path.get_path())
    }
}

fn json_obj_keys<M>(redis_key: &mut M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let root = redis_key.get_value()?;
    let res: RedisValue = match root {
        Some(root) => {
            let values = find_all_values(path, root, |v| v.get_type() == SelectValueType::Object)?;
            let mut res: Vec<RedisValue> = vec![];
            for v in values {
                res.push(match v {
                    Some(v) => v.keys().unwrap().collect::<Vec<&str>>().into(),
                    _ => RedisValue::Null,
                });
            }
            res.into()
        }
        None => RedisValue::Null,
    };
    Ok(res.into())
}

fn json_obj_keys_legacy<M>(redis_key: &mut M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let value = match redis_key.get_value()? {
        Some(doc) => KeyValue::new(doc)
            .obj_keys(path)?
            .collect::<Vec<&str>>()
            .into(),
        None => RedisValue::Null,
    };

    Ok(value)
}

pub fn command_json_obj_len<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let path = Path::new(args.next_str()?);

    let key = manager.open_key_read(ctx, &key)?;
    if !path.is_legacy() {
        json_obj_len::<M>(&key, path.get_path())
    } else {
        json_obj_len_legacy::<M>(&key, path.get_path())
    }
}

fn json_obj_len<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    let root = redis_key.get_value()?;
    let res = match root {
        Some(root) => find_all_values(path, root, |v| v.get_type() == SelectValueType::Object)?
            .iter()
            .map(|v| match *v {
                Some(v) => RedisValue::Integer(v.len().unwrap() as i64),
                None => RedisValue::Null,
            })
            .collect::<Vec<RedisValue>>()
            .into(),
        None => RedisValue::Null,
    };
    Ok(res)
}

fn json_obj_len_legacy<M>(redis_key: &M::ReadHolder, path: &str) -> RedisResult
where
    M: Manager,
{
    match redis_key.get_value()? {
        Some(doc) => Ok(RedisValue::Integer(KeyValue::new(doc).obj_len(path)? as i64)),
        None => Ok(RedisValue::Null),
    }
}

pub fn command_json_clear<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key = args.next_arg()?;
    let paths = args.try_fold::<_, _, Result<Vec<Path>, RedisError>>(
        Vec::with_capacity(args.len()),
        |mut acc, arg| {
            let s = arg.try_as_str()?;
            acc.push(Path::new(s));
            Ok(acc)
        },
    )?;

    let paths = if paths.is_empty() {
        vec![Path::new(JSON_ROOT_PATH)]
    } else {
        paths
    };

    let path = paths.first().unwrap().get_path();

    let mut redis_key = manager.open_key_write(ctx, key)?;

    let root = redis_key
        .get_value()?
        .ok_or_else(RedisError::nonexistent_key)?;

    let paths = find_paths(path, root, |v| {
        v.get_type() == SelectValueType::Array || v.get_type() == SelectValueType::Object
    })?;
    let mut cleared = 0;
    if !paths.is_empty() {
        for p in paths {
            cleared += redis_key.clear(p)?;
        }
    }
    if cleared > 0 {
        redis_key.apply_changes(ctx, "json.clear")?;
    }
    Ok(cleared.into())
}

pub fn command_json_debug<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    match args.next_str()?.to_uppercase().as_str() {
        "MEMORY" => {
            let key = args.next_arg()?;
            let path = Path::new(args.next_str().unwrap_or(JSON_ROOT_PATH_LEGACY));

            let key = manager.open_key_read(ctx, &key)?;
            if path.is_legacy() {
                Ok(match key.get_value()? {
                    Some(doc) => {
                        manager.get_memory(KeyValue::new(doc).get_first(path.get_path())?)?
                    }
                    None => 0,
                }
                .into())
            } else {
                Ok(match key.get_value()? {
                    Some(doc) => KeyValue::new(doc)
                        .get_values(path.get_path())?
                        .iter()
                        .map(|v| manager.get_memory(v).unwrap())
                        .collect::<Vec<usize>>(),
                    None => vec![],
                }
                .into())
            }
        }
        "HELP" => {
            let results = vec![
                "MEMORY <key> [path] - reports memory usage",
                "HELP                - this message",
            ];
            Ok(results.into())
        }
        _ => Err(RedisError::Str(
            "ERR unknown subcommand - try `JSON.DEBUG HELP`",
        )),
    }
}

pub fn command_json_resp<M: Manager>(
    manager: M,
    ctx: &Context,
    args: Vec<RedisString>,
) -> RedisResult {
    let mut args = args.into_iter().skip(1);

    let key = args.next_arg()?;
    let path = match args.next() {
        None => Path::new(JSON_ROOT_PATH),
        Some(s) => Path::new(s.try_as_str()?),
    };

    let key = manager.open_key_read(ctx, &key)?;
    match key.get_value()? {
        Some(doc) => KeyValue::new(doc).resp_serialize(path),
        None => Ok(RedisValue::Null),
    }
}

pub fn command_json_cache_info<M: Manager>(
    _manager: M,
    _ctx: &Context,
    _args: Vec<RedisString>,
) -> RedisResult {
    Err(RedisError::Str("Command was not implemented"))
}

pub fn command_json_cache_init<M: Manager>(
    _manager: M,
    _ctx: &Context,
    _args: Vec<RedisString>,
) -> RedisResult {
    Err(RedisError::Str("Command was not implemented"))
}
