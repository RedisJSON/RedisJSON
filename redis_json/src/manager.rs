/*
 * Copyright Redis Ltd. 2016 - present
 * Licensed under your choice of the Redis Source Available License 2.0 (RSALv2) or
 * the Server Side Public License v1 (SSPLv1).
 */

use json_path::select_value::SelectValue;
use redis_module::key::KeyFlags;
use serde_json::Number;

use redis_module::raw::RedisModuleKey;
use redis_module::{Context, RedisError, RedisResult, RedisString};

use crate::key_value::KeyValue;
use crate::Format;

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
    fn get_value(&self) -> RedisResult<Option<&V>>;
}

pub trait WriteHolder<O: Clone, V: SelectValue> {
    fn delete(&mut self) -> RedisResult<()>;
    fn get_value(&mut self) -> RedisResult<Option<&mut V>>;
    fn set_value(&mut self, path: Vec<String>, v: O) -> RedisResult<bool>;
    fn merge_value(&mut self, path: Vec<String>, v: O) -> RedisResult<bool>;
    fn dict_add(&mut self, path: Vec<String>, key: &str, v: O) -> RedisResult<bool>;
    fn delete_path(&mut self, path: Vec<String>) -> RedisResult<bool>;
    fn incr_by(&mut self, path: Vec<String>, num: &str) -> RedisResult<Number>;
    fn mult_by(&mut self, path: Vec<String>, num: &str) -> RedisResult<Number>;
    fn pow_by(&mut self, path: Vec<String>, num: &str) -> RedisResult<Number>;
    fn bool_toggle(&mut self, path: Vec<String>) -> RedisResult<bool>;
    fn str_append(&mut self, path: Vec<String>, val: String) -> RedisResult<usize>;
    fn arr_append(&mut self, path: Vec<String>, args: &[O]) -> RedisResult<usize>;
    fn arr_insert(&mut self, path: Vec<String>, args: &[O], index: i64) -> RedisResult<usize>;
    fn arr_pop<C>(&mut self, path: Vec<String>, index: i64, serialize_callback: C) -> RedisResult
    where
        C: FnOnce(Option<&V>) -> RedisResult;
    fn arr_trim(&mut self, path: Vec<String>, start: i64, stop: i64) -> RedisResult<usize>;
    fn clear(&mut self, path: Vec<String>) -> RedisResult<usize>;
    fn notify_keyspace_event(self, ctx: &Context, command: &str) -> RedisResult<()>;
}

pub trait Manager {
    /* V - SelectValue that the json path library can work on
     * O - SelectValue Holder
     * Naive implementation is that V and O are from the same type but its not
     * always possible so they are separated
     */
    type V: SelectValue;
    type O: Clone;
    type WriteHolder: WriteHolder<Self::O, Self::V>;
    type ReadHolder: ReadHolder<Self::V>;
    fn open_key_read(&self, ctx: &Context, key: &RedisString) -> RedisResult<Self::ReadHolder>;
    fn open_key_read_with_flags(
        &self,
        ctx: &Context,
        key: &RedisString,
        flags: KeyFlags,
    ) -> RedisResult<Self::ReadHolder>;
    fn open_key_write(&self, ctx: &Context, key: RedisString) -> RedisResult<Self::WriteHolder>;
    fn apply_changes(&self, ctx: &Context);
    #[allow(clippy::wrong_self_convention)]
    fn from_str(&self, val: &str, format: Format, limit_depth: bool) -> RedisResult<Self::O>;
    fn get_memory(&self, v: &Self::V) -> RedisResult<usize>;
    fn is_json(&self, key: *mut RedisModuleKey) -> RedisResult<bool>;
}

pub(crate) fn err_json<V: SelectValue>(value: &V, exp: &'static str) -> RedisError {
    expected(exp, KeyValue::value_name(value))
}

pub(crate) fn expected(exp: &'static str, found: &str) -> RedisError {
    RedisError::String(format!(
        "ERR WRONGTYPE wrong type of path value - expected {exp} but found {found}"
    ))
}

pub(crate) fn path_doesnt_exist_with_param(path: &str) -> RedisError {
    RedisError::String(format!("ERR Path '{path}' does not exist"))
}

pub(crate) fn path_doesnt_exist() -> RedisError {
    RedisError::String("ERR Path does not exist".into())
}

pub(crate) fn path_doesnt_exist_with_param_or(path: &str, or: &str) -> RedisError {
    RedisError::String(format!("ERR Path '{path}' does not exist or {or}"))
}
