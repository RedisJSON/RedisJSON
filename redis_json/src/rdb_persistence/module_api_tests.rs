/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

// Runs the lifecycle and value callbacks the way Redis calls them, against a stand-in for the module
// API that keeps what a save writes in memory and hands it back to the load.

use super::tests::{
    document, inline_record, load_session_exists, loading, persistence, reset, save_session_exists,
};
use super::*;
use ijson::{IArray, IString};
use json_path::select_value::MAX_DEPTH;
use std::cell::Cell;
use std::collections::VecDeque;
use std::ffi::{c_char, c_int};
use std::sync::{Mutex, MutexGuard, Once, PoisonError};

thread_local! {
    /// What a callback wrote with one call, or reads with one call.
    static RDB: RefCell<VecDeque<Vec<u8>>> = const { RefCell::new(VecDeque::new()) };
    static KEYED: Cell<bool> = const { Cell::new(true) };
    static SHORT_READ: Cell<bool> = const { Cell::new(false) };
    static DBID: Cell<c_int> = const { Cell::new(0) };
    static IN_RAM: Cell<bool> = const { Cell::new(true) };
    static RESIDENCY_QUERIES: Cell<usize> = const { Cell::new(0) };
    static CONTEXTS_CREATED: Cell<usize> = const { Cell::new(0) };
    static CONTEXTS_FREED: Cell<usize> = const { Cell::new(0) };
}

fn io() -> *mut raw::RedisModuleIO {
    // The fake module API never reads through this pointer.
    std::ptr::NonNull::dangling().as_ptr()
}

unsafe extern "C" fn save_string_buffer(
    _io: *mut raw::RedisModuleIO,
    buffer: *const c_char,
    len: usize,
) {
    // SAFETY: the caller passes `len` readable bytes.
    let bytes = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), len) };
    RDB.with_borrow_mut(|rdb| rdb.push_back(bytes.to_vec()));
}

unsafe extern "C" fn load_string_buffer(
    _io: *mut raw::RedisModuleIO,
    lenptr: *mut usize,
) -> *mut c_char {
    let Some(bytes) = RDB.with_borrow_mut(VecDeque::pop_front) else {
        SHORT_READ.set(true);
        return std::ptr::null_mut();
    };
    // SAFETY: the allocation holds `bytes.len()` bytes (at least one), lenptr is writable, and
    // the fake `RedisModule_Free` below releases the buffer with free.
    unsafe {
        let buffer = libc::malloc(bytes.len().max(1)).cast::<c_char>();
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast::<u8>(), bytes.len());
        *lenptr = bytes.len();
        buffer
    }
}

unsafe extern "C" fn is_io_error(_io: *mut raw::RedisModuleIO) -> c_int {
    c_int::from(SHORT_READ.get())
}

unsafe extern "C" fn key_name_from_io(
    _io: *mut raw::RedisModuleIO,
) -> *const raw::RedisModuleString {
    if KEYED.get() {
        std::ptr::NonNull::dangling().as_ptr()
    } else {
        std::ptr::null()
    }
}

unsafe extern "C" fn free(ptr: *mut c_void) {
    // SAFETY: the pointer came from the malloc in load_string_buffer.
    unsafe { libc::free(ptr) };
}

unsafe extern "C" fn db_id_from_io(_io: *mut raw::RedisModuleIO) -> c_int {
    DBID.get()
}

unsafe extern "C" fn detached_context(_ctx: *mut raw::RedisModuleCtx) -> *mut raw::RedisModuleCtx {
    CONTEXTS_CREATED.set(CONTEXTS_CREATED.get() + 1);
    Box::into_raw(Box::new(0 as c_int)).cast()
}

unsafe extern "C" fn free_context(ctx: *mut raw::RedisModuleCtx) {
    CONTEXTS_FREED.set(CONTEXTS_FREED.get() + 1);
    // SAFETY: detached_context allocated this fake context's selected database.
    unsafe { drop(Box::from_raw(ctx.cast::<c_int>())) };
}

unsafe extern "C" fn select_db(ctx: *mut raw::RedisModuleCtx, dbid: c_int) -> c_int {
    if !(0..16).contains(&dbid) {
        return raw::REDISMODULE_ERR as c_int;
    }
    // SAFETY: ctx is the live allocation returned by detached_context.
    unsafe { *ctx.cast::<c_int>() = dbid };
    raw::REDISMODULE_OK as c_int
}

unsafe extern "C" fn is_key_in_ram(
    ctx: *mut raw::RedisModuleCtx,
    _key: *mut raw::RedisModuleString,
) -> c_int {
    RESIDENCY_QUERIES.set(RESIDENCY_QUERIES.get() + 1);
    // SAFETY: ctx is the live allocation returned by detached_context. A different database
    // has no matching key, exposing queries that forgot to select the IO database.
    c_int::from(unsafe { *ctx.cast::<c_int>() } == DBID.get() && IN_RAM.get())
}

fn feed(records: Vec<Vec<u8>>) {
    SHORT_READ.set(false);
    RDB.set(records.into());
}

/// What the callbacks saved with `RM_SaveStringBuffer` since the last drain.
fn drain_records() -> Vec<Vec<u8>> {
    RDB.take().into()
}

/// Everything one save wrote: one record per key.
#[derive(Debug, Clone)]
struct Rdb {
    records: Vec<Vec<u8>>,
}

/// What a load made of an `Rdb`: the value of each record.
#[derive(Debug)]
struct Loaded {
    values: Vec<Option<IValue>>,
}

/// The fake server. It owns the process-wide state a test may change for as long as it lives:
/// the module API callbacks, the writer settings and this thread's sessions.
struct Server {
    _serial: MutexGuard<'static, ()>,
}

impl Server {
    fn start() -> Self {
        static LOCK: Mutex<()> = Mutex::new(());
        static INSTALL: Once = Once::new();
        let serial = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        INSTALL.call_once(|| {
            // SAFETY: these are plain C globals that Redis fills in when it loads the module; a
            // unit test has no server, and no other test calls them without this lock.
            unsafe {
                raw::RedisModule_SaveStringBuffer = Some(save_string_buffer);
                raw::RedisModule_LoadStringBuffer = Some(load_string_buffer);
                raw::RedisModule_IsIOError = Some(is_io_error);
                raw::RedisModule_GetKeyNameFromIO = Some(key_name_from_io);
                raw::RedisModule_Free = Some(free);
                raw::RedisModule_GetDbIdFromIO = Some(db_id_from_io);
                raw::RedisModule_GetDetachedThreadSafeContext = Some(detached_context);
                raw::RedisModule_FreeThreadSafeContext = Some(free_context);
                raw::RedisModule_SelectDb = Some(select_db);
                raw::RedisModule_IsKeyInRam = Some(is_key_in_ram);
            }
        });
        let server = Self { _serial: serial };
        server.forget();
        server
    }

    /// Back to a server that has just started.
    fn forget(&self) {
        reset();
        feed(Vec::new());
        KEYED.set(true);
        DBID.set(0);
        IN_RAM.set(true);
        RESIDENCY_QUERIES.set(0);
        CONTEXTS_CREATED.set(0);
        CONTEXTS_FREED.set(0);
        RDB_DICTIONARY_MAX_BYTES.store(DEFAULT_RDB_DICTIONARY_MAX_BYTES, Ordering::Relaxed);
        RDB_DICTIONARY_MAX_DEF_BYTES.store(DEFAULT_RDB_DICTIONARY_MAX_DEF_BYTES, Ordering::Relaxed);
        BIGREDIS_ENABLED.store(false, Ordering::Relaxed);
        backward::LEGACY_RDB_DEPTH_LIMIT.store(false, Ordering::Relaxed);
    }

    fn start_save(&self) {
        persistence(raw::REDISMODULE_SUBEVENT_PERSISTENCE_SYNC_RDB_START);
    }

    fn start_flex_save(&self) {
        BIGREDIS_ENABLED.store(true, Ordering::Relaxed);
        self.start_save();
    }

    fn end_save(&self) {
        persistence(raw::REDISMODULE_SUBEVENT_PERSISTENCE_ENDED);
    }

    fn save_record(&self, value: &IValue) -> Vec<u8> {
        save_value(io(), value);
        let [record] = <[Vec<u8>; 1]>::try_from(drain_records()).expect("one record per key");
        record
    }

    /// A save as Redis runs it: START, one record per key, ENDED.
    fn save(&self, values: &[IValue]) -> Rdb {
        self.start_save();
        let records = values.iter().map(|v| self.save_record(v)).collect();
        self.end_save();
        Rdb { records }
    }

    fn load_record(&self, record: &[u8]) -> Option<IValue> {
        feed(vec![record.to_vec()]);
        load_value(io())
    }

    /// A load as Redis runs it, with lifecycle events bracketing the records.
    fn load(&self, rdb: &Rdb) -> Loaded {
        loading(raw::REDISMODULE_SUBEVENT_LOADING_RDB_START);
        let values = rdb
            .records
            .iter()
            .map(|record| self.load_record(record))
            .collect();
        loading(raw::REDISMODULE_SUBEVENT_LOADING_ENDED);
        Loaded { values }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.forget();
    }
}

fn shared_documents(text: &str, count: usize) -> (IString, Vec<IValue>) {
    let owner = IString::from(text);
    let docs = (0..count).map(|_| document(&owner)).collect();
    (owner, docs)
}

fn save_external(text: &str) -> Vec<u8> {
    crate::rdb_persistence::save_with(io(), |strings, out| {
        ciborium::into_writer(&strings.string(text), out)
    })
    .unwrap();
    let [record] = <[Vec<u8>; 1]>::try_from(drain_records()).unwrap();
    record
}

#[test]
fn manager_init_can_keep_the_address_dictionary_or_register_its_own() {
    let server = Server::start();
    for backend_initialized in [false, true] {
        server.forget();
        if backend_initialized {
            crate::rdb_persistence::set_dictionary_factory(|| {
                Box::new(ijson::ContentDictionary::default())
            });
        }
        let owner = IString::from("a string shared by the selected backend");
        let value = document(&owner);
        server.start_save();
        let resident = server.save_record(&value);
        let external = save_external(owner.as_str());
        server.end_save();
        assert_eq!(resident[0], MARKER_STREAM);
        if backend_initialized {
            assert_eq!(external[0], MARKER_STREAM);
            assert_eq!(&external[5..], &[0xc7, 0]);
        } else {
            assert_eq!(external, inline_record(&IValue::from(owner.as_str())));
        }
        let loaded = server.load(&Rdb {
            records: vec![resident, external],
        });
        assert_eq!(loaded.values[0], Some(value));
        assert_eq!(loaded.values[1], Some(IValue::from(owner.as_str())));
    }
}

#[test]
fn external_manager_records_share_strings_and_standalone_values_stay_inline() {
    let server = Server::start();
    set_dictionary_factory(|| Box::new(ijson::ContentDictionary::default()));
    server.start_flex_save();
    IN_RAM.set(false);
    let first = save_external(&String::from("a string owned by an external manager"));
    KEYED.set(false);
    let standalone = save_external("another external string");
    KEYED.set(true);
    let second = save_external(&String::from("a string owned by an external manager"));
    assert_eq!((first[0], standalone[0], second[0]), (1, 0, 1));
    assert_eq!(&second[5..], &[0xc7, 0]);
    assert_eq!(RESIDENCY_QUERIES.get(), 0);
    server.end_save();
    assert_eq!(save_external("a string owned by an external manager")[0], 0);

    let loaded = server.load(&Rdb {
        records: vec![first, standalone, second],
    });
    let expected = [
        "a string owned by an external manager",
        "another external string",
        "a string owned by an external manager",
    ];
    for (value, text) in loaded.values.iter().zip(expected) {
        assert_eq!(value.as_ref().unwrap().as_string().unwrap().as_str(), text);
    }
}

#[test]
fn configured_dictionary_is_shared_by_all_managers() {
    let server = Server::start();
    set_dictionary_factory(|| Box::new(ijson::ContentDictionary::default()));
    let owner = IString::from("a shared resident string");
    let value = document(&owner);
    server.start_save();
    let first = server.save_record(&value);
    let external = save_external(owner.as_str());
    let repeated = save_external(owner.as_str());
    let last = server.save_record(&value);
    server.end_save();
    assert_eq!([first[0], external[0], repeated[0], last[0]], [1, 1, 1, 1]);
    assert_eq!(&external[1..5], &1_u32.to_le_bytes());
    assert_eq!(&last[1..5], &1_u32.to_le_bytes());
    assert_eq!(&external[5..], &[0xc7, 0]);
    let loaded = server.load(&Rdb {
        records: vec![first, external, repeated, last],
    });
    assert_eq!(loaded.values[0], Some(value.clone()));
    assert_eq!(loaded.values[1], loaded.values[2]);
    assert_eq!(
        loaded.values[1]
            .as_ref()
            .unwrap()
            .as_string()
            .unwrap()
            .as_str(),
        owner.as_str()
    );
    assert_eq!(loaded.values[3], Some(value));
}

#[test]
fn external_serializer_failure_writes_nothing_and_prevents_dangling_references() {
    let server = Server::start();
    set_dictionary_factory(|| Box::new(ijson::ContentDictionary::default()));
    server.start_save();
    let result = crate::rdb_persistence::save_with(io(), |strings, out| {
        ciborium::into_writer(&strings.string("a definition that was not saved"), out).unwrap();
        Err::<(), _>("external serialization failed")
    });
    assert_eq!(result, Err("external serialization failed"));
    assert!(drain_records().is_empty());
    let later = save_external("a definition that was not saved");
    assert_eq!(later[0], 0);
    assert!(server.load_record(&later).is_some());
    let owner = IString::from("a string from the resident manager");
    assert_eq!(server.save_record(&document(&owner))[0], 0);
}

#[test]
fn managers_configure_their_dictionary_once_and_each_save_gets_a_fresh_instance() {
    #[derive(Default)]
    struct ManagerDictionary(ijson::ContentDictionary);

    impl ijson::StringDictionary for ManagerDictionary {
        fn borrows_strings(&self) -> bool {
            false
        }

        fn len(&self) -> usize {
            self.0.len()
        }
        fn clear(&mut self) {
            self.0.clear();
        }
        fn get(&self, text: &ijson::StreamString<'_>) -> Option<(u32, usize)> {
            self.0.get(text)
        }
        fn can_define(&self, text: &ijson::StreamString<'_>) -> bool {
            text.as_str().starts_with("share:")
        }
        fn insert(&mut self, text: &ijson::StreamString<'_>, id: u32, budget: usize) -> bool {
            self.0.insert(text, id, budget)
        }
    }

    let server = Server::start();
    thread_local! { static CREATED: Cell<usize> = const { Cell::new(0) }; }
    CREATED.set(0);
    set_dictionary_factory(|| {
        CREATED.set(CREATED.get() + 1);
        Box::new(ManagerDictionary::default())
    });
    server.start_save();
    for text in [
        "share: manager string",
        "share: manager string",
        "inline manager string",
    ] {
        save_with(io(), |strings, out| {
            ciborium::into_writer(&strings.string(text), out)
        })
        .unwrap();
    }
    let records = drain_records();
    assert_eq!(CREATED.get(), 1);
    assert_eq!(
        records.iter().map(|record| record[0]).collect::<Vec<_>>(),
        [1, 1, 0]
    );
    assert_eq!(&records[1][5..], &[0xc7, 0]);
    server.end_save();
    let values = server.load(&Rdb { records }).values;
    assert_eq!(values[0], values[1]);
    assert_eq!(
        values[2].as_ref().unwrap().as_string().unwrap().as_str(),
        "inline manager string"
    );
    server.start_save();
    assert_eq!(CREATED.get(), 2);
    let fresh = save_external("share: manager string");
    assert_eq!(fresh[0], MARKER_STREAM);
    assert_eq!(&fresh[1..5], &0_u32.to_le_bytes());
}

#[test]
fn external_borrowing_dictionary_obeys_flex_residency() {
    let server = Server::start();
    server.start_flex_save();
    let owner = IString::from("a stable string supplied by a resident manager");
    let shared = owner.clone();
    for in_ram in [false, true, false] {
        IN_RAM.set(in_ram);
        save_with(io(), |strings, out| {
            ciborium::into_writer(&strings.string(&shared), out)
        })
        .unwrap();
    }
    let records = drain_records();
    assert_eq!(
        records.iter().map(|record| record[0]).collect::<Vec<_>>(),
        [0, 1, 1]
    );
    assert_eq!(&records[2][5..], &[0xc7, 0]);
    assert_eq!(RESIDENCY_QUERIES.get(), 3);
    server.end_save();
    assert!(server
        .load(&Rdb { records })
        .values
        .iter()
        .all(|value| { value.as_ref().unwrap().as_string().unwrap() == &owner }));
}

#[test]
fn external_manager_resets_and_definition_limits_use_the_shared_framing() {
    let server = Server::start();
    set_dictionary_factory(|| Box::new(ijson::ContentDictionary::default()));
    RDB_DICTIONARY_MAX_BYTES.store(256, Ordering::Relaxed);
    RDB_DICTIONARY_MAX_DEF_BYTES.store(300, Ordering::Relaxed);
    server.start_save();
    let texts: Vec<_> = (0..10)
        .map(|i| format!("{i:04}{}", "x".repeat(96)))
        .collect();
    let records: Vec<_> = texts.iter().map(|text| save_external(text)).collect();
    assert!(records.iter().any(|record| record[0] == MARKER_RESET));
    assert!(records
        .iter()
        .rev()
        .take(3)
        .all(|record| record[0] == MARKER_INLINE));
    server.end_save();
    let values = server.load(&Rdb { records }).values;
    for (value, text) in values.iter().zip(texts) {
        assert_eq!(value.as_ref().unwrap().as_string().unwrap().as_str(), text);
    }
}

#[test]
fn lifecycle_events_bracket_stream_records_and_repeated_loads() {
    let server = Server::start();
    let (_owner, docs) =
        shared_documents("a string that the whole stream shares, in memory once", 3);
    for _ in 0..2 {
        let rdb = server.save(&docs);
        assert!(rdb.records.iter().all(|record| record[0] == MARKER_STREAM));
        assert!(!save_session_exists());

        let loaded = server.load(&rdb);
        let values: Vec<IValue> = loaded.values.into_iter().map(Option::unwrap).collect();
        assert_eq!(values, docs);
        assert!(!load_session_exists());
    }
}

#[test]
fn a_session_ended_by_a_command_leaves_the_rest_of_the_save_inline() {
    let server = Server::start();
    let (_owner, docs) = shared_documents("a string that a command in the middle ends sharing", 3);
    server.start_save();
    let first = server.save_record(&docs[0]);
    assert_eq!(first[0], MARKER_STREAM);
    // SAFETY: The command filter only clears thread-local state; ctx is unused.
    unsafe { command_filter(std::ptr::null_mut()) };
    let rest = [&docs[1], &docs[2]].map(|doc| server.save_record(doc));
    assert_eq!(rest[0], inline_record(&docs[1]));
    assert_eq!(rest[1], inline_record(&docs[2]));
    server.end_save();

    let rdb = Rdb {
        records: vec![first, rest[0].clone(), rest[1].clone()],
    };
    let loaded = server.load(&rdb);
    let values: Vec<IValue> = loaded.values.into_iter().map(Option::unwrap).collect();
    assert_eq!(values, docs);
}

#[test]
fn a_load_refuses_damaged_records_and_restore_cannot_resolve_a_stream() {
    let server = Server::start();
    let (_owner, docs) = shared_documents("a string that only a stream in progress can resolve", 2);
    let rdb = server.save(&docs);
    let plain = inline_record(&docs[0]);

    // RESTORE: a key but no load in progress. Plain payloads work, stream payloads do not.
    assert_eq!(server.load_record(&plain), Some(docs[0].clone()));
    assert_eq!(server.load_record(&rdb.records[0]), None);
    assert_eq!(server.load_record(&rdb.records[1]), None);

    // A load: reserved and unknown markers, an empty record, and a record that is out of order.
    loading(raw::REDISMODULE_SUBEVENT_LOADING_RDB_START);
    for marker in [2u8, 3, 9] {
        let mut record = plain.clone();
        record[0] = marker;
        assert_eq!(server.load_record(&record), None, "marker {marker}");
    }
    assert_eq!(server.load_record(&[]), None);
    assert_eq!(server.load_record(&rdb.records[1]), None);
    assert!(!load_session_exists());
    loading(raw::REDISMODULE_SUBEVENT_LOADING_ENDED);
}

#[test]
fn a_document_that_shares_nothing_is_saved_as_it_always_was_inside_a_stream() {
    let server = Server::start();
    let (_owner, shared) =
        shared_documents("a string that the shared documents of this stream hold", 2);
    let alone: IValue = serde_json::from_str(
        r#"{"only":"a string that no other document of the unshared stream test holds","n":[1,2]}"#,
    )
    .unwrap();
    let numbers: IValue = serde_json::from_str("[1,2,3]").unwrap();
    // The documents that share nothing sit between the ones that do.
    let docs = [shared[0].clone(), alone, numbers, shared[1].clone()];

    let rdb = server.save(&docs);
    let markers: Vec<u8> = rdb.records.iter().map(|record| record[0]).collect();
    assert_eq!(
        markers,
        [MARKER_STREAM, MARKER_INLINE, MARKER_INLINE, MARKER_STREAM]
    );
    // None of the stream header is left in front of the marker 0 of the others.
    assert_eq!(rdb.records[1], inline_record(&docs[1]));
    assert_eq!(rdb.records[2], inline_record(&docs[2]));

    let loaded = server.load(&rdb);
    let values: Vec<IValue> = loaded.values.into_iter().map(Option::unwrap).collect();
    assert_eq!(values, docs);
}

/// Wraps `leaf` in `depth` single element arrays.
fn nested_arrays(leaf: IValue, depth: usize) -> IValue {
    (0..depth).fold(leaf, |inner, _| {
        IArray::try_from(vec![inner]).unwrap().into()
    })
}

#[test]
fn the_legacy_depth_limit_applies_to_stream_records_as_it_does_to_inline_ones() {
    let server = Server::start();
    let owner = IString::from("a string at the bottom of a deeply nested stream document");
    for depth in [MAX_DEPTH - 1, MAX_DEPTH] {
        // The first record defines the string and the second references it.
        let doc = nested_arrays(IValue::from(owner.clone()), depth);
        let rdb = server.save(&[doc.clone(), doc.clone()]);
        assert!(rdb.records.iter().all(|record| record[0] == MARKER_STREAM));
        for limit in [false, true] {
            backward::LEGACY_RDB_DEPTH_LIMIT.store(limit, Ordering::Relaxed);
            let inline = server.load_record(&inline_record(&doc));
            let streamed = server.load(&rdb).values.iter().all(Option::is_some);
            let case = format!("depth {depth} limit {limit}");
            assert_eq!(inline.is_some(), !limit || depth < MAX_DEPTH, "{case}");
            assert_eq!(streamed, inline.is_some(), "{case}");
        }
    }
}

/// What the core does with a value that is not in memory when a save reaches it, as Flex does with
/// the values it keeps on flash: it decodes the record, saves the value and frees it, one at a time
/// and on the thread of the save.
fn save_a_decoded_record(record: &[u8]) -> Vec<u8> {
    feed(vec![record.to_vec()]);
    let value = crate::redisjson::type_methods::rdb_load(io(), 5);
    assert!(!value.is_null());
    let was_in_ram = IN_RAM.replace(false);
    // SAFETY: the value is the one rdb_load returned, and it stays alive until the free below.
    unsafe { crate::redisjson::type_methods::rdb_save(io(), value) };
    // SAFETY: the value is not used again.
    unsafe { crate::redisjson::type_methods::free(value) };
    IN_RAM.set(was_in_ram);
    let [saved] = <[Vec<u8>; 1]>::try_from(drain_records()).expect("one record per key");
    saved
}

#[test]
fn a_value_decoded_during_a_save_defines_no_string_but_may_reference_one() {
    let server = Server::start();
    // The writer owns no strings, so a string that was freed and whose address another string of
    // the same length took over would go unnoticed.
    let (owner, resident) = shared_documents("a string that the values in memory share", 1);
    server.start_flex_save();
    let defining = server.save_record(&resident[0]);
    assert_eq!(defining[0], MARKER_STREAM);

    // Two values that are only records until the save reaches them. Each repeats a string, so each
    // has two owners while it is decoded, and the strings have one length.
    let cold: Vec<IValue> = (1..=2)
        .map(|n| {
            let text =
                IString::from(format!("cold value {n} repeats a text of one length").as_str());
            IValue::from(
                IArray::try_from(vec![IValue::from(text.clone()), IValue::from(text)]).unwrap(),
            )
        })
        .collect();
    let mut saved = vec![defining];
    saved.extend(
        cold.iter()
            .map(|value| save_a_decoded_record(&inline_record(value))),
    );
    assert!(
        saved[1..].iter().all(|record| record[0] == MARKER_INLINE),
        "a decoded value defined a string: {saved:?}"
    );

    // A decoded value that holds a string of the stream references it: the decoder interned the
    // very allocation that the stream defined.
    let holder = document(&owner);
    saved.push(save_a_decoded_record(&inline_record(&holder)));
    assert_eq!(saved[3][0], MARKER_STREAM);
    assert_eq!(
        saved[3][1..5],
        1u32.to_le_bytes(),
        "no definition came before it"
    );

    server.end_save();

    let rdb = Rdb { records: saved };
    let loaded = server.load(&rdb);
    let values: Vec<IValue> = loaded.values.into_iter().map(Option::unwrap).collect();
    assert_eq!(
        values,
        [
            resident[0].clone(),
            cold[0].clone(),
            cold[1].clone(),
            holder
        ]
    );
}

/// Records of values that exist only as records: every string of a value dies with it.
fn cold_records(count: usize) -> (Vec<Vec<u8>>, Vec<String>) {
    let texts: Vec<String> = (0..count)
        .map(|n| format!("cold value {n:04} repeats a text of one length"))
        .collect();
    let records = texts
        .iter()
        .map(|text| {
            let text = IString::from(text.as_str());
            let value = IValue::from(
                IArray::try_from(vec![IValue::from(text.clone()), IValue::from(text)]).unwrap(),
            );
            inline_record(&value)
        })
        .collect();
    (records, texts)
}

#[test]
fn values_that_the_core_freed_during_a_save_read_back_with_their_own_text() {
    let server = Server::start();
    let (_owner, resident) = shared_documents("a string that the values in memory share", 1);
    server.start_flex_save();
    let mut saved = vec![server.save_record(&resident[0])];
    let (records, texts) = cold_records(64);
    saved.extend(records.iter().map(|record| save_a_decoded_record(record)));
    server.end_save();
    let loaded = server.load(&Rdb { records: saved });
    let values: Vec<IValue> = loaded.values.into_iter().map(Option::unwrap).collect();
    for (value, text) in values[1..].iter().zip(&texts) {
        let array = value.as_array().unwrap();
        assert_eq!(array[0].as_string().unwrap().as_str(), text);
        assert_eq!(array[1].as_string().unwrap().as_str(), text);
    }
}

/// The flash pass of Flex, modelled: documents that stay in memory and documents that are only
/// records until the save reaches them, in a random order, with strings of three lengths that a
/// document repeats. Returns how many documents read back with another text.
fn random_flash_pass(ops: usize, seed: u64) -> usize {
    let server = Server::start();
    let mut state = seed;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let lens = [64usize, 65, 90];
    let text = |id: u64, len: usize| -> String {
        let mut s = format!("{id:016x}-");
        while s.len() < len {
            s.push_str(&format!(
                "{:08x}",
                id.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32
            ));
        }
        s.truncate(len);
        s
    };
    // Strings that the documents in memory share for the whole save.
    let pool_texts: Vec<String> = (0..12)
        .map(|i| text(1000 + i, lens[(i % 3) as usize]))
        .collect();
    let pool: Vec<IString> = pool_texts
        .iter()
        .map(|text| IString::from(text.as_str()))
        .collect();
    server.start_flex_save();
    let mut records = Vec::new();
    let mut expected: Vec<Vec<String>> = Vec::new();
    let mut alive = Vec::new();
    let mut counter = 5000u64;
    for _ in 0..ops {
        let resident = next() % 4 == 0;
        let mut texts = Vec::new();
        let mut items = Vec::new();
        for _ in 0..1 + next() % 3 {
            if next() % 3 == 0 {
                let i = (next() % 12) as usize;
                texts.push(pool_texts[i].clone());
                items.push(IValue::from(pool[i].clone()));
            } else {
                // A fresh string that the document uses twice, so it has two owners.
                counter += 1;
                let t = text(counter, lens[(next() % 3) as usize]);
                let s = IString::from(t.as_str());
                texts.push(t.clone());
                items.push(IValue::from(s.clone()));
                texts.push(t);
                items.push(IValue::from(s));
            }
        }
        let value = IValue::from(IArray::try_from(items).unwrap());
        if resident {
            records.push(server.save_record(&value));
            alive.push(value);
        } else {
            let record = inline_record(&value);
            drop(value);
            records.push(save_a_decoded_record(&record));
        }
        expected.push(texts);
    }
    server.end_save();
    let loaded = server.load(&Rdb { records });

    let mut wrong = 0;
    for (value, texts) in loaded.values.iter().zip(&expected) {
        let value = value.as_ref().expect("every record loads");
        let got: Vec<String> = value
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_string().unwrap().as_str().to_owned())
            .collect();
        if got != *texts {
            wrong += 1;
        }
    }
    drop(alive);
    wrong
}

#[test]
fn a_random_flash_pass_reads_back_exactly() {
    let wrong: usize = (1..=20)
        .map(|seed| random_flash_pass(400, seed * 0x1234_5679))
        .sum();
    assert_eq!(wrong, 0, "documents that read back with another text");
}

#[test]
fn a_cap_on_defined_bytes_limits_what_a_save_defines_and_a_load_reads_it_all() {
    let server = Server::start();
    // Three strings of 40 bytes, each in two documents: the first pass defines what the cap of 80
    // bytes holds, and the string that does not fit stays inline in both of its documents.
    let owners: Vec<IString> = (0..3).map(|i| IString::from(format!("{i:040}"))).collect();
    let docs: Vec<IValue> = owners.iter().chain(owners.iter()).map(document).collect();
    RDB_DICTIONARY_MAX_DEF_BYTES.store(80, Ordering::Relaxed);
    let rdb = server.save(&docs);
    let markers: Vec<u8> = rdb.records.iter().map(|record| record[0]).collect();
    assert_eq!(
        markers,
        [
            MARKER_STREAM,
            MARKER_STREAM,
            MARKER_INLINE,
            MARKER_STREAM,
            MARKER_STREAM,
            MARKER_INLINE
        ]
    );

    let loaded = server.load(&rdb);
    let values: Vec<IValue> = loaded.values.into_iter().map(Option::unwrap).collect();
    assert_eq!(values, docs);

    // Zero sets no bound, and so does a value that a module argument made negative.
    for unbounded in [0, -1] {
        RDB_DICTIONARY_MAX_DEF_BYTES.store(unbounded, Ordering::Relaxed);
        let rdb = server.save(&docs);
        assert!(rdb.records.iter().all(|record| record[0] == MARKER_STREAM));
    }
}

#[test]
fn a_short_io_read_prevents_decoder_restart_until_a_new_loading_event() {
    let server = Server::start();
    let (_owner, docs) = shared_documents("a definition that cannot restart a failed load", 2);
    let rdb = server.save(&docs);
    loading(raw::REDISMODULE_SUBEVENT_LOADING_RDB_START);
    assert!(server.load_record(&rdb.records[0]).is_some());
    feed(Vec::new());
    assert!(load_value(io()).is_none());
    assert!(!load_session_exists());
    assert!(server.load_record(&rdb.records[0]).is_none());
    loading(raw::REDISMODULE_SUBEVENT_LOADING_FAILED);
    let loaded = server.load(&rdb);
    assert_eq!(
        loaded.values,
        docs.into_iter().map(Some).collect::<Vec<_>>()
    );
}

#[test]
fn resident_values_after_a_temporary_decode_still_define_strings() {
    let server = Server::start();
    server.start_flex_save();
    let (_first_owner, first) = shared_documents("a resident definition before a cold value", 2);
    let (_later_owner, later) = shared_documents("a resident definition after a cold value", 2);
    assert_eq!(server.save_record(&first[0])[0], MARKER_STREAM);
    assert_eq!(
        save_a_decoded_record(&inline_record(&IValue::from(17)))[0],
        MARKER_INLINE
    );
    assert_eq!(server.save_record(&later[0])[0], MARKER_STREAM);
    assert_eq!(server.save_record(&first[1])[0], MARKER_STREAM);
    assert_eq!(server.save_record(&later[1])[0], MARKER_STREAM);
}

#[test]
fn flex_residency_uses_the_io_database_and_skips_unknown_keys() {
    let server = Server::start();
    server.start_flex_save();
    let (_owner, docs) = shared_documents("a string available only in the selected database", 2);
    for dbid in [3, 0, 15] {
        DBID.set(dbid);
        assert_eq!(server.save_record(&docs[0])[0], MARKER_STREAM);
    }
    assert_eq!(RESIDENCY_QUERIES.get(), 3);
    let (_owner, unknown) = shared_documents("a string whose residency is unknown", 2);
    for dbid in [-1, 16] {
        DBID.set(dbid);
        assert_eq!(server.save_record(&unknown[0]), inline_record(&unknown[0]));
    }
    DBID.set(0);
    KEYED.set(false);
    assert_eq!(server.save_record(&docs[0]), inline_record(&docs[0]));
    assert_eq!(RESIDENCY_QUERIES.get(), 3);
}

#[test]
fn flex_without_the_residency_api_writes_inline_values() {
    let server = Server::start();
    server.start_flex_save();
    let (_owner, docs) = shared_documents("a string that needs a residency guarantee", 2);
    // SAFETY: Server serializes the tests that use these module API mocks.
    unsafe { raw::RedisModule_IsKeyInRam = None };
    let records: Vec<_> = docs.iter().map(|doc| server.save_record(doc)).collect();
    // SAFETY: restore the shared mock before checking the output.
    unsafe { raw::RedisModule_IsKeyInRam = Some(is_key_in_ram) };
    for (doc, record) in docs.iter().zip(records) {
        assert_eq!(record, inline_record(doc));
    }
    assert_eq!(RESIDENCY_QUERIES.get(), 0);
}

#[test]
fn flex_save_context_is_released_at_every_session_boundary() {
    let server = Server::start();
    for end in [
        raw::REDISMODULE_SUBEVENT_PERSISTENCE_ENDED,
        raw::REDISMODULE_SUBEVENT_PERSISTENCE_FAILED,
    ] {
        server.start_flex_save();
        assert_eq!(CONTEXTS_CREATED.get(), CONTEXTS_FREED.get() + 1);
        persistence(end);
        assert!(!save_session_exists());
        assert_eq!(CONTEXTS_CREATED.get(), CONTEXTS_FREED.get());
    }
    server.start_save();
    server.start_save();
    assert_eq!(CONTEXTS_CREATED.get(), CONTEXTS_FREED.get() + 1);
    // SAFETY: the filter ignores its argument and only drops this thread's save session.
    unsafe { command_filter(std::ptr::null_mut()) };
    assert_eq!(CONTEXTS_CREATED.get(), CONTEXTS_FREED.get());
    let created = CONTEXTS_CREATED.get();
    RDB_DICTIONARY_MAX_BYTES.store(0, Ordering::Relaxed);
    server.start_save();
    assert_eq!(CONTEXTS_CREATED.get(), created);
}

#[test]
fn a_failed_legacy_decode_does_not_disable_resident_definitions() {
    let server = Server::start();
    let (_owner, docs) = shared_documents("resident strings after a legacy decoding attempt", 2);
    server.start_flex_save();
    feed(Vec::new());
    assert!(crate::redisjson::type_methods::rdb_load(io(), 4).is_null());
    assert_eq!(server.save_record(&docs[0])[0], MARKER_STREAM);
}

#[test]
fn a_refused_record_never_leaves_a_table_that_misses_a_definition() {
    let server = Server::start();
    let (_owner, docs) = shared_documents("a definition after a refused record", 2);
    let rdb = server.save(&docs);
    // A stream record that fails releases the reader, so nothing after it resolves.
    for malformed in [
        vec![MARKER_STREAM, 0],
        vec![MARKER_STREAM, 7, 0, 0, 0, 0xf6],
        vec![MARKER_STREAM, 0, 0, 0, 0, 0xc7, 0],
    ] {
        loading(raw::REDISMODULE_SUBEVENT_LOADING_RDB_START);
        assert!(server.load_record(&malformed).is_none(), "{malformed:?}");
        assert!(!load_session_exists());
        assert!(
            server.load_record(&rdb.records[0]).is_none(),
            "{malformed:?}"
        );
        loading(raw::REDISMODULE_SUBEVENT_LOADING_FAILED);
        assert!(server.load(&rdb).values.iter().all(Option::is_some));
    }
    // A record refused before the decoder (empty, an unknown marker, damaged inline CBOR) leaves
    // the table as it was. A stream record lost with it shows at the next one, whose base id no
    // longer matches.
    for malformed in [vec![], vec![255], vec![MARKER_INLINE, 0xff]] {
        loading(raw::REDISMODULE_SUBEVENT_LOADING_RDB_START);
        assert!(server.load_record(&malformed).is_none(), "{malformed:?}");
        assert!(
            server.load_record(&rdb.records[1]).is_none(),
            "{malformed:?}"
        );
        loading(raw::REDISMODULE_SUBEVENT_LOADING_FAILED);
    }
}

#[test]
fn persistence_start_creates_a_writer_and_the_end_drops_it() {
    let server = Server::start();
    assert!(!save_session_exists());
    server.start_save();
    assert!(save_session_exists());
    let (_owner, docs) = shared_documents("a resident string shared by two values", 2);
    KEYED.set(false);
    assert_eq!(server.save_record(&docs[0]), inline_record(&docs[0]));
    KEYED.set(true);
    assert_eq!(server.save_record(&docs[0])[0], MARKER_STREAM);
    server.end_save();
    assert!(!save_session_exists());
    assert_eq!(server.save_record(&docs[0]), inline_record(&docs[0]));
}

#[test]
fn a_budget_of_zero_arms_no_writer() {
    let server = Server::start();
    let (_owner, docs) = shared_documents("a string that a writer that is off never defines", 2);
    // A negative module argument counts as zero.
    for budget in [0, -1] {
        RDB_DICTIONARY_MAX_BYTES.store(budget, Ordering::Relaxed);
        server.start_save();
        assert!(!save_session_exists());
        assert_eq!(server.save_record(&docs[0]), inline_record(&docs[0]));
        server.end_save();
    }
}

#[test]
fn every_persistence_start_forgets_the_strings_of_the_save_before() {
    let server = Server::start();
    let (_owner, docs) = shared_documents("a string that only the save before defined", 2);
    for start in [
        raw::REDISMODULE_SUBEVENT_PERSISTENCE_RDB_START,
        raw::REDISMODULE_SUBEVENT_PERSISTENCE_AOF_START,
        raw::REDISMODULE_SUBEVENT_PERSISTENCE_SYNC_RDB_START,
        raw::REDISMODULE_SUBEVENT_PERSISTENCE_SYNC_AOF_START,
    ] {
        server.start_save();
        assert_eq!(server.save_record(&docs[0])[0], MARKER_STREAM);
        persistence(start);
        // The same string is defined again, with base id 0: nothing was remembered.
        let record = server.save_record(&docs[0]);
        assert_eq!(record[0], MARKER_STREAM);
        assert_eq!(record[1..5], 0u32.to_le_bytes());
    }
}
