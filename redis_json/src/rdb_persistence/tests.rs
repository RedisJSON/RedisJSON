/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

use super::*;
use ijson::{IObject, IString};
use std::sync::{Mutex, MutexGuard, PoisonError};

// The writer settings and the Flex flag are process-wide, so the tests that change them or depend
// on them take turns.
pub(super) fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

const EVENT: raw::RedisModuleEvent = raw::RedisModuleEvent { id: 0, dataver: 1 };

fn config() -> StreamEncoderConfig {
    StreamEncoderConfig {
        table_budget_bytes: 1 << 20,
    }
}

// Each test runs on a thread of its own, but nothing here relies on that.
pub(super) fn reset() {
    SAVE_SESSION.set(None);
    LOAD_SESSION.set(None);
    DICTIONARY_FACTORY.set(None);
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

pub(super) fn slot_migration(subevent: u64) {
    // SAFETY: the callback only reads subevent; pointer arguments are unused.
    unsafe { slot_migration_event(std::ptr::null_mut(), EVENT, subevent, std::ptr::null_mut()) };
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
    let (bytes, start) = encode_for_save(value, keyed);
    bytes[start..].to_vec()
}

/// A plain record: marker 0 and the CBOR.
pub(super) fn inline_record(value: &IValue) -> Vec<u8> {
    let mut bytes = vec![0];
    bytes.extend_from_slice(&ijson::encode(value));
    bytes
}

#[test]
fn first_use_defines_a_string_and_later_uses_reference_it() {
    let _serial = serial();
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
fn full_dictionary_restarts_before_later_definitions_and_preserves_record_order() {
    let _serial = serial();
    reset();
    SAVE_SESSION.set(Some(SaveSession::new(StreamEncoderConfig {
        table_budget_bytes: 84,
    })));
    let owners: Vec<IString> = (0..8)
        .map(|i| IString::from(format!("reset string {i:04}")))
        .collect();
    let docs: Vec<IValue> = [0, 1, 2, 3, 3, 4, 5, 6, 6, 7, 7]
        .iter()
        .map(|&i| document(&owners[i]))
        .collect();
    let records: Vec<_> = docs.iter().map(|doc| record(doc, true)).collect();
    assert_eq!(
        records.iter().map(|bytes| bytes[0]).collect::<Vec<_>>(),
        [1, 1, 1, 0, 2, 1, 1, 0, 2, 1, 1]
    );
    // A string that finds no room in the table stays inline, byte for byte a plain record.
    for (doc, bytes) in docs.iter().zip(&records) {
        if bytes[0] == MARKER_INLINE {
            assert_eq!(*bytes, inline_record(doc));
        }
    }
    assert_eq!(
        SAVE_SESSION.with_borrow(|s| s.as_ref().unwrap().encoder.stats().defs),
        8
    );
    start_loading();
    let mut loaded = Vec::new();
    for (doc, bytes) in docs.iter().zip(&records) {
        loaded.push(decode_for_load(bytes, true).unwrap());
        assert_eq!(loaded.last(), Some(doc));
        assert!(LOAD_SESSION.with_borrow(|s| s.as_ref().unwrap().decoder.len()) <= 3);
    }
    assert_eq!(loaded, docs);
    for order in [&[0, 1, 2, 5][..], &[0, 1, 2, 4, 4], &[8]] {
        start_loading();
        assert!(order
            .iter()
            .any(|&i| decode_for_load(&records[i], true).is_err()));
        assert!(!load_session_exists());
    }
}

#[test]
fn pending_reset_waits_through_inline_values_and_releases_reader_handles() {
    let _serial = serial();
    reset();
    SAVE_SESSION.set(Some(SaveSession::new(StreamEncoderConfig {
        table_budget_bytes: 84,
    })));
    let owners: Vec<IString> = (0..4)
        .map(|i| IString::from(format!("pending reset string {i:04}")))
        .collect();
    let docs: Vec<_> = owners.iter().map(document).collect();
    let probe = IValue::from(owners[0].clone());
    let unpinned = probe.mem_allocated();
    let mut records: Vec<_> = docs.iter().map(|doc| record(doc, true)).collect();
    let inline = IValue::from(42);
    records.push(record(&inline, true));
    assert_eq!(records.last().unwrap(), &inline_record(&inline));
    records.push(record(&docs[3], true));
    assert_eq!(records.last().unwrap()[0], 2);
    start_loading();
    for bytes in &records[..records.len() - 1] {
        decode_for_load(bytes, true).unwrap();
    }
    assert!(probe.mem_allocated() < unpinned);
    assert_eq!(
        decode_for_load(records.last().unwrap(), true),
        Ok(docs[3].clone())
    );
    assert_eq!(probe.mem_allocated(), unpinned);
    reset();
    assert_eq!(
        decode_for_load(records.last().unwrap(), true),
        Err(LoadError::NoStream)
    );
}

#[test]
fn a_poisoned_session_writes_marker_zero_records() {
    let _serial = serial();
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
fn save_sessions_never_change_string_owner_counts() {
    let _serial = serial();
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
fn a_self_contained_record_is_refused_when_its_ids_do_not_resolve() {
    let _serial = serial();
    // A reference before any definition, a definition under 8 bytes, and bytes after the value.
    for cbor in [
        &[0xC7, 0x00][..],
        &[0xC6, 0x65, b's', b'h', b'o', b'r', b't'][..],
        &[0xF6, 0x00][..],
    ] {
        let mut record = vec![MARKER_LOCAL];
        record.extend_from_slice(cbor);
        assert!(
            matches!(decode_for_load(&record, false), Err(LoadError::Cbor(_))),
            "{record:02x?}"
        );
    }
}

#[test]
fn markers_four_and_above_are_refused() {
    let _serial = serial();
    start_loading();
    let value: IValue = serde_json::from_str(r#"{"inline":true}"#).unwrap();
    let cbor = ijson::encode(&value);
    for marker in [4u8, 255] {
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
fn reset_records_without_definitions_or_with_invalid_references_are_refused() {
    let _serial = serial();
    for payload in [&[0xf6][..], &[0xc7, 0], &[]] {
        start_loading();
        let mut bytes = vec![MARKER_RESET, 0, 0, 0, 0];
        bytes.extend_from_slice(payload);
        assert!(matches!(
            decode_for_load(&bytes, true),
            Err(LoadError::Cbor(_))
        ));
        assert!(!load_session_exists());
    }
    start_loading();
    assert!(decode_for_load(&[MARKER_RESET, 0], true).is_err());
    assert!(!load_session_exists());
}

#[test]
fn stream_records_need_a_keyed_load_with_a_session() {
    let _serial = serial();
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
    let _serial = serial();
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
    let _serial = serial();
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
    let _serial = serial();
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
    let _serial = serial();
    let owner = IString::from("a string defined by the load before a new START");
    start_saving();
    let defining = record(&document(&owner), true);
    // Active defrag leaves the string cache alone while the session holds decoded strings.
    for subevent in [
        raw::REDISMODULE_SUBEVENT_LOADING_RDB_START,
        raw::REDISMODULE_SUBEVENT_LOADING_AOF_START,
        raw::REDISMODULE_SUBEVENT_LOADING_REPL_START,
    ] {
        start_loading();
        assert!(!load_session_is_live());
        decode_for_load(&defining, true).unwrap();
        assert!(load_session_is_live());
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
        decode_for_load(&defining, true).unwrap();
        loading(subevent);
        assert!(
            !load_session_exists() && !load_session_is_live(),
            "end {subevent}"
        );
    }
    // The standard subevents are 0 to 4. Flex adds ones above them, and they do not end the load.
    for subevent in [5, 6, 7] {
        start_loading();
        loading(subevent);
        assert!(load_session_exists(), "event {subevent}");
    }
    reset();
}
