/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use json_path::select_value::SelectValue;
use redis_module::key::KeyFlags;
use serde_json::Number;

use redis_module::raw::RedisModuleKey;
use redis_module::rediserror::RedisError;
use redis_module::{Context, RedisResult, RedisString};

use crate::Format;

use crate::error::Error;

use crate::key_value::KeyValue;

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

/// A read holder is a type that can obtain an immutable reference to
/// the stored data.
pub trait ReadHolder<Selector: SelectValue> {
    fn get_value(&self) -> Result<Option<&Selector>, RedisError>;
}

/// A write holder is a type that can obtain a mutable reference to
/// the stored data and so allow mutating it. As the data can be
/// mutated, an additional [`WriteHolder::apply_changes`] method is
/// provided to notify about the changes.
pub trait WriteHolder<StorageDataType: Clone, SelectorType: SelectValue>:
    StorageBackend<StorageData = StorageDataType, Selector = SelectorType>
{
    /// Deletes the value.
    fn delete(&mut self) -> Result<(), RedisError>;

    /// Returns a mutable reference to the value.
    fn get_value_mut(&mut self) -> Result<Option<&mut SelectorType>, RedisError>;

    /// Notifies about the changes made to the value.
    fn apply_changes(&mut self, ctx: &Context, command: &str) -> Result<(), RedisError>;

    /// Sets the value at the given path to `value`.
    fn set_value(&mut self, path: JsonPath, value: Self::StorageData) -> Result<bool, RedisError>;

    /// Merges the passed value with the value at the given path.
    fn merge_value(&mut self, path: JsonPath, value: Self::StorageData)
        -> Result<bool, RedisError>;
}

/// A json path is a slice of strings following the "JSONPath" syntax.
/// See <https://support.smartbear.com/alertsite/docs/monitors/api/endpoint/jsonpath.html>.
pub type JsonPath<'a> = &'a [String];

/// A trait for the storage backend.
// TODO: rename properly "O" and "V".
pub trait StorageBackend {
    type StorageData: Clone;
    type Selector: SelectValue;

    fn dict_add(
        &mut self,
        path: JsonPath,
        key: &str,
        value: Self::StorageData,
    ) -> Result<bool, RedisError>;
    fn delete_path(&mut self, path: JsonPath) -> Result<bool, RedisError>;
    fn incr_by(&mut self, path: JsonPath, num: &str) -> Result<Number, RedisError>;
    fn mult_by(&mut self, path: JsonPath, num: &str) -> Result<Number, RedisError>;
    fn pow_by(&mut self, path: JsonPath, num: &str) -> Result<Number, RedisError>;
    fn bool_toggle(&mut self, path: JsonPath) -> Result<bool, RedisError>;
    fn str_append(&mut self, path: JsonPath, value: String) -> Result<usize, RedisError>;
    fn arr_append(
        &mut self,
        path: JsonPath,
        args: Vec<Self::StorageData>,
    ) -> Result<usize, RedisError>;
    fn arr_insert(
        &mut self,
        path: JsonPath,
        args: &[Self::StorageData],
        index: i64,
    ) -> Result<usize, RedisError>;
    fn arr_pop<C: FnOnce(Option<&Self::Selector>) -> RedisResult>(
        &mut self,
        path: JsonPath,
        index: i64,
        serialize_callback: C,
    ) -> RedisResult;
    fn arr_trim(&mut self, path: JsonPath, start: i64, stop: i64) -> Result<usize, RedisError>;
    fn clear(&mut self, path: JsonPath) -> Result<usize, RedisError>;
}

/// A json value manager abstraction.
///
/// Naive implementation is that V and O are from the same type but its
/// not always possible so they are separated.
pub trait Manager {
    // TODO: rename properly "O" and "V".
    /// [`Manager::V`] - A type which the json path library can select
    /// from the json value.
    type V: SelectValue;

    /// [`Manager::O`] - The type of data that is stored in the storage
    /// backend.
    type O: Clone;

    /// A type which is used for writing to the storage backend.
    type WriteHolder: WriteHolder<Self::O, Self::V>;
    /// A type which is used for reading from the storage backend.
    type ReadHolder: ReadHolder<Self::V>;

    fn open_key_read(
        &self,
        ctx: &Context,
        key: &RedisString,
    ) -> Result<Self::ReadHolder, RedisError>;

    fn open_key_read_with_flags(
        &self,
        ctx: &Context,
        key: &RedisString,
        flags: KeyFlags,
    ) -> Result<Self::ReadHolder, RedisError>;

    fn open_key_write(
        &self,
        ctx: &Context,
        key: RedisString,
    ) -> Result<Self::WriteHolder, RedisError>;

    #[allow(clippy::wrong_self_convention)]
    fn from_str(&self, val: &str, format: Format, limit_depth: bool) -> Result<Self::O, Error>;

    fn get_memory(&self, v: &Self::V) -> Result<usize, RedisError>;

    fn is_json(&self, key: *mut RedisModuleKey) -> Result<bool, RedisError>;
}

pub(crate) fn err_json<V: SelectValue>(value: &V, expected_value: &'static str) -> Error {
    Error::from(err_msg_json_expected(
        expected_value,
        KeyValue::value_name(value),
    ))
}

pub(crate) fn err_msg_json_expected(expected_value: &'static str, found: &str) -> String {
    format!("WRONGTYPE wrong type of path value - expected {expected_value} but found {found}")
}

pub(crate) fn err_msg_json_path_doesnt_exist_with_param(path: &str) -> String {
    format!("ERR Path '{path}' does not exist")
}

pub(crate) fn err_msg_json_path_doesnt_exist() -> String {
    "ERR Path does not exist".to_string()
}

pub(crate) fn err_msg_json_path_doesnt_exist_with_param_or(path: &str, or: &str) -> String {
    format!("ERR Path '{path}' does not exist or {or}")
}
