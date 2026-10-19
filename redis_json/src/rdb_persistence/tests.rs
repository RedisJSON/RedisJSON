/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use super::*;
use ijson::array::ArraySliceRef;
use ijson::{IArray, IObject, IString};

const EVENT: raw::RedisModuleEvent = raw::RedisModuleEvent { id: 0, dataver: 1 };

fn config() -> StreamEncoderConfig {
    StreamEncoderConfig {
        table_budget_bytes: 1 << 20,
        max_def_bytes: 0,
    }
}

// Each test runs on a thread of its own, but nothing here relies on that.
pub(super) fn reset() {
    SAVE_SESSION.set(None);
    LOAD_SESSION.set(None);
}

fn start_saving() {
    reset();
    SAVE_SESSION.set(Some(SaveSession::new(config())));
}

fn start_loading() {
    LOAD_SESSION.set(Some(LoadSession::new()));
}

pub(super) fn save_session_exists() -> bool {
    SAVE_SESSION.with_borrow(Option::is_some)
}

pub(super) fn load_session_exists() -> bool {
    LOAD_SESSION.with_borrow(Option::is_some)
}

pub(super) fn persistence(subevent: u64) {
    // SAFETY: the callback only reads subevent; pointer arguments are unused.
    unsafe { persistence_event(std::ptr::null_mut(), EVENT, subevent, std::ptr::null_mut()) };
}

pub(super) fn loading(subevent: u64) {
    // SAFETY: the callback only reads subevent; pointer arguments are unused.
    unsafe { loading_event(std::ptr::null_mut(), EVENT, subevent, std::ptr::null_mut()) };
}

/// `{"k": text}`; the caller keeps the `IString` alive, so the text has two owners.
pub(super) fn document(text: &IString) -> IValue {
    let mut object = IObject::new();
    object.insert("k", IValue::from(text.clone())).unwrap();
    object.into()
}

fn record(value: &IValue, keyed: bool) -> Vec<u8> {
    let (bytes, start) = encode_for_save(value, keyed, true);
    bytes[start..].to_vec()
}

#[test]
fn loading_start_creates_a_reader() {
    start_saving();
    let owner = IString::from("a resident string shared by two values");
    let value = document(&owner);
    let bytes = record(&value, true);
    loading(raw::REDISMODULE_SUBEVENT_LOADING_RDB_START);
    assert!(load_session_exists());
    assert_eq!(decode_for_load(&bytes, true), Ok(value));
}

#[test]
fn truncated_record_releases_reader_and_prevents_restart_until_next_load() {
    start_saving();
    let owner = IString::from("a string kept until a malformed record");
    let value = document(&owner);
    let bytes = record(&value, true);
    start_loading();
    assert!(decode_for_load(&bytes, true).is_ok());
    assert!(decode_for_load(&[MARKER_STREAM, 0], true).is_err());
    assert!(!load_session_exists());
    assert_eq!(decode_for_load(&bytes, true), Err(LoadError::NoStream));
    loading(raw::REDISMODULE_SUBEVENT_LOADING_FAILED);
    loading(raw::REDISMODULE_SUBEVENT_LOADING_RDB_START);
    assert_eq!(decode_for_load(&bytes, true), Ok(value));
}

/// What a save wrote for a value before streams existed: marker 0 and the CBOR.
pub(super) fn inline_record(value: &IValue) -> Vec<u8> {
    let mut bytes = vec![0];
    bytes.extend_from_slice(&ijson::encode(value));
    bytes
}

#[test]
fn first_use_defines_a_string_and_later_uses_reference_it() {
    start_saving();
    let text = "0123456789abcdefghijklmnopqrstuvwxyzABCD";
    assert_eq!(text.len(), 40);
    let owner = IString::from(text);
    let (first, second) = (document(&owner), document(&owner));

    let mut expected = vec![1, 0, 0, 0, 0, 0xA1, 0x61, 0x6B, 0xC6, 0x78, 0x28];
    expected.extend_from_slice(text.as_bytes());
    assert_eq!(record(&first, true), expected);
    assert_eq!(
        record(&second, true),
        [1, 1, 0, 0, 0, 0xA1, 0x61, 0x6B, 0xC7, 0x00]
    );
    let (stream_docs, inline_docs) = SAVE_SESSION
        .with_borrow(|s| s.as_ref().map(|s| (s.stream_docs, s.inline_docs)))
        .unwrap();
    assert_eq!((stream_docs, inline_docs), (2, 0));
}

#[test]
fn keys_values_nesting_and_typed_arrays_survive_the_stream() {
    start_saving();
    let json = r#"{"a repeated object key, long enough to be shared":
        ["a repeated string value, also long enough","short","שלום שלום שלום",""],
        "nested":{"a repeated object key, long enough to be shared":null}}"#;
    let first: IValue = serde_json::from_str(json).unwrap();
    let second: IValue = serde_json::from_str(json).unwrap();
    let mut typed = IObject::new();
    typed
        .insert("typed", IArray::try_from(vec![1.5f32, 2.5, 3.5]).unwrap())
        .unwrap();
    let typed: IValue = typed.into();

    let records: Vec<Vec<u8>> = [&first, &second, &typed]
        .into_iter()
        .map(|doc| record(doc, true))
        .collect();
    assert_eq!(records[0][0], MARKER_STREAM);
    assert_eq!(records[1][0], MARKER_STREAM);
    assert_eq!(records[2], inline_record(&typed));

    start_loading();
    let loaded: Vec<IValue> = records
        .iter()
        .map(|record| decode_for_load(record, true).unwrap())
        .collect();
    assert_eq!(loaded[0], first);
    assert_eq!(loaded[1], second);
    let array = loaded[2].as_object().unwrap().get("typed").unwrap();
    assert!(matches!(
        array.as_array().unwrap().as_slice(),
        ArraySliceRef::F32(_)
    ));
    assert_eq!(ijson::encode(&loaded[2]), ijson::encode(&typed));
}

#[test]
fn a_poisoned_session_writes_marker_zero_records() {
    start_saving();
    let owner = IString::from("a string that a poisoned encoder must not define");
    let (first, second) = (document(&owner), document(&owner));
    assert_eq!(record(&first, true)[0], MARKER_STREAM);
    SAVE_SESSION.with_borrow_mut(|s| s.as_mut().unwrap().encoder.poison());
    assert_eq!(record(&second, true), inline_record(&second));
    let inline_docs = SAVE_SESSION.with_borrow(|s| s.as_ref().unwrap().inline_docs);
    assert_eq!(inline_docs, 1);
}

#[test]
fn a_small_budget_degrades_gradually_and_every_record_still_loads() {
    reset();
    SAVE_SESSION.set(Some(SaveSession::new(StreamEncoderConfig {
        table_budget_bytes: 1500,
        ..config()
    })));
    let owners: Vec<IString> = (0..64)
        .map(|index| IString::from(format!("budget string {index:04} used twice").as_str()))
        .collect();
    // Every string is used by two documents, one after the other.
    let docs: Vec<IValue> = owners.iter().chain(&owners).map(document).collect();
    let records: Vec<Vec<u8>> = docs.iter().map(|doc| record(doc, true)).collect();

    // The table froze with room for some strings: those keep being referenced, the others
    // stay inline in both uses, byte for byte what an unframed save writes.
    let stats = SAVE_SESSION.with_borrow(|s| s.as_ref().unwrap().encoder.stats());
    assert!(stats.defs > 0 && stats.defs < 64, "{stats:?}");
    assert_eq!(stats.refs, stats.defs);
    for (doc, record) in docs.iter().zip(&records) {
        if record[0] == MARKER_INLINE {
            assert_eq!(*record, inline_record(doc));
        }
    }

    start_loading();
    for (doc, record) in docs.iter().zip(&records) {
        assert_eq!(decode_for_load(record, true).as_ref(), Ok(doc));
    }
}

#[test]
fn save_sessions_never_change_string_owner_counts() {
    start_saving();
    let owner = IString::from("owner count watched during save");
    let (first, second) = (document(&owner), document(&owner));
    let probe = IValue::from(owner.clone());
    let before = probe.mem_allocated();
    record(&first, true);
    record(&second, true);
    assert_eq!(probe.mem_allocated(), before);
    reset();
    assert_eq!(probe.mem_allocated(), before);
}

#[test]
fn markers_two_three_and_unknown_are_refused() {
    start_loading();
    let value: IValue = serde_json::from_str(r#"{"inline":true}"#).unwrap();
    let cbor = ijson::encode(&value);
    for marker in [2u8, 3, 4, 255] {
        let mut bytes = vec![marker];
        bytes.extend_from_slice(&cbor);
        assert_eq!(
            decode_for_load(&bytes, true),
            Err(LoadError::Marker(marker))
        );
    }
    // An empty record and a stream record cut short of its base id.
    assert!(decode_for_load(&[], true).is_err());
    assert!(decode_for_load(&[MARKER_STREAM, 0, 0], true).is_err());
    assert_eq!(decode_for_load(&inline_record(&value), true), Ok(value));
}

#[test]
fn stream_records_need_a_keyed_load_with_a_session() {
    start_saving();
    let owner = IString::from("a string defined by a record loaded out of context");
    let defining = record(&document(&owner), true);

    // Not loading and an unkeyed read each refuse the record.
    reset();
    assert_eq!(decode_for_load(&defining, true), Err(LoadError::NoStream));
    loading(raw::REDISMODULE_SUBEVENT_LOADING_RDB_START);
    assert_eq!(decode_for_load(&defining, false), Err(LoadError::NoStream));
    // The refusal leaves the load as it was.
    assert!(load_session_exists());
    assert!(decode_for_load(&defining, true).is_ok());
}

#[test]
fn base_id_must_match_the_table_length() {
    start_saving();
    let (a, b) = (
        IString::from("first string of the base id test"),
        IString::from("second string of the base id test"),
    );
    // Defines a, references a, defines b, references b.
    let records: Vec<Vec<u8>> = [document(&a), document(&a), document(&b), document(&b)]
        .iter()
        .map(|doc| record(doc, true))
        .collect();
    let bases: Vec<u8> = records.iter().map(|record| record[1]).collect();
    assert_eq!(bases, [0, 1, 1, 2]);

    // In order every record loads, and so do records that only skip a reference.
    for order in [&[0, 1, 2, 3][..], &[0, 2, 3]] {
        start_loading();
        assert!(order
            .iter()
            .all(|index| decode_for_load(&records[*index], true).is_ok()));
    }

    // A skipped, repeated or reordered definition fails loudly and releases the table.
    let scenarios: [(&[usize], usize, usize); 4] = [
        (&[1], 0, 1),
        (&[0, 0], 1, 0),
        (&[2, 0], 0, 1),
        (&[0, 1, 3], 1, 2),
    ];
    for (order, expected, found) in scenarios {
        start_loading();
        let error = order
            .iter()
            .find_map(|index| decode_for_load(&records[*index], true).err());
        let found = found as u32;
        assert_eq!(
            error,
            Some(LoadError::BaseId { expected, found }),
            "{order:?}"
        );
        assert!(!load_session_exists(), "{order:?}");
    }
}

#[test]
fn a_failed_stream_document_releases_the_decoder_table() {
    start_saving();
    let owner = IString::from("a string held by the decoder table until a document fails");
    let (first, second) = (document(&owner), document(&owner));
    let defining = record(&first, true);
    let referencing = record(&second, true);
    let probe = IValue::from(owner.clone());
    let unpinned = probe.mem_allocated();

    start_loading();
    decode_for_load(&defining, true).unwrap();
    assert!(probe.mem_allocated() < unpinned);

    let mut corrupt = vec![MARKER_STREAM, 1, 0, 0, 0];
    corrupt.extend_from_slice(&[0xA1, 0x61, 0x6B, 0xC7, 0x05]);
    assert!(matches!(
        decode_for_load(&corrupt, true),
        Err(LoadError::Cbor(_))
    ));
    assert!(!load_session_exists());
    assert_eq!(probe.mem_allocated(), unpinned);
    assert_eq!(
        decode_for_load(&referencing, true),
        Err(LoadError::NoStream)
    );
}

#[test]
fn flex_io_threads_have_no_session() {
    start_saving();
    start_loading();
    let text = "a string shared across the main thread and an IO thread";
    let owner = IString::from(text);
    let streamed = record(&document(&owner), true);
    assert_eq!(streamed[0], MARKER_STREAM);
    std::thread::spawn(move || {
        assert_eq!(decode_for_load(&streamed, true), Err(LoadError::NoStream));
        let owner = IString::from(text);
        let value = document(&owner);
        let encoded = record(&value, true);
        assert_eq!(encoded, inline_record(&value));
        assert_eq!(decode_for_load(&encoded, true), Ok(value));
    })
    .join()
    .unwrap();
    assert!(save_session_exists() && load_session_exists());
}

#[test]
fn loading_events_arm_and_release_the_load_state() {
    let owner = IString::from("a string defined by the load before a new START");
    start_saving();
    let defining = record(&document(&owner), true);
    for subevent in [
        raw::REDISMODULE_SUBEVENT_LOADING_RDB_START,
        raw::REDISMODULE_SUBEVENT_LOADING_AOF_START,
        raw::REDISMODULE_SUBEVENT_LOADING_REPL_START,
    ] {
        start_loading();
        decode_for_load(&defining, true).unwrap();
        loading(subevent);
        // A fresh reader: the table of the load before is gone.
        assert!(
            load_session_exists() && !load_session_is_live(),
            "start {subevent}"
        );
    }
    for subevent in [
        raw::REDISMODULE_SUBEVENT_LOADING_ENDED,
        raw::REDISMODULE_SUBEVENT_LOADING_FAILED,
    ] {
        start_loading();
        loading(subevent);
        assert!(!load_session_exists(), "end {subevent}");
    }
    // Flex adds intermediate events; they do not end the full load.
    for subevent in [5, 6, 7] {
        start_loading();
        loading(subevent);
        assert!(load_session_exists(), "event {subevent}");
    }
    reset();
}

#[test]
fn defrag_leaves_the_string_cache_alone_while_a_stream_is_loading() {
    start_saving();
    let owner = IString::from("a string that a decoder holds while defrag starts");
    let defining = record(&document(&owner), true);
    reset();

    start_loading();
    assert!(!load_session_is_live());
    decode_for_load(&defining, true).unwrap();
    assert!(load_session_is_live());
    loading(raw::REDISMODULE_SUBEVENT_LOADING_ENDED);
    assert!(!load_session_is_live());
}
