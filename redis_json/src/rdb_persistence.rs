/*
 * Copyright (c) 2006-Present, Redis Ltd.
 * All rights reserved.
 *
 * Licensed under your choice of (a) the Redis Source Available License 2.0
 * (RSALv2); or (b) the Server Side Public License v1 (SSPLv1); or (c) the
 * GNU Affero General Public License v3 (AGPLv3).
 */

//! RDB v5 save support shared by JSON value managers.
//!
//! [`save_with`] lets a manager serialize its own values while this module owns
//! dictionary lifetime, budgets and record framing. Initialization is performed
//! by `redis_json_module_create!`.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};

use ijson::{
    CborDecodeError, ContentDictionary, DocUse, IValue, StreamDecoder, StreamEncoder,
    StreamEncoderConfig, StreamEncoderStats, StreamStrings, StringDictionary,
};
use redis_module::logging::{log_io_error, RedisLogLevel};
use redis_module::{raw, Context, Status};

use crate::backward;

// Two ways to write a string that repeats.
// A stream: a string that several values share is written to an RDB once, where it is first used,
// and as a small id afterwards; the ids are implicit, in the order of the first uses. The writer
// keeps its table in a session that lives between persistence START and ENDED/FAILED events, and
// the reader rebuilds the same table in a session of its own. Records of a stream depend on the
// ones before them.
// A self-contained value (every save without a session: Flex, DUMP, MIGRATE, the ASM snapshot,
// an IO thread): a fresh encoder writes a string that repeats inside the one value once, with ids
// that start at 0 and end with the record. Nothing is shared between values, so a fresh decoder
// reads it with no session and no key.

// A version 5 value record starts with a marker.
// 0: plain CBOR.
// 1: a base id (u32 LE),
// then CBOR that defines and references the strings of one RDB stream (ijson's StreamEncoder).
const MARKER_INLINE: u8 = 0;
const MARKER_STREAM: u8 = 1;
// Marker 2 clears the reader dictionary before new definitions; base IDs stay cumulative.
const MARKER_RESET: u8 = 2;
// Marker 3: self-contained CBOR that defines and references strings; its ids belong to this
// record alone, so it has no base id and needs no session.
const MARKER_LOCAL: u8 = 3;
// The encoder appends after these bytes: the marker 1 header, or four unused bytes and marker 0.
// Marker 2 uses the same header as marker 1.
const STREAM_HEADER_LEN: usize = 5;

// Ceiling on the table the writer builds in the saving process; 0 turns the writer off.
// When it is reached the strings defined so far keep being referenced and new ones go inline.
// After a budget miss, the next definition-capable document reuses the cleared table.
pub const DEFAULT_RDB_DICTIONARY_MAX_BYTES: i64 = 1024 * 1024 * 1024;
pub const RDB_DICTIONARY_MAX_BYTES_CEILING: i64 = 1 << 40; // 1TB
pub static RDB_DICTIONARY_MAX_BYTES: AtomicI64 = AtomicI64::new(DEFAULT_RDB_DICTIONARY_MAX_BYTES);
// Whether the server runs Flex; set once by init(). Flex never writes or reads a stream.
static BIGREDIS_ENABLED: AtomicBool = AtomicBool::new(false);
// Slot migrations this node is exporting. A fork child started meanwhile may write the migration
// snapshot, which the destination loads without a session, so such a save never streams.
static ASM_EXPORTS: AtomicUsize = AtomicUsize::new(0);

/// Creates a fresh dictionary for each save. Without a custom factory, saves use
/// [`ijson::AddressDictionary`]; custom factories may supply any [`StringDictionary`].
/// Factories must return empty dictionaries whose entry allocations fit the configured budget.
pub type DictionaryFactory = fn() -> Box<dyn StringDictionary>;

/// Selects a custom dictionary after the consumer has initialized its backend.
/// Call only from the module's `init` callback, on the Redis main thread.
/// Omitting this call keeps [`ijson::AddressDictionary`]. Each save creates a fresh
/// dictionary from this factory and uses it for the entire stream.
pub fn set_dictionary_factory(factory: DictionaryFactory) {
    DICTIONARY_FACTORY.set(Some(factory));
}

thread_local! {
    // Initialization and persistence callbacks run on the main thread, including its fork child.
    static DICTIONARY_FACTORY: Cell<Option<DictionaryFactory>> = const { Cell::new(None) };
    // Redis invokes a stream's lifecycle and value callbacks on the same thread.
    // The load session owns string handles; cleanup never resets the live intern cache.
    // Persistence events bracket synchronous key callbacks. Flex IO threads have no session.
    // A save session lives from persistence START to the end of the save. A load session lives
    // from loading START to the end of the load, or until a stream record fails.
    static SAVE_SESSION: RefCell<Option<SaveSession>> = const { RefCell::new(None) };
    static LOAD_SESSION: RefCell<Option<LoadSession>> = const { RefCell::new(None) };
}

/// The writer of one RDB stream. Dropping it forgets which strings were defined, so every later
/// record of the stream is self-contained.
struct SaveSession {
    encoder: Box<dyn SaveEncoder>,
    reset_pending: bool,
    // Times the table filled up and was cleared; a save that logs any ran into the ceiling.
    resets: u64,
    stream_docs: u64,
    inline_docs: u64,
}

type EncodeCallback<'a> = dyn FnMut(&RdbStringEncoder<'_>, &mut Vec<u8>) -> Result<(), ()> + 'a;

// Dispatch once per document so the default IValue writer keeps static string lookups.
trait SaveEncoder {
    fn stats(&self) -> StreamEncoderStats;
    fn is_poisoned(&self) -> bool;
    fn reset_if_needed(&mut self) -> bool;
    fn encode_into(&mut self, out: &mut Vec<u8>, value: &IValue) -> DocUse;
    fn encode_with(
        &mut self,
        out: &mut Vec<u8>,
        encode: &mut EncodeCallback<'_>,
    ) -> Result<DocUse, ()>;
    #[cfg(test)]
    fn poison(&mut self);
}

impl<D: StringDictionary> SaveEncoder for StreamEncoder<D> {
    fn stats(&self) -> StreamEncoderStats {
        self.stats()
    }
    fn is_poisoned(&self) -> bool {
        self.is_poisoned()
    }
    fn reset_if_needed(&mut self) -> bool {
        self.reset_if_needed()
    }
    fn encode_into(&mut self, out: &mut Vec<u8>, value: &IValue) -> DocUse {
        self.encode_into(out, value)
    }
    fn encode_with(
        &mut self,
        out: &mut Vec<u8>,
        encode: &mut EncodeCallback<'_>,
    ) -> Result<DocUse, ()> {
        self.encode_with(out, encode)
    }
    #[cfg(test)]
    fn poison(&mut self) {
        self.poison();
    }
}

fn make_encoder(config: StreamEncoderConfig) -> Box<dyn SaveEncoder> {
    match DICTIONARY_FACTORY.get() {
        None => Box::new(StreamEncoder::new(config)),
        Some(factory) => Box::new(StreamEncoder::with_dictionary(config, factory())),
    }
}

impl SaveSession {
    fn new(config: StreamEncoderConfig) -> Self {
        Self {
            encoder: make_encoder(config),
            reset_pending: false,
            resets: 0,
            stream_docs: 0,
            inline_docs: 0,
        }
    }

    /// Encodes a value of this stream as `bytes[start..]`. The encoder appends after the header
    /// bytes, so a document that uses no string of the stream skips the first four of them and
    /// keeps the marker 0 that fills the last.
    fn encode(&mut self, value: &IValue) -> (Vec<u8>, usize) {
        let (base_id, mut bytes) = self.begin_record();
        let used = self.encoder.encode_into(&mut bytes, value);
        self.finish_record(bytes, base_id, used)
    }

    fn encode_with<E>(
        &mut self,
        encode: impl FnOnce(&RdbStringEncoder<'_>, &mut Vec<u8>) -> Result<(), E>,
    ) -> Result<(Vec<u8>, usize), E> {
        let (base_id, mut bytes) = self.begin_record();
        let mut encode = Some(encode);
        let mut error = None;
        let used = self.encoder.encode_with(&mut bytes, &mut |strings, out| {
            encode.take().expect("one callback per document")(strings, out)
                .map_err(|e| error = Some(e))
        });
        match used {
            Ok(used) => Ok(self.finish_record(bytes, base_id, used)),
            Err(()) => Err(error.expect("callback error recorded above")),
        }
    }

    fn begin_record(&mut self) -> (u32, Vec<u8>) {
        let base_id = self.encoder.stats().defs as u32;
        if self.encoder.reset_if_needed() {
            self.reset_pending = true;
            self.resets += 1;
        }
        (base_id, vec![MARKER_INLINE; STREAM_HEADER_LEN])
    }

    fn finish_record(
        &mut self,
        mut bytes: Vec<u8>,
        base_id: u32,
        used: DocUse,
    ) -> (Vec<u8>, usize) {
        if used == DocUse::default() {
            self.inline_docs += 1;
            return (bytes, STREAM_HEADER_LEN - 1);
        }
        bytes[0] = if std::mem::take(&mut self.reset_pending) {
            MARKER_RESET
        } else {
            MARKER_STREAM
        };
        bytes[1..STREAM_HEADER_LEN].copy_from_slice(&base_id.to_le_bytes());
        self.stream_docs += 1;
        (bytes, 0)
    }

    fn summary(&self, phase: &str) -> String {
        let stats = self.encoder.stats();
        format!(
            "JSON RDB dictionary phase={phase} defs={} def_bytes={} refs={} resets={} stream_docs={} inline_docs={} poisoned={}",
            stats.defs, stats.def_bytes, stats.refs, self.resets, self.stream_docs, self.inline_docs,
            self.encoder.is_poisoned(),
        )
    }
}

/// The reader of one RDB stream: the strings its records defined so far, in the order they came.
struct LoadSession {
    decoder: StreamDecoder,
    stream_docs: u64,
}

impl LoadSession {
    fn new() -> Self {
        Self {
            decoder: StreamDecoder::new(),
            stream_docs: 0,
        }
    }

    fn summary(&self, phase: &str) -> String {
        let stats = self.decoder.stats();
        format!(
            "JSON RDB dictionary phase={phase} defs={} def_bytes={} refs={} stream_docs={}",
            stats.defs, stats.def_bytes, stats.refs, self.stream_docs,
        )
    }
}

/// Whether this thread's load session holds strings, for active defrag, which must not reset the
/// string cache under a decoder that holds strings interned in it.
pub(crate) fn load_session_is_live() -> bool {
    LOAD_SESSION.with_borrow(|session| {
        session
            .as_ref()
            .is_some_and(|session| !session.decoder.is_empty())
    })
}

// A module argument skips the range check of CONFIG SET, so a value outside the range of its
// setting counts as the nearest bound: a negative budget switches the writer off instead of
// lifting its ceiling.
fn config_value(setting: &AtomicI64) -> usize {
    usize::try_from(
        setting
            .load(Ordering::Relaxed)
            .clamp(0, RDB_DICTIONARY_MAX_BYTES_CEILING),
    )
    .unwrap_or(usize::MAX)
}

// The module API that logs does not exist under cargo test, where the message is only checked:
// the log helper may pass it to the server as a printf format. The callbacks cannot unwind, so a
// percent character aborts the test run after printing the message.
fn log_io(rdb: *mut raw::RedisModuleIO, level: RedisLogLevel, message: &str) {
    if cfg!(test) {
        assert!(!message.contains('%'), "{message}");
    } else {
        log_io_error(rdb, level, message);
    }
}

// Server events have no IO handle to log through.
fn log_event(ctx: *mut raw::RedisModuleCtx, message: &str) {
    if cfg!(test) {
        assert!(!message.contains('%'), "{message}");
    } else {
        Context::new(ctx).log_notice(message);
    }
}

pub(crate) fn init(ctx: &Context) -> Status {
    // bigredis-enabled is immutable; missing config on OSS means RAM-only Redis.
    let flex = crate::read_yes_no_config(ctx, "bigredis-enabled");
    BIGREDIS_ENABLED.store(flex, Ordering::Relaxed);
    for (id, callback) in [
        (
            raw::REDISMODULE_EVENT_PERSISTENCE,
            persistence_event as unsafe extern "C" fn(_, _, _, _),
        ),
        (
            raw::REDISMODULE_EVENT_LOADING,
            loading_event as unsafe extern "C" fn(_, _, _, _),
        ),
    ] {
        let event = raw::RedisModuleEvent { id, dataver: 1 };
        if raw::subscribe_to_server_event(ctx.ctx, event, Some(callback)) != Status::Ok {
            return Status::Err;
        }
    }
    // A server without atomic slot migration has no such event and no snapshot to protect.
    let migration = raw::RedisModuleEvent {
        id: raw::REDISMODULE_EVENT_CLUSTER_SLOT_MIGRATION,
        dataver: 1,
    };
    let _ = raw::subscribe_to_server_event(ctx.ctx, migration, Some(slot_migration_event));
    // A command can be invoked by another module during SAVE, or after an aborted
    // module-initiated RdbSave that did not publish a persistence-ended event.
    // Stop sharing before such a command can produce a standalone DUMP/MIGRATE.
    // SAFETY: Redis owns ctx and retains this callback; flags 0 include module calls.
    let filter = unsafe {
        raw::RedisModule_RegisterCommandFilter.unwrap()(ctx.ctx, Some(command_filter), 0)
    };
    if filter.is_null() {
        Status::Err
    } else {
        Status::Ok
    }
}

fn has_key(rdb: *mut raw::RedisModuleIO) -> bool {
    // SAFETY: rdb is supplied by Redis. The borrowed key is only checked for null.
    unsafe { !raw::RedisModule_GetKeyNameFromIO.unwrap()(rdb).is_null() }
}

/// Settings of the encoder of one self-contained value. A budget of 0 defines nothing, so the
/// record is plain.
fn local_config() -> StreamEncoderConfig {
    StreamEncoderConfig {
        table_budget_bytes: config_value(&RDB_DICTIONARY_MAX_BYTES),
    }
}

/// Whether a value with this many definitions and references is smaller than its plain form. A
/// definition adds 1 byte (tag 6). A reference replaces text of 8 or more bytes (9 encoded) with
/// tag 7 and an id of at most 5 bytes, so it saves at least 3.
fn pays(used: DocUse) -> bool {
    used.refs * 3 > used.defs
}

/// A value outside any stream. A fresh encoder defines the strings that repeat inside the value.
/// The record keeps that form (marker 3) only when it is smaller than the plain one. Strings that
/// other values share but this one holds once are defined and never referenced, which is what the
/// size check pays for.
fn encode_value(value: &IValue) -> Vec<u8> {
    let mut bytes = vec![MARKER_LOCAL];
    let used = StreamEncoder::new(local_config()).encode_into(&mut bytes, value);
    if used == DocUse::default() {
        // Nothing was defined: the bytes are the plain ones.
        bytes[0] = MARKER_INLINE;
    } else if !pays(used) {
        bytes.clear();
        bytes.push(MARKER_INLINE);
        ijson::encode_into(value, &mut bytes);
    }
    bytes
}

/// Encodes a value as `bytes[start..]`. A keyed value is encoded by the session of the save, when
/// it has one. Saves without a session (Flex, the writer is off), keyless values, and poisoned
/// encoders stay self-contained.
fn encode_for_save(value: &IValue, keyed: bool) -> (Vec<u8>, usize) {
    keyed
        .then(|| SAVE_SESSION.with_borrow_mut(|session| session.as_mut().map(|s| s.encode(value))))
        .flatten()
        .unwrap_or_else(|| (encode_value(value), 0))
}

pub(crate) fn save_value(rdb: *mut raw::RedisModuleIO, value: &IValue) {
    // SaveDataTypeToString/Serialize have no key and always remain self-contained.
    // DUMP, MIGRATE and the ASM slot snapshot do carry a key: the command filter ends the session
    // before any command runs, and the ASM child has none.
    let (bytes, start) = encode_for_save(value, has_key(rdb));
    // A failed write fails the whole save, so a record that was encoded but not written is never
    // followed by one that refers to the strings it defined.
    raw::save_slice(rdb, &bytes[start..]);
}

/// String wrappers for an external manager's CBOR serializer.
pub type RdbStringEncoder<'a> = StreamStrings<'a>;

/// Saves one external manager value using the current RDB v5 stream.
///
/// `rdb` must be the IO handle from this value's save callback. The callback writes
/// one complete CBOR document in wire order, wrapping object keys and string
/// values with [`RdbStringEncoder::string`]. It must not run Redis commands or
/// recursively call a save helper. Plain non-string values use normal Serde encoding.
///
/// The optional call to [`set_dictionary_factory`] from the module's `init` callback selects
/// the dictionary once at initialization; omitting it uses [`ijson::AddressDictionary`].
/// Every manager in a save shares that dictionary.
///
/// Dictionaries obey `json-rdb-dictionary-max-bytes` through their `insert` method.
/// With [`ijson::ContentDictionary`], source strings need only live through this
/// callback. Borrowing implementations must keep defined storage alive and
/// unchanged for the stream.
/// Unkeyed saves and saves outside a persistence session are self-contained: a string that
/// repeats inside the value is written once. The callback runs a second time when the strings it
/// defined do not pay for themselves and the plain form is written instead, so it must serialize
/// the same value each time.
///
/// Serialization errors write no record and disable subsequent references. The
/// manager must propagate the error through its save callback so Redis aborts the save.
///
/// ```no_run
/// # fn save(rdb: *mut redis_module::raw::RedisModuleIO, text: &str) {
/// rejson::rdb_persistence::save_with(rdb, |strings, out| {
///     ciborium::into_writer(&strings.string(text), out)
/// }).unwrap();
/// # }
/// ```
pub fn save_with<E>(
    rdb: *mut raw::RedisModuleIO,
    mut encode: impl FnMut(&RdbStringEncoder<'_>, &mut Vec<u8>) -> Result<(), E>,
) -> Result<(), E> {
    let keyed = has_key(rdb);
    let (bytes, start) = SAVE_SESSION.with_borrow_mut(|slot| {
        if let Some(session) = slot.as_mut().filter(|_| keyed) {
            session.encode_with(encode)
        } else {
            let mut bytes = vec![MARKER_LOCAL];
            let used = StreamEncoder::with_dictionary(local_config(), ContentDictionary::default())
                .encode_with(&mut bytes, &mut encode)?;
            if used == DocUse::default() {
                bytes[0] = MARKER_INLINE;
            } else if !pays(used) {
                bytes.clear();
                bytes.push(MARKER_INLINE);
                StreamEncoder::new(StreamEncoderConfig {
                    table_budget_bytes: 0,
                })
                .encode_with(&mut bytes, &mut encode)?;
            }
            Ok((bytes, 0))
        }
    })?;
    raw::save_slice(rdb, &bytes[start..]);
    Ok(())
}

/// Why a value record was refused. Redis only learns that the load failed, so this is logged.
/// Refusing a record while a file loads stops the server, and its exit status can be 0: the RDB
/// checker it runs on the file skips module values. The log line is the only sign of the cause.
#[derive(Debug, PartialEq, Eq)]
enum LoadError {
    /// A marker that is reserved or unknown.
    Marker(u8),
    /// A stream record outside a keyed load that armed a session.
    NoStream,
    /// The record does not start where the table of defined strings ends, so an earlier record
    /// that defines strings was skipped, repeated or reordered.
    BaseId {
        expected: usize,
        found: u32,
    },
    Cbor(CborDecodeError),
}

impl From<CborDecodeError> for LoadError {
    fn from(error: CborDecodeError) -> Self {
        Self::Cbor(error)
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Marker(marker) => write!(f, "marker_{marker}"),
            Self::NoStream => f.write_str("no_stream"),
            Self::BaseId { expected, found } => {
                write!(f, "base_id_mismatch expected={expected} found={found}")
            }
            Self::Cbor(error) => write!(f, "cbor_{error:?}"),
        }
    }
}

pub(crate) fn load_value(rdb: *mut raw::RedisModuleIO) -> Option<IValue> {
    // A stream record that fails releases the session, so ask before decoding.
    let loading = LOAD_SESSION.with_borrow(Option::is_some);
    let bytes = match raw::load_string_buffer(rdb) {
        Ok(bytes) => bytes,
        Err(_) => {
            LOAD_SESSION.set(None);
            return None;
        }
    };
    match decode_for_load(bytes.as_ref(), has_key(rdb)) {
        Ok(value) => Some(value),
        Err(error) => {
            let level = if loading {
                RedisLogLevel::Warning
            } else {
                RedisLogLevel::Verbose
            };
            log_io(
                rdb,
                level,
                &format!("JSON RDB dictionary phase=load status=refused reason={error}"),
            );
            None
        }
    }
}

fn decode_for_load(bytes: &[u8], keyed: bool) -> Result<IValue, LoadError> {
    let limit_depth = backward::legacy_rdb_depth_limit_enabled();
    match bytes.split_first() {
        Some((&MARKER_INLINE, cbor)) => {
            ijson::decode_with_depth_limit(cbor, limit_depth).map_err(LoadError::from)
        }
        Some((&MARKER_LOCAL, cbor)) => StreamDecoder::new()
            .decode(cbor, limit_depth)
            .map_err(LoadError::from),
        Some((&marker @ (MARKER_STREAM | MARKER_RESET), record)) => {
            decode_stream_record(record, keyed, limit_depth, marker == MARKER_RESET)
        }
        Some((&marker, _)) => Err(LoadError::Marker(marker)),
        None => Err(CborDecodeError::DecodeError.into()),
    }
}

fn decode_stream_record(
    record: &[u8],
    keyed: bool,
    limit_depth: bool,
    reset_dictionary: bool,
) -> Result<IValue, LoadError> {
    LOAD_SESSION.with_borrow_mut(|slot| {
        // Only a loading START event can arm a stream, so RESTORE and reads
        // without a key can never resolve one.
        let session = slot.as_mut().filter(|_| keyed).ok_or(LoadError::NoStream)?;
        let result =
            decode_stream_document(&mut session.decoder, record, limit_depth, reset_dictionary);
        match &result {
            Ok(_) => session.stream_docs += 1,
            // The decoder refuses every record after a failure, so its table is released now.
            Err(_) => *slot = None,
        }
        result
    })
}

fn decode_stream_document(
    decoder: &mut StreamDecoder,
    record: &[u8],
    limit_depth: bool,
    reset_dictionary: bool,
) -> Result<IValue, LoadError> {
    let (base_id, document) = record
        .split_first_chunk()
        .ok_or(CborDecodeError::DecodeError)?;
    let found = u32::from_le_bytes(*base_id);
    let expected = decoder.stats().defs as usize;
    if found as usize != expected {
        return Err(LoadError::BaseId { expected, found });
    }
    if reset_dictionary {
        decoder.reset();
    }
    let value = decoder.decode(document, limit_depth)?;
    // A reset must advance the cumulative position so dropping its record is detectable.
    if reset_dictionary && decoder.stats().defs as usize == expected {
        return Err(CborDecodeError::InvalidValue.into());
    }
    Ok(value)
}

unsafe extern "C" fn command_filter(_ctx: *mut raw::RedisModuleCommandFilterCtx) {
    SAVE_SESSION.set(None);
}

/// Whether this save may share strings across keys. A stream makes each record depend on the ones
/// before it, so it is only for a save that Redis reads back through its loading events in order.
/// Flex never writes one: it can keep any value on flash undecoded and decode it later alone.
/// Neither does a save while this node exports slots: the destination loads that snapshot without
/// a session. Neither does a save for a syncer, which an Enterprise replica-of uses: it converts
/// the RDB into one RESTORE per key and cannot read a stream record. The core says so in the flags
/// of the persistence event.
fn cross_key_stream_allowed(flex: bool, budget: usize, asm_export: bool, syncer: bool) -> bool {
    !flex && budget > 0 && !asm_export && !syncer
}

/// Whether the core marked this save as one that feeds a syncer. A core that predates the flag
/// passes no data.
///
/// # Safety
/// `data` is null or points to the `RedisModulePersistenceInfo` the core passed to the callback.
unsafe fn saves_for_syncer(data: *mut c_void) -> bool {
    // SAFETY: the caller guarantees `data` is null or valid for reads.
    let info = unsafe { data.cast::<raw::RedisModulePersistenceInfo>().as_ref() };
    info.is_some_and(|info| info.flags & u64::from(raw::REDISMODULE_PERSISTENCE_FLAG_SYNCER) != 0)
}

unsafe extern "C" fn slot_migration_event(
    _ctx: *mut raw::RedisModuleCtx,
    _event: raw::RedisModuleEvent,
    subevent: u64,
    _data: *mut c_void,
) {
    match subevent {
        raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_MIGRATE_STARTED => {
            ASM_EXPORTS.fetch_add(1, Ordering::Relaxed);
        }
        raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_MIGRATE_COMPLETED
        | raw::REDISMODULE_SUBEVENT_CLUSTER_SLOT_MIGRATION_MIGRATE_FAILED => {
            // A module loaded while a task runs sees its end without its start.
            let _ = ASM_EXPORTS
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1));
        }
        _ => {}
    }
}

unsafe extern "C" fn persistence_event(
    ctx: *mut raw::RedisModuleCtx,
    _event: raw::RedisModuleEvent,
    subevent: u64,
    data: *mut c_void,
) {
    // Whatever starts or ends a save, the session of an earlier one must not outlive it.
    let phase = match subevent {
        raw::REDISMODULE_SUBEVENT_PERSISTENCE_RDB_START
        | raw::REDISMODULE_SUBEVENT_PERSISTENCE_AOF_START
        | raw::REDISMODULE_SUBEVENT_PERSISTENCE_SYNC_RDB_START
        | raw::REDISMODULE_SUBEVENT_PERSISTENCE_SYNC_AOF_START => {
            let budget = config_value(&RDB_DICTIONARY_MAX_BYTES);
            let flex = BIGREDIS_ENABLED.load(Ordering::Relaxed);
            let asm_export = ASM_EXPORTS.load(Ordering::Relaxed) > 0;
            // Without a session every value is self-contained.
            // SAFETY: the core passes null or a `RedisModulePersistenceInfo` for these subevents.
            let syncer = unsafe { saves_for_syncer(data) };
            SAVE_SESSION.set(
                cross_key_stream_allowed(flex, budget, asm_export, syncer).then(|| {
                    SaveSession::new(StreamEncoderConfig {
                        table_budget_bytes: budget,
                    })
                }),
            );
            let mode = match (budget, flex, asm_export, syncer) {
                (0, ..) => "plain reason=off",
                (_, true, ..) => "local reason=flex",
                (_, _, true, _) => "local reason=asm",
                (_, _, _, true) => "local reason=syncer",
                _ => "stream",
            };
            log_event(
                ctx,
                &format!(
                    "JSON RDB dictionary phase=persistence_start mode={mode} budget_bytes={budget}"
                ),
            );
            return;
        }
        raw::REDISMODULE_SUBEVENT_PERSISTENCE_ENDED => "persistence_ended",
        raw::REDISMODULE_SUBEVENT_PERSISTENCE_FAILED => "persistence_failed",
        _ => return,
    };
    if let Some(session) = SAVE_SESSION.take() {
        // A server that saved no JSON value has nothing to report.
        if session.stream_docs + session.inline_docs > 0 {
            log_event(ctx, &session.summary(phase));
        }
    }
}

unsafe extern "C" fn loading_event(
    ctx: *mut raw::RedisModuleCtx,
    _event: raw::RedisModuleEvent,
    subevent: u64,
    _data: *mut c_void,
) {
    match subevent {
        raw::REDISMODULE_SUBEVENT_LOADING_RDB_START
        | raw::REDISMODULE_SUBEVENT_LOADING_AOF_START
        | raw::REDISMODULE_SUBEVENT_LOADING_REPL_START => {
            // Flex never writes a stream and can keep a record undecoded, so it never reads one.
            let session = (!BIGREDIS_ENABLED.load(Ordering::Relaxed)).then(LoadSession::new);
            LOAD_SESSION.set(session);
        }
        raw::REDISMODULE_SUBEVENT_LOADING_ENDED | raw::REDISMODULE_SUBEVENT_LOADING_FAILED => {
            if let Some(session) = LOAD_SESSION.take() {
                // A load without stream records has nothing to report.
                if session.stream_docs > 0 {
                    let phase = if subevent == raw::REDISMODULE_SUBEVENT_LOADING_ENDED {
                        "loading_ended"
                    } else {
                        "loading_failed"
                    };
                    log_event(ctx, &session.summary(phase));
                }
            }
        }
        // Flex adds intermediate SST/RDB events; they do not end the full load.
        _ => {}
    }
}

#[cfg(test)]
mod module_api_tests;
#[cfg(test)]
mod tests;
