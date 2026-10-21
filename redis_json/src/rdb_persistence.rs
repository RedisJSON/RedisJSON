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
use std::ffi::{c_int, c_void};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use ijson::{
    CborDecodeError, DocUse, IValue, StreamDecoder, StreamEncoder, StreamEncoderConfig,
    StreamEncoderStats, StreamStrings, StringDictionary,
};
use redis_module::logging::{log_io_error, RedisLogLevel};
use redis_module::raw::ModuleOptions;
use redis_module::{raw, Context, Status};

use crate::backward;

// A string that several values share is written to an RDB once, where it is first used, and as a
// small id afterwards; the ids are implicit, in the order of the first uses. The writer keeps
// its table in a session that lives between persistence START and ENDED/FAILED events, and
// the reader rebuilds the same table in a session of its own. A value outside such a stream
// (DUMP, RESTORE, an IO thread) is always self-contained. A value that the core decodes during a
// save only to write it and free it (Flex does so for the values it keeps on flash) may reference
// the strings of the stream but defines none.
// Content dictionaries own their strings, so external managers can define temporary values.

// Existing Flex module option; the upstream redis-module bindings omit fork flags.
const REQUIRE_LOADED_KEYS_IN_RAM: ModuleOptions = ModuleOptions::from_bits_retain(1 << 30);

// A version 5 value record starts with a marker.
// 0: self-contained CBOR.
// 1: a base id (u32 LE),
// then CBOR that defines and references the strings of one RDB stream (ijson's StreamEncoder).
const MARKER_INLINE: u8 = 0;
const MARKER_STREAM: u8 = 1;
// Marker 2 clears the reader dictionary before new definitions; base IDs stay cumulative.
const MARKER_RESET: u8 = 2;
// The encoder appends after these bytes: the marker 1 header, or four unused bytes and marker 0.
// Marker 2 uses the same header as marker 1.
const STREAM_HEADER_LEN: usize = 5;

// Ceiling on the table the writer builds in the saving process; 0 turns the writer off.
// When it is reached the strings defined so far keep being referenced and new ones go inline.
// After a budget miss, the next definition-capable document reuses the cleared table.
pub const DEFAULT_RDB_DICTIONARY_MAX_BYTES: i64 = 256 * 1024 * 1024;
pub const RDB_DICTIONARY_MAX_BYTES_CEILING: i64 = 1 << 40;
pub static RDB_DICTIONARY_MAX_BYTES: AtomicI64 = AtomicI64::new(DEFAULT_RDB_DICTIONARY_MAX_BYTES);
// Bound on the bytes of the strings that a stream defines; 0 sets none. A reader keeps every
// defined string until the end of the stream, even when Flex spills the values that hold them, so
// this is also the most memory that the strings of a stream pin in a reader. Past it a string
// stays inline, and the strings defined so far keep being referenced.
// Dictionary resets release earlier handles; this allowance still counts the entire stream.
pub const DEFAULT_RDB_DICTIONARY_MAX_DEF_BYTES: i64 = 0;
pub static RDB_DICTIONARY_MAX_DEF_BYTES: AtomicI64 =
    AtomicI64::new(DEFAULT_RDB_DICTIONARY_MAX_DEF_BYTES);
// Both save modes use the configured address-table budget.

// Whether the server runs Flex, for the log; set once by init().
static BIGREDIS_ENABLED: AtomicBool = AtomicBool::new(false);

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
    /// A context with its own client/database, used only by the saving thread. GetContextFromIO
    /// has no client, so it cannot be used for SelectDb or IsKeyInRam.
    flex_ctx: Option<*mut raw::RedisModuleCtx>,
    stream_docs: u64,
    inline_docs: u64,
    lookup_only_docs: u64,
}

type EncodeCallback<'a> = dyn FnMut(&RdbStringEncoder<'_>, &mut Vec<u8>) -> Result<(), ()> + 'a;

// Dispatch once per document so the default IValue writer keeps static string lookups.
trait SaveEncoder {
    fn stats(&self) -> StreamEncoderStats;
    fn is_poisoned(&self) -> bool;
    fn borrows_strings(&self) -> bool;
    fn reset_if_needed(&mut self) -> bool;
    fn encode_into(&mut self, out: &mut Vec<u8>, value: &IValue, allow: bool) -> DocUse;
    fn encode_with(
        &mut self,
        out: &mut Vec<u8>,
        allow: bool,
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
    fn borrows_strings(&self) -> bool {
        self.borrows_strings()
    }
    fn reset_if_needed(&mut self) -> bool {
        self.reset_if_needed()
    }
    fn encode_into(&mut self, out: &mut Vec<u8>, value: &IValue, allow: bool) -> DocUse {
        self.encode_into(out, value, allow)
    }
    fn encode_with(
        &mut self,
        out: &mut Vec<u8>,
        allow: bool,
        encode: &mut EncodeCallback<'_>,
    ) -> Result<DocUse, ()> {
        self.encode_with(out, allow, encode)
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
            flex_ctx: None,
            stream_docs: 0,
            inline_docs: 0,
            lookup_only_docs: 0,
        }
    }

    /// Whether this value may add string addresses to the stream's dictionary.
    fn allow_definitions(&self, rdb: *mut raw::RedisModuleIO) -> bool {
        if !self.encoder.borrows_strings() {
            return true;
        }
        let Some(ctx) = self.flex_ctx else {
            return true;
        };
        // SAFETY: the SDK resolves this optional Flex API during module initialization.
        let Some(is_key_in_ram) = (unsafe { raw::RedisModule_IsKeyInRam }) else {
            return false;
        };
        // SAFETY: Redis supplies rdb and its borrowed key. Persistence runs with the main thread
        // blocked or in its fork child, so no extra GIL lock is needed.
        unsafe {
            let key = raw::RedisModule_GetKeyNameFromIO.unwrap()(rdb);
            let dbid = raw::RedisModule_GetDbIdFromIO.unwrap()(rdb);
            !key.is_null()
                && dbid >= 0
                && raw::RedisModule_SelectDb.unwrap()(ctx, dbid) == raw::REDISMODULE_OK as c_int
                && is_key_in_ram(ctx, key.cast_mut()) == 1
        }
    }

    /// Encodes a value of this stream as `bytes[start..]`. The encoder appends after the header
    /// bytes, so a document that uses no string of the stream skips the first four of them and
    /// keeps the marker 0 that fills the last. With `allow_define=false`, a value may reference
    /// existing definitions but adds no string addresses: temporary strings die with the value,
    /// and their addresses can be reused.
    fn encode(&mut self, value: &IValue, allow_define: bool) -> (Vec<u8>, usize) {
        let (base_id, mut bytes) = self.begin_record(allow_define);
        let used = self.encoder.encode_into(&mut bytes, value, allow_define);
        self.finish_record(bytes, base_id, used)
    }

    fn encode_with<E>(
        &mut self,
        allow_define: bool,
        encode: impl FnOnce(&RdbStringEncoder<'_>, &mut Vec<u8>) -> Result<(), E>,
    ) -> Result<(Vec<u8>, usize), E> {
        let (base_id, mut bytes) = self.begin_record(allow_define);
        let mut encode = Some(encode);
        let mut error = None;
        let used = self
            .encoder
            .encode_with(&mut bytes, allow_define, &mut |strings, out| {
                encode.take().expect("one callback per document")(strings, out)
                    .map_err(|e| error = Some(e))
            });
        match used {
            Ok(used) => Ok(self.finish_record(bytes, base_id, used)),
            Err(()) => Err(error.expect("callback error recorded above")),
        }
    }

    fn begin_record(&mut self, allow_define: bool) -> (u32, Vec<u8>) {
        let base_id = self.encoder.stats().defs as u32;
        self.reset_pending |= allow_define && self.encoder.reset_if_needed();
        self.lookup_only_docs += u64::from(!allow_define);
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
            "JSON RDB dictionary phase={phase} defs={} def_bytes={} refs={} stream_docs={} inline_docs={} lookup_only_docs={} poisoned={}",
            stats.defs, stats.def_bytes, stats.refs, self.stream_docs, self.inline_docs,
            self.lookup_only_docs, self.encoder.is_poisoned(),
        )
    }
}

impl Drop for SaveSession {
    fn drop(&mut self) {
        if let Some(ctx) = self.flex_ctx {
            // SAFETY: this session owns the detached context and no callback retains it.
            unsafe { raw::RedisModule_FreeThreadSafeContext.unwrap()(ctx) };
        }
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
    if flex {
        // Decode keys before Flex can copy their RDB bytes directly to SSD.
        ctx.set_module_options(ModuleOptions::HANDLE_IO_ERRORS | REQUIRE_LOADED_KEYS_IN_RAM);
    }
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

/// A value outside any stream: marker 0 and the CBOR of the value.
fn encode_value(value: &IValue) -> Vec<u8> {
    let mut bytes = vec![MARKER_INLINE];
    ijson::encode_into(value, &mut bytes);
    bytes
}

/// Encodes a value as `bytes[start..]`. A keyed value is encoded by the session of the save, when
/// it has one. Saves without a session (the writer is off), keyless values, and poisoned
/// encoders stay self-contained. `allow_define` controls whether this value may add definitions.
fn encode_for_save(value: &IValue, keyed: bool, allow_define: bool) -> (Vec<u8>, usize) {
    keyed
        .then(|| {
            SAVE_SESSION
                .with_borrow_mut(|session| session.as_mut().map(|s| s.encode(value, allow_define)))
        })
        .flatten()
        .unwrap_or_else(|| (encode_value(value), 0))
}

pub(crate) fn save_value(rdb: *mut raw::RedisModuleIO, value: &IValue) {
    // SaveDataTypeToString/Serialize have no key and always remain self-contained.
    // DUMP, MIGRATE and the ASM slot snapshot do carry a key: the command filter ends the session
    // before any command runs, and the ASM child has none.
    // Flex never inserts its temporary save objects into the keyspace.
    let allow_define = SAVE_SESSION.with_borrow(|session| {
        session
            .as_ref()
            .is_none_or(|session| session.allow_definitions(rdb))
    });
    let (bytes, start) = encode_for_save(value, has_key(rdb), allow_define);
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
/// unchanged for the stream; Flex temporary values cannot add such definitions.
/// Unkeyed saves and saves outside a persistence session remain self-contained.
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
    encode: impl FnOnce(&RdbStringEncoder<'_>, &mut Vec<u8>) -> Result<(), E>,
) -> Result<(), E> {
    let keyed = has_key(rdb);
    let (bytes, start) = SAVE_SESSION.with_borrow_mut(|slot| {
        if let Some(session) = slot.as_mut().filter(|_| keyed) {
            let allow_define = session.allow_definitions(rdb);
            session.encode_with(allow_define, encode)
        } else {
            let mut encoder = StreamEncoder::new(StreamEncoderConfig {
                table_budget_bytes: 0,
                max_def_bytes: 0,
            });
            let mut bytes = vec![MARKER_INLINE];
            encoder.encode_with(&mut bytes, false, encode)?;
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

unsafe extern "C" fn persistence_event(
    ctx: *mut raw::RedisModuleCtx,
    _event: raw::RedisModuleEvent,
    subevent: u64,
    _data: *mut c_void,
) {
    // Whatever starts or ends a save, the session of an earlier one must not outlive it.
    let phase = match subevent {
        raw::REDISMODULE_SUBEVENT_PERSISTENCE_RDB_START
        | raw::REDISMODULE_SUBEVENT_PERSISTENCE_AOF_START
        | raw::REDISMODULE_SUBEVENT_PERSISTENCE_SYNC_RDB_START
        | raw::REDISMODULE_SUBEVENT_PERSISTENCE_SYNC_AOF_START => {
            let budget = config_value(&RDB_DICTIONARY_MAX_BYTES);
            let max_def_bytes = config_value(&RDB_DICTIONARY_MAX_DEF_BYTES);
            // A budget of 0 turns the writer off: no session, so every value is self-contained.
            SAVE_SESSION.set((budget > 0).then(|| {
                let mut session = SaveSession::new(StreamEncoderConfig {
                    table_budget_bytes: budget,
                    max_def_bytes,
                });
                if BIGREDIS_ENABLED.load(Ordering::Relaxed) {
                    // SAFETY: ctx belongs to the persistence callback. The detached context is owned
                    // by this save session and is released by Drop.
                    session.flex_ctx = Some(unsafe {
                        raw::RedisModule_GetDetachedThreadSafeContext.unwrap()(ctx)
                    });
                }
                session
            }));
            log_event(ctx, &format!(
                "JSON RDB dictionary phase=persistence_start status=armed flex={} budget_bytes={budget} max_def_bytes={max_def_bytes}",
                BIGREDIS_ENABLED.load(Ordering::Relaxed),
            ));
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
            LOAD_SESSION.set(Some(LoadSession::new()));
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
