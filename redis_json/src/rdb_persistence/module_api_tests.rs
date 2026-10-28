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
    document, inline_record, load_session_exists, loading, persistence, persistence_with_flags,
    reset, save_session_exists, serial, slot_migration,
};
use super::*;
use ijson::{IArray, IObject, IString};
use json_path::select_value::MAX_DEPTH;
use std::cell::Cell;
use std::collections::VecDeque;
use std::ffi::{c_char, c_int};
use std::sync::{MutexGuard, Once};

thread_local! {
    /// What a callback wrote with one call, or reads with one call.
    static RDB: RefCell<VecDeque<Vec<u8>>> = const { RefCell::new(VecDeque::new()) };
    static KEYED: Cell<bool> = const { Cell::new(true) };
    static SHORT_READ: Cell<bool> = const { Cell::new(false) };
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
        static INSTALL: Once = Once::new();
        let serial = serial();
        INSTALL.call_once(|| {
            // SAFETY: these are plain C globals that Redis fills in when it loads the module; a
            // unit test has no server, and no other test calls them without this lock.
            unsafe {
                raw::RedisModule_SaveStringBuffer = Some(save_string_buffer);
                raw::RedisModule_LoadStringBuffer = Some(load_string_buffer);
                raw::RedisModule_IsIOError = Some(is_io_error);
                raw::RedisModule_GetKeyNameFromIO = Some(key_name_from_io);
                raw::RedisModule_Free = Some(free);
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
        RDB_DICTIONARY_MAX_MEMORY_PERCENT
            .store(DEFAULT_RDB_DICTIONARY_MAX_MEMORY_PERCENT, Ordering::Relaxed);
        // 20% of it is far more than any table a test builds.
        USED_MEMORY_FOR_TESTS.set(Some(1 << 32));
        BIGREDIS_ENABLED.store(false, Ordering::Relaxed);
        ASM_EXPORTS.store(0, Ordering::Relaxed);
        backward::LEGACY_RDB_DEPTH_LIMIT.store(false, Ordering::Relaxed);
    }

    fn start_save(&self) {
        persistence(raw::REDISMODULE_SUBEVENT_PERSISTENCE_SYNC_RDB_START);
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
fn a_self_contained_save_ignores_the_stream_session() {
    let server = Server::start();
    let (_owner, docs) = shared_documents("a string the stream defines before and after", 2);
    server.start_save();
    assert_eq!(server.save_record(&docs[0])[0], MARKER_STREAM);

    let text = "a string repeated inside one self-contained value";
    let save = |texts: &[&str]| {
        save_self_contained(io(), |strings, out| {
            let values: Vec<_> = texts.iter().map(|text| strings.string(*text)).collect();
            ciborium::into_writer(&values, out)
        })
        .unwrap();
        let [record] = <[Vec<u8>; 1]>::try_from(drain_records()).unwrap();
        record
    };
    let repeated = save(&[text, text, text]);
    assert_eq!(repeated[0], MARKER_LOCAL);
    let unique = save(&[text]);
    assert_eq!(unique[0], MARKER_INLINE);
    // Each record reads alone, as a RESTORE of a standard export would.
    for record in [&repeated, &unique] {
        assert!(decode_for_load(record, false).is_ok());
    }

    // The stream goes on as if the self-contained values were not there: one definition so far.
    assert_eq!(
        server.save_record(&docs[1])[..5],
        [MARKER_STREAM, 1, 0, 0, 0]
    );
    server.end_save();
}

#[test]
fn without_a_registered_dictionary_an_external_value_stays_plain_beside_streamed_ones() {
    let server = Server::start();
    let owner = IString::from("a string shared by the selected backend");
    let value = document(&owner);
    server.start_save();
    let resident = server.save_record(&value);
    let external = save_external(owner.as_str());
    server.end_save();
    assert_eq!(resident[0], MARKER_STREAM);
    assert_eq!(external, inline_record(&IValue::from(owner.as_str())));
    let loaded = server.load(&Rdb {
        records: vec![resident, external],
    });
    assert_eq!(loaded.values[0], Some(value));
    assert_eq!(loaded.values[1], Some(IValue::from(owner.as_str())));
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
fn external_manager_resets_use_the_shared_framing() {
    let server = Server::start();
    set_dictionary_factory(|| Box::new(ijson::ContentDictionary::default()));
    // 20% of 1280 bytes.
    USED_MEMORY_FOR_TESTS.set(Some(1280));
    server.start_save();
    let texts: Vec<_> = (0..10)
        .map(|i| format!("{i:04}{}", "x".repeat(96)))
        .collect();
    let records: Vec<_> = texts.iter().map(|text| save_external(text)).collect();
    assert!(records.iter().any(|record| record[0] == MARKER_RESET));
    server.end_save();
    let values = server.load(&Rdb { records }).values;
    for (value, text) in values.iter().zip(texts) {
        assert_eq!(value.as_ref().unwrap().as_string().unwrap().as_str(), text);
    }
}

#[test]
fn lifecycle_events_bracket_stream_records_and_repeated_loads() {
    let server = Server::start();
    assert!(!save_session_exists());
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
    // A value without a key name is plain even inside a session, and so is every value after it.
    server.start_save();
    KEYED.set(false);
    assert_eq!(server.save_record(&docs[0]), inline_record(&docs[0]));
    KEYED.set(true);
    server.end_save();
    assert_eq!(server.save_record(&docs[0]), inline_record(&docs[0]));
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
fn restore_without_a_load_accepts_plain_payloads_and_refuses_stream_records() {
    let server = Server::start();
    let (_owner, docs) = shared_documents("a string that only a stream in progress can resolve", 2);
    let rdb = server.save(&docs);
    assert_eq!(
        server.load_record(&inline_record(&docs[0])),
        Some(docs[0].clone())
    );
    assert_eq!(server.load_record(&rdb.records[0]), None);
    assert_eq!(server.load_record(&rdb.records[1]), None);
}

#[test]
fn documents_that_share_nothing_are_written_inline_between_stream_records() {
    let server = Server::start();
    let (_owner, shared) =
        shared_documents("a string that the shared documents of this stream hold", 2);
    let alone: IValue = serde_json::from_str(
        r#"{"only":"a string that no other document of the unshared stream test holds","n":[1,2]}"#,
    )
    .unwrap();
    let numbers: IValue = serde_json::from_str("[1,2,3]").unwrap();
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

#[test]
fn the_library_loader_reports_a_refused_record_without_logging_through_the_io() {
    // CRDT's standard RDB load passes an IO with no module type, and logging through it crashes.
    let server = Server::start();
    let (_owner, docs) = shared_documents("a string a RESTORE cannot resolve without a stream", 2);
    let rdb = server.save(&docs);
    IO_LOGS.set(Vec::new());

    feed(vec![rdb.records[1].clone()]);
    let error = crate::redisjson::type_methods::value_rdb_load_json(io(), 5).unwrap_err();
    assert!(error.to_string().contains("no_stream"), "{error}");
    assert_eq!(IO_LOGS.take(), Vec::<String>::new());

    // The module's own type still logs the refusal through the IO.
    assert!(server.load_record(&rdb.records[1]).is_none());
    assert_eq!(IO_LOGS.take().len(), 1);
}

#[test]
fn stream_records_refused_outside_a_load_warn_at_growing_intervals() {
    // A RESTORE of a stream record means the RDB went to a consumer that reads key by key.
    let server = Server::start();
    let (_owner, docs) = shared_documents("a string a RESTORE cannot resolve without a stream", 2);
    let rdb = server.save(&docs);
    IO_LOGS.set(Vec::new());
    for _ in 0..8 {
        assert!(server.load_record(&rdb.records[1]).is_none());
    }
    let warnings: Vec<String> = IO_LOGS
        .take()
        .into_iter()
        .filter(|log| log.starts_with("warning "))
        .collect();
    assert_eq!(warnings.len(), 4, "{warnings:?}");
    assert!(
        warnings[3].ends_with("reason=no_stream refusals=8"),
        "{warnings:?}"
    );
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

fn repeats_inside(owner: &IString) -> IValue {
    IArray::try_from(vec![
        IValue::from(owner.clone()),
        IValue::from(owner.clone()),
    ])
    .unwrap()
    .into()
}

#[test]
fn a_value_that_repeats_a_string_is_self_contained_without_a_session() {
    let server = Server::start();
    let owner = IString::from("a long string that the value holds twice");
    let value = repeats_inside(&owner);
    // No persistence event: this is a DUMP, a MIGRATE, the ASM snapshot or an IO thread.
    assert!(!save_session_exists());
    let record = server.save_record(&value);
    assert_eq!(&record[..1], &[MARKER_LOCAL]);
    assert!(record.len() < inline_record(&value).len());
    // The same bytes read back with no load session, and with one that belongs to another load.
    assert_eq!(server.load_record(&record), Some(value.clone()));
    loading(raw::REDISMODULE_SUBEVENT_LOADING_RDB_START);
    assert_eq!(server.load_record(&record), Some(value));
    loading(raw::REDISMODULE_SUBEVENT_LOADING_ENDED);

    // An external manager's value is the same: written once, and its callback runs once because
    // the definitions pay for themselves.
    let text = "a long string that the value holds twice";
    let mut runs = 0;
    save_with(io(), |strings, out| {
        runs += 1;
        ciborium::into_writer(&vec![strings.string(text), strings.string(text)], out)
    })
    .unwrap();
    let [external] = <[Vec<u8>; 1]>::try_from(drain_records()).unwrap();
    assert_eq!(runs, 1);
    assert_eq!(&external[..1], &[MARKER_LOCAL]);
    assert_eq!(
        server.load_record(&external),
        Some(IValue::from(vec![IValue::from(text), IValue::from(text)]))
    );
}

#[test]
fn one_large_repeated_string_pays_for_many_definitions_that_are_never_referenced() {
    let server = Server::start();
    // The field names are shared with a retained clone, so each is defined and never referenced
    // inside the value. Referencing the large string once saves far more than they cost.
    let large = IString::from("x".repeat(256 * 1024));
    let mut object = IObject::new();
    for i in 0..20 {
        object
            .insert(format!("shared_field_{i:02}"), IValue::from(i))
            .unwrap();
    }
    object.insert("large_pair", repeats_inside(&large)).unwrap();
    let value = IValue::from(object);
    let _retained = value.clone();
    let record = server.save_record(&value);
    assert_eq!(&record[..1], &[MARKER_LOCAL]);
    assert!(record.len() < inline_record(&value).len() / 2 + 1024);
    assert_eq!(server.load_record(&record), Some(value));

    // The same for an external manager, whose dictionary defines every long string.
    let names: Vec<String> = (0..20).map(|i| format!("shared_field_{i:02}")).collect();
    save_with(io(), |strings, out| {
        let fields: Vec<_> = names
            .iter()
            .map(|name| (strings.string(name.as_str()), 0))
            .collect();
        let pair = vec![
            strings.string(large.as_str()),
            strings.string(large.as_str()),
        ];
        ciborium::into_writer(&(fields, pair), out)
    })
    .unwrap();
    let [external] = <[Vec<u8>; 1]>::try_from(drain_records()).unwrap();
    assert_eq!(&external[..1], &[MARKER_LOCAL]);
    assert!(external.len() < 2 * large.len());
}

#[test]
fn a_self_contained_record_never_touches_the_ids_of_a_stream() {
    let server = Server::start();
    let first = IString::from("the first string of the stream, shared");
    let second = IString::from("the string that the local record defines");
    let local = server.save_record(&repeats_inside(&second));
    assert_eq!(&local[..1], &[MARKER_LOCAL]);
    let docs = [document(&first), document(&first)];
    let rdb = server.save(&docs);
    assert_eq!(rdb.records[0][0], MARKER_STREAM);
    // Inside one load: a stream record defines id 0, a local record defines its own id 0, and the
    // stream record that follows must still resolve id 0 to the stream's string.
    loading(raw::REDISMODULE_SUBEVENT_LOADING_RDB_START);
    assert_eq!(server.load_record(&rdb.records[0]), Some(docs[0].clone()));
    assert_eq!(server.load_record(&local), Some(repeats_inside(&second)));
    assert_eq!(server.load_record(&rdb.records[1]), Some(docs[1].clone()));
    loading(raw::REDISMODULE_SUBEVENT_LOADING_ENDED);
}

#[test]
fn flex_arms_no_session_and_refuses_a_stream_record() {
    let server = Server::start();
    let owner = IString::from("a string that a stream shares between keys");
    let docs = [document(&owner), document(&owner)];
    let stream = server.save(&docs);
    assert_eq!(stream.records[0][0], MARKER_STREAM);

    set_dictionary_factory(|| Box::new(ijson::ContentDictionary::default()));
    BIGREDIS_ENABLED.store(true, Ordering::Relaxed);
    server.start_save();
    assert!(!save_session_exists());
    assert_eq!(server.save_record(&docs[0])[0], MARKER_INLINE);
    // A registered dictionary changes nothing: an external manager's value is plain too.
    let text = "a string owned by an external manager";
    let plain = inline_record(&IValue::from(text));
    assert_eq!(save_external(text), plain);
    assert_eq!(save_external(text), plain);
    server.end_save();
    loading(raw::REDISMODULE_SUBEVENT_LOADING_RDB_START);
    assert!(!load_session_exists());
    assert!(server.load_record(&stream.records[0]).is_none());
    loading(raw::REDISMODULE_SUBEVENT_LOADING_FAILED);
}

#[test]
fn the_save_budget_is_a_share_of_the_memory_the_server_uses() {
    assert_eq!(save_budget(20, Some(1000)), 200);
    assert_eq!(save_budget(100, Some(1000)), 1000);
    // A server that does not report its memory gets no table.
    assert_eq!(save_budget(20, None), 0);
    assert_eq!(
        save_budget(100, Some(u64::MAX)),
        usize::try_from(u64::MAX).unwrap_or(usize::MAX)
    );
}

#[test]
fn a_save_streams_only_when_its_memory_share_leaves_room_for_a_table() {
    let server = Server::start();
    server.start_save();
    assert!(save_session_exists());
    // 20% of 4 bytes is no table at all, and neither is a server that reports no memory.
    for used in [Some(4), None] {
        USED_MEMORY_FOR_TESTS.set(used);
        server.start_save();
        assert!(!save_session_exists());
    }
    // A module argument above 100 counts as 100.
    RDB_DICTIONARY_MAX_MEMORY_PERCENT.store(500, Ordering::Relaxed);
    USED_MEMORY_FOR_TESTS.set(Some(1 << 20));
    server.start_save();
    assert!(save_session_exists());
}

#[test]
fn a_percentage_of_zero_or_less_writes_plain_records_even_inside_a_value() {
    let server = Server::start();
    let owner = IString::from("a long string that the value holds twice");
    let shared = document(&owner);
    let repeated = repeats_inside(&owner);
    let text = "a long string that the value holds twice";
    // A negative module argument counts as zero.
    for percent in [0, -1] {
        RDB_DICTIONARY_MAX_MEMORY_PERCENT.store(percent, Ordering::Relaxed);
        server.start_save();
        assert!(!save_session_exists());
        assert_eq!(server.save_record(&shared), inline_record(&shared));
        assert_eq!(server.save_record(&repeated), inline_record(&repeated));
        save_with(io(), |strings, out| {
            ciborium::into_writer(&vec![strings.string(text), strings.string(text)], out)
        })
        .unwrap();
        let [external] = <[Vec<u8>; 1]>::try_from(drain_records()).unwrap();
        assert_eq!(external[..2], [MARKER_INLINE, 0x82]);
        server.end_save();
    }
}

#[test]
fn a_save_while_slots_are_exported_never_streams() {
    let server = Server::start();
    let (_owner, docs) = shared_documents("a string that two keys share while slots move", 2);
    for end in [
        raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_MIGRATE_COMPLETED,
        raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_MIGRATE_FAILED,
    ] {
        slot_migration(raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_MIGRATE_STARTED);
        server.start_save();
        assert!(!save_session_exists());
        assert_eq!(server.save_record(&docs[0]), inline_record(&docs[0]));
        server.end_save();

        slot_migration(end);
        server.start_save();
        assert!(save_session_exists());
        assert_eq!(server.save_record(&docs[0])[0], MARKER_STREAM);
        server.end_save();
    }
}

#[test]
fn a_save_for_a_syncer_never_streams() {
    let server = Server::start();
    let (_owner, docs) = shared_documents("a string that two keys share for a syncer", 2);
    persistence_with_flags(
        raw::REDISMODULE_SUBEVENT_PERSISTENCE_RDB_START,
        u64::from(raw::REDISMODULE_PERSISTENCE_FLAG_SYNCER),
    );
    assert!(!save_session_exists());
    assert_eq!(server.save_record(&docs[0]), inline_record(&docs[0]));
    server.end_save();

    // Flags the module does not know are ignored, and a core without the flag passes no data.
    persistence_with_flags(raw::REDISMODULE_SUBEVENT_PERSISTENCE_RDB_START, 1 << 8);
    assert!(save_session_exists());
    server.end_save();
    server.start_save();
    assert!(save_session_exists());
    server.end_save();
}

#[test]
fn a_save_streams_again_only_after_every_export_has_ended() {
    let server = Server::start();
    // The end of a task that began before the module loaded must not count below zero.
    slot_migration(raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_MIGRATE_COMPLETED);
    for event in [
        raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_MIGRATE_STARTED,
        raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_MIGRATE_STARTED,
        raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_MIGRATE_FAILED,
    ] {
        slot_migration(event);
    }
    server.start_save();
    assert!(!save_session_exists());
    server.end_save();
    slot_migration(raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_MIGRATE_COMPLETED);
    server.start_save();
    assert!(save_session_exists());
    server.end_save();
}

#[test]
fn the_import_side_of_a_slot_migration_does_not_stop_the_stream() {
    let server = Server::start();
    for event in [
        raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_IMPORT_STARTED,
        raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_IMPORT_FAILED,
        raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_IMPORT_COMPLETED,
    ] {
        slot_migration(event);
        server.start_save();
        assert!(save_session_exists());
        server.end_save();
    }
}
