# Shared strings in the RDB (RED-217194)

RAM holds one copy of each long string. ijson interns strings, so documents that share a
string hold the same allocation. Without help, an RDB writes every occurrence, so it can be
many times larger than RAM. RedisJSON writes a repeated string once and refers to it afterwards,
in one of two forms.

- A **stream** shares strings across keys. The first save that reaches a shared string writes
  it in full (a definition), and every later occurrence is a 2 to 6 byte id (a reference).
  Ids are implicit and sequential, and the loader rebuilds them by reading the records in
  the order they were written. A record therefore depends on the records before it.
- A **self-contained value** shares strings only inside the one value. A string that occurs
  twice or more in the value is written once, in a record of marker 3, and the reader needs
  no other record, no session and no key.

Both forms use existing Redis module APIs only. The writer owns no string handles, so its
memory grows with the number of distinct shared strings, not with their size, the number of
documents, or the number of occurrences.

## Which form a save uses

| Save | Form |
|---|---|
| SAVE, shutdown, `DEBUG RELOAD`, AOF preamble, BGSAVE on RoR | stream |
| Any save on Flex (`bigredis-enabled`) | self-contained |
| Any save that starts while this node exports slots (atomic slot migration) | self-contained |
| Any save for an Enterprise replica-of syncer, on a core that sets the syncer flag | self-contained |
| DUMP, MIGRATE, the ASM slot snapshot, Flex IO threads, saves without a key | self-contained |
| `json-rdb-dictionary-max-memory-percent` set to 0 | plain CBOR, nothing shared |

`cross_key_stream_allowed` in `rdb_persistence.rs` is the only place that decides. It reads
the budget, whether the server is Flex, whether a slot migration is exporting from this node, and
whether the core flagged the save as one for a syncer, when a save starts. The module counts exports from the `MIGRATE_STARTED`, `MIGRATE_COMPLETED` and
`MIGRATE_FAILED` subevents of `RedisModuleEvent_ClusterSlotMigration`. A fork child inherits the
count, so the snapshot child of a migration never opens a session. A server without atomic slot
migration has no such event, and the module loads without it.

## Replica-of

An Enterprise replica-of destination cannot read a stream. Its syncer converts the source's RDB
into one RESTORE per key, and a RESTORE has no load session, so the destination refuses a stream
record and the sync fails.

The syncer sends `REPLCONF SYNCER 1` to the source, and the core passes
`REDISMODULE_PERSISTENCE_FLAG_SYNCER` in the `RedisModulePersistenceInfo` of the persistence
start event for a save that feeds it. The module then writes self-contained records. The core
keeps such a save apart from a save for ordinary replicas, so the two never share one. An ordinary
Redis replica loads the stream in order with a session and needs nothing.

A core or syncer that predates the flag passes no data, and the module cannot tell a syncer save
from a BGSAVE. A database that is the source of a replica-of on such a version must not write a
stream. Set `json-rdb-dictionary-max-memory-percent` to 0 on it with `CONFIG SET`; the next save writes
plain CBOR. Flex writes no stream and needs no setting.

## How a stream works

```text
save    persistence START       creates the encoder (none when the budget is 0 or on Flex)
        each keyed value        writes marker 1 or 2, or marker 0 (plain)
        table-budget miss       schedules a reset before the next document
        first new definition    writes marker 2 so the reader resets too
        ENDED / FAILED          logs counters and drops the encoder
load    loading START           creates the decoder (none on Flex)
        each marker 1           checks base_id, then decodes using the table
        each marker 2           checks base_id, clears the table, then decodes
        failed stream record    drops the decoder; later stream records are refused
        ENDED / FAILED          logs counters and drops the decoder
```

- The session is thread-local. Redis calls a stream's lifecycle and value callbacks on the same
  thread, so a thread without a session, such as a Flex IO thread, writes self-contained values.
- A string is defined when it lives on the heap (the empty string and strings of up to seven
  bytes sit inside their handle and have no address), so it has at least 8 bytes, and has at
  least two owners. Object keys are strings and take the same path. The encoder recognizes a
  string by the address of its shared allocation and never hashes its bytes. An owner outside
  this RDB can make a string that is used once qualify. It is then defined and never
  referenced, which costs one byte and a table entry.
- Forked saves (BGSAVE, AOF rewrite, diskless and rdb-channel replication) use a frozen
  keyspace, whose documents own the registered strings. Blocking saves (SAVE, SHUTDOWN,
  `DEBUG RELOAD`, module `RdbSave`) keep the keyspace stable until persistence completes.
  Both keep only an address table and use the configured table budget. A running child does
  not disable the writer, because encoding does not change string owners.
- No save writes module AUX records, so an RDB without JSON stays loadable without the
  module, including after defrag has reset the intern cache.
- The loader pins the string of every definition until the next reset, the end or failure of
  the load, or the start of another load. The core decodes every module value in stream order
  before it decides to drop it, so a definition on an expired or slot-trimmed key is still read
  and its string stays pinned until then.
- DUMP, MIGRATE and `SaveDataTypeToString` never join a stream. DUMP, MIGRATE and the ASM slot
  snapshot do carry a key, so two things keep them self-contained. The command filter ends the
  save session before any command runs, and the ASM child has no session. RESTORE and
  `RESTOREMODAUX` cannot arm a load session, so RESTORE of a stream record fails.

## How a self-contained value works

A fresh `StreamEncoder` writes the value and a fresh `StreamDecoder` reads it, so the record
reuses the stream's tags 6 and 7 but carries no base id and shares nothing with any other record.

- A string is defined at its first occurrence in the value when it has at least 8 bytes, a heap
  allocation, and two or more owners. Each later occurrence is a reference (tag 7) to the
  definition's id. Ids belong to the value and start at 0.
- The owner count is process-wide, so a string that other values share but this value holds once
  is defined and never referenced. The writer therefore keeps the record only when it is smaller
  than the plain form: a definition adds 1 byte (tag 6), and the encoder counts the exact bytes
  each reference saves over its text, so the record is kept when those savings exceed the number
  of definitions. Otherwise the writer encodes the value
  again as plain marker 0. A value with no definition at all is already plain, and the writer
  only changes the marker.
- The table lives for one encoding call and is dropped before the call returns. It has no
  ceiling of its own, since it is smaller than the value it encodes. With the writer off
  (`json-rdb-dictionary-max-memory-percent` 0) it defines nothing, so every value is plain.
- `save_with` identifies strings by content with a `ContentDictionary`, and runs its callback a
  second time when it falls back to the plain form.
- The reader creates a fresh `StreamDecoder` per record and drops it after the call. It never
  reads or writes the load session. It refuses a definition under 8 bytes, a reference to an id
  not yet defined, and bytes after the value.

## Other value managers

Managers with different string storage can register a dictionary factory from their module's
`init` callback, after they select and initialize their backend:

```rust
rejson::rdb_persistence::set_dictionary_factory(|| {
    Box::new(ijson::ContentDictionary::default())
});
```

Register on the Redis main thread during module initialization, not in value callbacks. Omitting
the call keeps `ijson::AddressDictionary`. Each stream save creates one fresh dictionary from the
factory and uses it for the whole save. Any `ijson::StringDictionary` implementation is accepted.
All managers in a save share that dictionary, including the built-in `IValue` writer.

Value callbacks call one helper whatever the dictionary:

```rust
rejson::rdb_persistence::save_with(
    rdb,
    |strings, out| ciborium::into_writer(&strings.string(text), out),
)?;
```

- The callback wraps object keys and string values with `strings.string(...)` and writes one
  complete CBOR document directly in wire order. Other fields keep their normal serialization.
  A wrapper registers a definition only when it is serialized.
- The callback must not run Redis commands or call a save helper recursively. It is `FnMut`
  because a self-contained save runs it a second time when the definitions do not pay for
  themselves and the plain form is written. It must serialize the same value each time.
- A serialization error writes no record and stops later references. The manager must propagate
  the error so Redis aborts the save.
- `ContentDictionary` owns copies, so source strings need only live through the callback. It
  needs no intern cache. Its budget includes the copied bytes, the hash table and the
  allocations while the table grows. It admits every string of at least 8 bytes, so unique
  strings also use space. A dictionary that borrows its strings must keep the source storage
  alive and unchanged for the stream.

HDT can read RDBs that this module writes, and this module can read HDT's standard JSON-compatible
RDBs, as long as both implement the complete v5 framing and loading lifecycle. The dictionary policy
does not need to match, because a reader builds its own `IString` table from definitions and
resolves references in order. Native HDT and CRDT persistence is a separate format that this module
does not change, so an Active-Active database never writes a stream for its own persistence.

HDT writes its standard exports through `save_self_contained`. Every consumer of a standard export
reads it one key at a time (backup restore and import run `rl_rdbloader`, which issues one RESTORE
per key), so a stream record there could never be read. `save_self_contained` writes marker 3 when
strings that repeat inside the value pay, otherwise marker 0, and leaves the save's stream alone.

## Wire format

Encoding version 5 keeps its marker byte.

| Marker | Record | Notes |
|---|---|---|
| 0 | `[0][CBOR]` | Plain CBOR with no tag 6 or 7. |
| 1 | `[1][base id: u32 LE][CBOR]` | `base id` is the cumulative number of definitions before this record, across all dictionaries. A document that defines and references nothing is written as marker 0. |
| 2 | `[2][base id: u32 LE][CBOR]` | Checks the same cumulative position, then clears the reader's dictionary. The record must introduce at least one definition. Ids restart at 0. |
| 3 | `[3][CBOR]` | Self-contained. Tags 6 and 7 refer to this record alone, and ids start at 0. This is the form for DUMP, RESTORE, Flex and saves without a session when a string repeats. |
| other | | Refused. |

In the CBOR, a definition is tag 6 around the text and takes the next id. A reference is tag 7
around an unsigned integer smaller than the number of definitions in the current dictionary.
Typed arrays, numbers and the nesting limit follow the same rules as in marker 0, and the legacy
depth limit setting applies to stream records.

```text
{"k": S}, S 40 bytes, first use   01 00000000 A1 61 6B C6 78 28 <S>
{"k": S}, next record             01 01000000 A1 61 6B C7 00
{"k": T}, after a reset           02 01000000 A1 61 6B C6 78 28 <T>
{"k": T}, next record             01 02000000 A1 61 6B C7 00
```

- Cumulative positions detect a missing, repeated or reordered record that carries definitions,
  even when ids are reused. A reset record with no definition is rejected, because it would not
  advance the position. The writer holds its reset marker back across inline documents until a
  document introduces a definition.
- After `u32::MAX` cumulative definitions, new strings stay inline and existing references keep
  working.
- A reference costs 2, 3, 4 or 6 bytes for ids below 24, 256, 65,536 and 2^32. A stream record has
  a 5-byte header against the 1-byte marker of an inline one, and a definition adds one byte for
  the tag. A stream RDB therefore exceeds the inline one by at most 4 bytes per stream record and
  1 byte per definition, before any reference saves anything.
- A value record that the module refuses (its loader returns NULL) stops startup, AOF loading or
  replica loading. The normal RDB checksum protects the file when it is enabled. The module has no
  footer, so it does not detect a removed final record unless a later base id check exposes it.
- A reader that does not know a marker refuses the record. Upgrade readers
  before writers. Modules that predate encoding version 5 return NULL for it, which stops the
  server.

## Session lifecycle

| Event | Save session | Load session |
|---|---|---|
| persistence START (RDB or AOF, sync or child) | old encoder dropped; new encoder created, none when the budget is 0 or on Flex | |
| eligible definition misses the table budget | table cleared before the next document; allocation reused | |
| persistence ENDED or FAILED | counters logged; encoder dropped | |
| command filter | encoder dropped | |
| loading START (RDB, AOF, replication) | | old decoder dropped; new decoder created, none on Flex |
| stream record | | base id must equal the cumulative number of definitions read |
| marker 2 | | old dictionary handles released before decoding new definitions |
| failed stream record or IO failure | | decoder dropped; later stream records are refused |
| loading ENDED or FAILED | | counters logged; decoder dropped |

Flex intermediate loading events do not end the full load. Active defrag skips its intern-cache
reset while a decoder owns strings.

The call paths are those of OSS Redis `52a92ccd`. `rdbSave` brackets SAVE, BGSAVE and DEBUG
RELOAD. `rewriteAppendOnlyFile` brackets AOF preambles. `rdbSaveRioWithEOFMark` brackets diskless
replication. `rdbSaveToFile` brackets module `RdbSave`. Loading START precedes the value
callbacks, and ENDED or FAILED follows them. Module serialization without these events, IO-thread
work, DUMP, MIGRATE and the ASM slot snapshot write self-contained values. The command filter ends
the save session before any command can create a standalone payload, including a command that
another module invokes.

## Safety invariants

- **Address reuse is the worst failure.** A reference that resolves to the wrong text gives no
  error. In a stream, the child's keyspace is frozen and blocking saves keep resident documents
  alive until persistence completes. Each table slot keeps the string's length, and a mismatch
  at a reference poisons the session, so the rest of the save is inline. The length check is
  telemetry, not proof: an address reused by a string of the same length goes unnoticed. The
  caller's ownership prevents that reuse, so nothing in a save may free or move a registered
  allocation while a stream is being written.
- **Flex never writes a stream.** Flex frees values that it decodes from flash while it saves, so
  their addresses can be reused. A self-contained value is safe because its table lives only for
  the call that encodes it, while the value is borrowed.
- **A self-contained value cannot touch a stream.** Its ids are local to the value, it carries no
  base id, and decoding it neither reads nor writes the load session. A stream record that
  reaches a reader with no session (RESTORE, Flex, a replica converter) is refused as
  `no_stream`. It is never resolved against another table.
- **Writes are transactional.** A stale hit poisons the encoder, so the rest of the save is
  inline. A failed write fails the whole save, so a registered id never outlives a document that
  was not written.
- **Reads are in lockstep.** A stream record is read only when its base id equals the number of
  definitions read. A skipped, duplicated or reordered record that carries definitions fails
  loudly instead of resolving to the wrong string.
- **No `RefCell` or lock is held across a module call**, because loading pumps the event loop.
- **A failed load leaves nothing pinned.** A failed stream record or IO read releases the session,
  and so does the loading ENDED or FAILED event.
- Log lines contain no percent character, because the log helper can pass a line to the server as
  a printf format.

## Configuration

| Setting | Default | Meaning |
|---|---|---|
| `json-rdb-dictionary-max-memory-percent` | 20 | Ceiling on the heap that a stream's address table uses, as a percentage of `used_memory` when the save starts. The old and the new allocation both count while the table grows. When an eligible definition misses this budget, the rest of that document keeps its new strings inline, and the table is cleared before the next document and reused. 0 turns string sharing off, across keys and inside a value. The reader always accepts. Range 0 to 100. |

The setting changes at runtime with `CONFIG SET`. Each save reads it, and `used_memory`, at
persistence START; for a BGSAVE that is in the child, which sees the memory of the fork. The
extra memory a save needs at its peak is therefore at most this share of the dataset. A module
argument skips the range check of `CONFIG SET`, so a value outside the range counts as its nearest
bound, and a negative value turns sharing off.

When an eligible definition does not fit the table budget, existing entries keep being referenced
for the rest of that document and new strings stay inline. Before the next document, the writer
clears the table and keeps its allocation, so new definitions can fill it again. A full table that
only has hits is left alone, unshared strings do not schedule resets, and a budget too small for
one allocation stays inline without futile resets.

The reader releases old dictionary handles at the reset marker, and values that are already
loaded keep their own handles. This bounds the retained dictionary entries by the largest window.
It does not cap string payload or process memory, because one window can still define large
strings. There is no reset inside a single document, so a value larger than the table writes the
rest of its strings inline after the table fills.

A reset trades old references for the chance to define later strings. It helps when later
documents repeat different strings. It hurts when old, frequently used strings must be defined
again. Redis hash iteration is not a locality guarantee.

## Logs

Children have no INFO channel, so each save and load logs lines that start with
`JSON RDB dictionary phase=`.

| Line | Fields |
|---|---|
| `persistence_start` | `mode=stream`, `mode=local reason=budget`, `mode=local reason=flex`, `mode=local reason=asm`, `mode=local reason=syncer` or `mode=plain reason=off`, then `percent` and `budget_bytes`. `reason=budget` means the share of the memory left no room for a table. |
| `persistence_ended` / `persistence_failed` | `defs def_bytes refs resets stream_docs inline_docs poisoned`. A stream save that wrote a JSON value emits it. `resets` counts how often the table filled up and was cleared. Above 0 means the save ran into `json-rdb-dictionary-max-memory-percent`, so raise it to share more. |
| `loading_ended` / `loading_failed` | `defs def_bytes refs stream_docs`, when the load read a stream record |
| `load` | `status=refused reason=...` with `marker_N`, `no_stream`, `base_id_mismatch` or `cbor_...`. The level is warning while loading. Outside a load, `no_stream` adds `refusals=N` and is a warning when N is a power of two, otherwise verbose. |

Persistence summaries follow the core's completion or failure event. A refused record while a file
loads stops the server. The exit status can be 0, because the RDB checker that Redis runs skips
module values, so the warning line is the only sign of the cause. `poisoned=true` means the
encoder stopped writing definitions and references. `rdb_last_cow_size` includes the table and is
not dataset copy-on-write.

## Limits

- **Memory.** A single pass that shares strings across keys must remember each repeated string
  between its first and last use, and Redis saves in hash order. The table costs about 22 to 36
  bytes per distinct shared string, so the default ceiling of 1 GiB holds about 30 to 49 million
  of them. The loader keeps 8 bytes per definition in the current
  dictionary. Resets keep the table's capacity and release the reader's old string handles.
- **Small strings.** Strings from 8 bytes up are shared, object keys included. A reference replaces
  only 9 to 64 bytes with 2 to 6, and every distinct string costs a table entry. A tiny document
  whose only shared string is held by two documents gets larger when that string is under 12
  bytes, because of the 5-byte header and the tag byte. Loading is slower per reference than
  reading inline text, so a document with many short shared strings loads more slowly than one
  without sharing. Compression with `rdbcompression yes` shrinks the gain, because LZF already
  shrinks repetition inside a string.
- **Order.** A stream's records depend on the records before them. They cannot be decoded alone,
  sliced out by key, or reordered. A stream RDB is read only through Redis loading with this
  module.
- **Versions.** Encoding version 5 is unreadable by modules that predate it, even when sharing
  is off.
- **Flex.** Every Flex value is self-contained, so a Flex save shares strings only inside one value.
  Flex can keep a value on flash undecoded and decode it later, alone, with no record before it, so
  a stream record cannot be resolved there. The module never writes a stream on Flex, never arms a
  load session there (a stream record that reaches Flex fails as `no_stream`), and does not set
  `REQUIRE_LOADED_KEYS_IN_RAM`, which would disable direct-to-disk loading for every key of every
  type. An RDB that a RoR server wrote as a stream is not loadable on Flex.
- **Expired keys.** A stream with many expired keys holds their strings until a reset or the end of
  the stream, because the core decodes every value before it can drop it.

## Open questions before the writer is on by default

Implementation does not wait for these. Turning the writer on by default does.

- **G1.** Do Enterprise replica-of, Active-Active, ASM scale-out, and import or backup-restore
  consume module values through RESTORE conversion (`rdb.c`, `rl_syncer`, `rl_rdbloader`)? A
  stream record cannot decode there, and the module cannot tell its consumer. If any of them
  does, the options are a per-database opt-out and a fresh self-contained save before every
  export, or the syncer signal described under Replica-of. Strings of 8 bytes and up are shared, object
  keys included, so a stream is written for nearly every dataset that repeats a key.
- **G2.** The production `rdbcompression` and `bigredis-compression` values on Flex. The default
  pair takes the path that re-serializes values.
- **G3.** The real number of distinct shared strings, their sizes, and their multiplicity in the
  target datasets. A customer on RedisJSON 8.6.5 stores JSON text (encoding version 3), so
  delivering there needs a CBOR release.
- Roll out readers first, then writers.

## Source and tests

| Repository | Files |
|---|---|
| RedisJSON | `redis_json/src/rdb_persistence.rs`: sessions, framing, events, command filter, and the choice between stream and self-contained. `rdb_persistence/tests.rs` and `module_api_tests.rs`: unit tests, the second against a fake module API. `redisjson.rs`: version dispatch. `lib.rs`: configs. `defrag.rs`. |
| ijson | `src/cbor/stream.rs`: `StreamEncoder`, `StreamDecoder`, tags 6 and 7. `src/cbor.rs`: serializer and decoder hooks. `src/string.rs`, `unsafe_string.rs`: `heap_identity`. `README.md`. |
| Flow tests | `tests/pytest/test_rdb_dictionary.py` |

```sh
# ijson: serial, because the intern cache is global
cargo test -- --test-threads=1
cargo test --features thread_safe -- --test-threads=1
# RedisJSON
cargo test -p redis_json
cargo test -p redis_json --features as-library
# Flow tests need a built module and redis-server. With TEST=, tests.sh adds -s, which RLTest
# refuses together with --parallelism, hence LOG=1 PARALLEL=0. Leave QUICK unset so that the
# replica and AOF modes run as well. The file skips on cluster, hence CLUSTER=0.
MODULE=/path/to/librejson.so CLUSTER=0 LOG=1 PARALLEL=0 TEST=test_rdb_dictionary.py ./tests/pytest/tests.sh
```

ijson has a fuzz target for the decoder, `fuzz_cbor_stream_decode`. `Cargo.toml` points at the
sibling `../ijson` checkout. A published ijson revision that has the stream API has to replace it
before the change is distributed.

The flow tests cover SAVE, BGSAVE, `DEBUG RELOAD`, several databases, DUMP and RESTORE before,
during and after a save (a DUMP of a value that repeats a string holds it once), RESTORE of a
stream record, a damaged stream, a BGSAVE under concurrent writes, an expired key that holds the
only definition, the AOF preamble, full replication, the 8-byte minimum for strings and keys, and
sharing turned off while streams still load. Exact RDB sizes depend on hash order, so the tests
assert sizes as ratios.
