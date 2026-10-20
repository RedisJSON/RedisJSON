# Shared strings in the RDB: first-use stream (RED-217194)

RAM holds one copy of each long string: ijson interns strings, and documents that
share one hold the same allocation. The RDB used to write every occurrence, so it
could be many times larger than RAM. The module now writes a shared string inline
once per dictionary, where a save reaches it first (a definition), and later occurrences as
small ids (references). A full table can be cleared and reused between documents.
Ids are implicit and sequential within each dictionary; the loader rebuilds them by
reading the records in the order they were written. The writer keeps no snapshot,
owns no string handles and writes no owner count in either save mode. Its memory grows with the
number of distinct shared strings, not with their size, the number of documents or the
number of occurrences. Only existing Redis module APIs are used.

This replaces the earlier snapshot dictionary, which pinned the repeated strings of
the intern cache before the first key and capped them at 65,536 entries.

## Flow

```text
save    persistence START       creates the encoder (none when the budget is 0)
        each keyed value        writes marker 1, 2 or 0
        each Flex value         queries residency; only resident values may define strings
        table-budget miss       schedules a reset before the next resident document
        first new definition    writes marker 2 so the reader resets too
        ENDED / FAILED          logs counters, drops encoder and residency context
load    loading START           creates the decoder
        each marker 1           checks base_id (0 for the first), then decodes using the table
        each marker 2           checks cumulative base_id, clears the table, then decodes
        failed stream record    drops the decoder; later stream records are refused
        ENDED / FAILED          logs counters and drops the decoder
```

- The session is thread-local: Redis calls a stream's lifecycle and value callbacks on the
  same thread. Flex IO threads therefore have no session and write self-contained
  records.
- A string is defined when it lives on the heap (the empty string and, by default,
  strings of up to seven bytes sit inside their handle and have no address), so it has
  at least 8 bytes, and has at least two owners. Object keys are strings too and take the
  same path. A string is recognised by the address of its shared allocation, not by
  hashing its bytes. An owner outside this RDB can make a string that is used once
  qualify; it is then defined and never referenced, which costs one byte and a table
  entry.
- Both save paths use the same encoder configuration:
  - Forked saves (BGSAVE, AOF rewrite, diskless and rdb-channel replication) use a
    frozen keyspace, whose documents own registered strings.
  - Blocking saves (SAVE, SHUTDOWN, `DEBUG RELOAD`, module `RdbSave`):
    serialization keeps the keyspace stable until persistence completion; the documents
    own registered strings throughout that interval.
  - Both paths keep only an address table and use the configured table budget. An
    existing child does not disable the writer: encoding does not update string owners.
  - A budget of 0 creates no encoder; every value is self-contained. No save writes
    module AUX records, so an RDB without JSON stays loadable without the module,
    including after defrag has reset the intern cache.
- DUMP, MIGRATE, `SaveDataTypeToString` and any save without a session write marker 0
  only. DUMP, MIGRATE and the ASM slot snapshot do carry a key, so what keeps them
  self-contained is the command filter, which ends the session before any command runs,
  and the absence of a session in the ASM child. RESTORE and `RESTOREMODAUX` cannot arm
  a load session, so RESTORE of a stream record is an error.
- The loader pins the string of every definition until the next reset, end or failure
  of the load, or the start of another load. The core decodes every module value in
  stream order before it decides to drop it, so a definition on an expired or
  slot-trimmed key is still read, and its string stays pinned until then.

## Other value managers

The address-only behavior above remains the OSS default. Managers with different
string storage select an optional factory in `redis_json_module_create!`:

```rust
rdb_dictionary: Some(|| Box::new(ijson::ContentDictionary::default())),
```

Omitting it, or supplying `None`, uses the address dictionary. The selected factory
creates one fresh dictionary at each save's start. It cannot change during the
save. Value callbacks use the same helper regardless of dictionary implementation:

```rust
rejson::rdb_persistence::save_with(
    rdb,
    |strings, out| ciborium::into_writer(&strings.string(text), out),
)?;
```

Any `ijson::StringDictionary` implementation is accepted; RedisJSON does not maintain
a list of supported implementations. All managers share that save's dictionary,
including the built-in `IValue` writer. Dictionary inputs carry text and, for
interned `IString` values, an optional allocation identity and owner count.

The serializer wraps participating object keys and string values with
`strings.string(...)`, writing one complete CBOR document directly in wire order.
Other fields keep their normal serialization. Wrappers register definitions only
when serialized. Serialization errors write no record and disable subsequent
references; the manager must propagate the error so Redis aborts the save. The
callback must not run Redis commands or recursively invoke a save helper.

`ContentDictionary` owns copies, including strings from temporary Flex values.
It needs no intern cache and no residency query. Its writer budget includes copied
bytes, hash-table storage and simultaneous growth allocations. Unlike the default
address policy, it admits every string of at least 8 bytes, so unique strings also
consume space. Custom borrowing dictionaries return `true` from `borrows_strings()`,
must preserve source storage throughout the stream, and receive the same Flex
residency guard as OSS.

RedisJSON still owns lifecycle, markers, cumulative positions and limits.
Budget resets work as before; there are no dictionary-type switches or type-ID
checks. The factory returns a boxed dictionary, accessed through its trait.
The default OSS encoder keeps static string lookups; encoder dispatch happens at
the document boundary.
Unkeyed saves and saves outside a session remain marker 0.

All policies produce the existing v5 format. Assuming both modules implement
the complete v5 framing and loading lifecycle, OSS JSON RDBs can be loaded by HDT
and HDT standard JSON-compatible RDBs can be loaded by OSS JSON. Dictionary policy
does not need to match: the reader still builds its own
`IString` table from DEF records and resolves REF records in order. A future HDT
adapter must use this helper for standard JSON-compatible exports and preserve
the shared loading lifecycle for imports. Native HDT/CRDT persistence is separate;
this infrastructure does not change that format or implement the HDT adapter.

## Wire format

Encoding version 5 keeps its marker byte.

| Marker | Record | Notes |
|---|---|---|
| 0 | `[0][CBOR]` | Self-contained: the only form for DUMP, RESTORE, Flex IO threads and sessionless saves. |
| 1 | `[1][base id: u32 LE][CBOR]` | `base id` is the cumulative number of definitions before this record, including previous dictionaries. A document that defines and references nothing is written as marker 0. |
| 2 | `[2][base id: u32 LE][CBOR]` | Checks the same cumulative position, then clears the reader dictionary. This record must introduce at least one definition. Dictionary IDs restart at 0. |
| other | | Refused. |

In the CBOR, a definition is tag 6 around the text and takes the next id; a reference
is tag 7 around an unsigned integer smaller than the number of definitions in the current dictionary.
Typed arrays, numbers and the nesting limit are unchanged, and the legacy depth limit
setting applies to stream records as to inline ones.

```text
{"k": S}, S 40 bytes, first use   01 00000000 A1 61 6B C6 78 28 <S>
{"k": S}, next record             01 01000000 A1 61 6B C7 00
{"k": T}, after a reset           02 01000000 A1 61 6B C6 78 28 <T>
{"k": T}, next record             01 02000000 A1 61 6B C7 00
```

Reset headers stay five bytes. Cumulative positions detect a missing, repeated or
reordered definition-bearing record even when dictionary IDs are reused. A reset with
no definition is rejected: it would not advance that position. The writer defers its
reset marker across inline documents until it can introduce a definition.
After `u32::MAX` cumulative definitions, new strings stay inline; existing references
remain usable.

A reference costs 2, 3, 4 or 6 bytes for ids below 24, 256, 65,536 and 2^32. A stream
record has a 5-byte header against the 1-byte marker of an inline one and a definition
adds one byte (the tag), so the RDB exceeds the inline one by at most 4 bytes per stream
record and 1 byte per definition, before any reference saves anything.

There are no module AUX records, format capability records, or module footer counters.
Existing first-use files without resets remain readable because their cumulative
positions equal their dictionary lengths. Earlier first-use builds reject marker 2;
upgrade readers before writers. Version 5 remains experimental. Existing experimental
AUX-format RDBs must be regenerated; no compatibility reader is provided.
A value record the module refuses (NULL from its loader) stops startup, AOF,
or replica loading. Base-ID and reference validation remain; the normal RDB checksum
protects the file when enabled. Without a footer, removing a final record is not detected
by the module when no later base-ID check exposes it.

## Session lifecycle

| Event | Save session | Load session |
|---|---|---|
| persistence START (RDB or AOF, sync or child) | old encoder dropped; new encoder created, none when the budget is 0 | |
| temporary Flex value saved | existing references allowed; no new definition from this value | |
| eligible definition misses the table budget | table cleared before the next resident document; allocation reused | |
| persistence ENDED or FAILED | counters logged; encoder dropped | |
| command filter | encoder dropped | |
| loading START (RDB, AOF, replication) | | old decoder dropped; new decoder created |
| stream record | | base ID must equal the cumulative number of definitions read |
| marker 2 | | old dictionary handles released before decoding new definitions |
| failed stream record or IO failure | | decoder dropped; later stream records are refused |
| loading ENDED or FAILED | | counters logged; decoder dropped |

Flex intermediate loading events do not end the full load. Active defrag skips its
intern-cache reset while a decoder owns strings.

Lifecycle call paths were inspected in OSS Redis `52a92ccd` and Flex `40db934`:
`rdbSave` brackets SAVE/BGSAVE and DEBUG RELOAD; `rewriteAppendOnlyFile` brackets AOF
preambles; `rdbSaveRioWithEOFMark` brackets diskless replication; `rdbSaveToFile` brackets
module `RdbSave`. Loading START precedes value callbacks, and ENDED/FAILED follows them.
Module serialization without these lifecycle events, IO-thread work, DUMP/MIGRATE, and
ASM slot snapshots remain self-contained. The command filter disarms sharing before
commands can create standalone payloads, including commands invoked by other modules.

## Safety invariants

- **Address reuse is the worst failure:** a reference that resolves to the wrong text
  without any error. Guards: the child's keyspace is frozen; blocking `sync` saves
  keep resident documents alive until persistence completion; each table slot keeps the string
  length, and a mismatch at a reference poisons the session (the rest of the save is inline) and counts a
  `stale_hit`. The length check is telemetry, not proof: an address reused by a string of
  the same length goes unnoticed in either mode. The caller's ownership prevents
  that reuse, so nothing in a save may free or move a registered allocation while
  a stream is being written.
  The Flex flash pass does free values: for a value that is not in memory the core
  decodes the record (`rdb_load`), saves the value (`rdb_save`) and frees it, one at a
  time on the thread of the save. Each `rdb_save` queries `RedisModule_IsKeyInRam`
  using the key and database from its IO handle. Temporary decoded objects never enter
  the keyspace, so this returns false even while the callback holds their decoded value.
  Metadata-only keys also return false. These values use `allow_define=false`: existing
  references remain usable, but no new address is registered. Later resident values may
  still define strings.
  The session owns a detached context with its own client for database selection;
  `GetContextFromIO` alone has no client. Persistence end, failure, the command filter
  or a fresh START releases that context. A missing residency API, unknown database,
  or failed database selection disables definitions for that value. Without
  the guard, on Flex, a BGSAVE of 300 values on flash that repeat one string of one length
  wrote 598 references to two definitions, and 298 of the 299 values read back with another
  string, without poisoning the encoder or reporting an error. With it, the same save defines
  nothing for them and every value reads back exactly (the Flex test for strings of one length). A test
  that makes the pages of the strings read-only in a forked child checks that encoding
  writes to none of them.
  Both save modes rely on the core decoding, saving and freeing temporary values on
  the saving thread, one at a time, without freeing or moving resident strings that
  supplied definitions. The Flex I/O drain precedes the first definition. These paths
  were audited on core `40db934`, and 14 focused resident/Flex tests passed with no
  writer-owned handles. The generic codec requires callers to preserve the same
  allocation-lifetime guarantee; arbitrary yielding or keyspace-mutating callbacks
  are outside this save protocol.
- **Transactional:** a stale hit poisons the encoder, so the rest of the save is
  inline, and a failed write fails the whole save, so a registered id never outlives a
  document that was not written.
- **Lockstep:** a stream record is read only when its base id equals the number of
  definitions read. A skipped, duplicated or reordered record that carries definitions
  fails loudly instead of resolving to the wrong string. A missing final record is left
  to the core RDB checksum; module footer validation has been removed.
- **No RefCell or lock is held across a module call:** loading pumps the event loop.
- **Failed loads leave nothing pinned:** a failed stream record or IO read releases the
  session, and so does the loading ENDED or FAILED event.
- Log lines contain no percent character; the log helper can pass a line to the server
  as a printf format.

## Configuration

The [reset experiment and comparison](rdb-dictionary/reset-results.md) documents
table reuse, memory behavior, validation and measured compression tradeoffs.

| Setting | Default | Meaning |
|---|---|---|
| `json-rdb-dictionary-max-bytes` | 256 MiB | Ceiling on the writer's address-table heap in either save mode. The old and the new allocation count while the table grows. After an eligible definition misses this budget, the table is cleared and reused between resident documents. 0 turns the writer off; the reader always accepts. Range 0 to 2^40. |
| `json-rdb-dictionary-max-def-bytes` | 0 | Maximum cumulative UTF-8 definition bytes across the whole RDB; 0 sets no bound. Resets do not replenish it, and redefining a string counts its bytes again. Past it a string stays inline and existing definitions remain usable. It bounds definition payload retained by the reader, not total reader RAM. Range 0 to 2^40; a negative module argument counts as 0. |

Both change at runtime with `CONFIG SET` (units such as `128mb` work); a BGSAVE child
keeps the value it inherited at fork. A module argument skips the range check of
`CONFIG SET`, so a value outside the range counts as its nearest bound, and a negative
budget switches the writer off. The names are those of the earlier design.

When an otherwise eligible definition does not fit the table budget, existing entries
keep being referenced for the rest of that document and new strings stay inline.
Before the next resident document, the writer clears the table while retaining its
allocation. New definitions can then fill it again. A full table with only hits is
left alone. Cold values, unshared strings and definition-payload limit misses do not
schedule resets. A budget too small for one allocation stays inline without futile resets.

The reader releases old dictionary handles at the reset marker; already loaded values
keep their own handles. This bounds retained dictionary entries by the largest window,
but does not cap string payload or process memory. One window can still define large strings.
There is no reset inside a single document, so an oversized value still writes its
remainder inline after the table fills.

Resets trade old references for opportunities to define later strings. They help when
later documents repeat different strings; they can hurt when old, frequently used
strings must be defined again. Redis hash iteration is not a locality guarantee.

## Logs

Children have no INFO channel, so each save and load logs lines that start with
`JSON RDB dictionary phase=`:

| Line | Fields |
|---|---|
| `persistence_start` | `status=armed flex budget_bytes max_def_bytes` |
| `persistence_ended` / `persistence_failed` | `defs def_bytes refs stream_docs inline_docs lookup_only_docs poisoned`; emitted if the save wrote a JSON value. `lookup_only_docs` counts values whose residency did not permit definitions; later resident values do not contribute to this count. |
| `loading_ended` / `loading_failed` | `defs def_bytes refs stream_docs`, when the load read a stream record |
| `load` | `status=refused reason=...` with `marker_N`, `no_stream`, `base_id_mismatch` or `cbor_...`; warning level while loading |

Persistence summaries follow the core completion or failure event. A refused record while a file loads stops the server,
and its exit status can be 0 because the RDB checker it runs skips module values; the
warning line is the only sign of the cause. `poisoned=true` means the encoder stopped writing
definitions and references. `rdb_last_cow_size` now includes the table and is no longer
dataset copy-on-write.

## Limits

- Not memory-free: a single pass that dedups across keys must remember each repeated
  string between its first and last use, and Redis saves in hash order. The table costs
  about 22 to 36 bytes per distinct shared string, and the loader keeps 8 bytes per
  definition in the current dictionary. Resets retain table capacity and release its
  previous string handles on the reader; their compression benefit depends on reuse.
- Strings from 8 bytes up are shared, object keys included, but a reference replaces only
  9 to 64 bytes with 2 to 6, and every distinct string costs a table entry: 2 million
  distinct strings of 8 to 30 bytes, each held by two documents, need a 71 MB table, and
  the pre-reset implementation froze it near 1.8 million entries in the server process.
  The following measurements predate dictionary resets. Measured against a
  minimum of 64 on 100,000 documents (SAVE in the server process, `rdbcompression no`):
  the RDB is 52 to 72 percent smaller where keys, enumerated values or ids repeat, and SAVE
  is 26 to 47 percent faster for it, because the file is smaller. Loading is slower: 3 to
  17 percent for those, about 25 to 30 percent for large documents and for strings that
  only two documents share, and 57 to 135 percent for arrays of 8-byte strings, about
  60 ns per reference. A tiny document whose only shared string is held by two documents
  gets larger below 12 bytes (15 percent in stream bytes at 8 bytes, 7 percent at 10). With
  the default `rdbcompression yes` the gain shrinks: documents of 40 items are 40 percent
  smaller than with 64, 79 percent without LZF. The encoder and decoder alone, run at 8, 10,
  12, 16, 24, 32 and 64, show the size gain and the read cost falling together as the
  minimum rises; no length keeps one and drops the other. Flex was not measured.
- Records of a stream depend on the records before them, so they cannot be decoded
  alone, sliced out by key or reordered. A stream RDB is read only through Redis loading
  with this module.
- Encoding version 5, even with the writer off, is unreadable by modules that predate
  it: they return NULL for an unknown version, which stops the server. Replicas and
  importers on older modules cannot load these streams. Experimental AUX-format files
  also need regeneration. Staying on version 4 and marking stream records
  with a root CBOR tag would keep inline records readable by them; the framing is in one
  function pair (`encode_for_save`, `decode_for_load`), so that choice stays reversible.
- Flex: the writer deduplicates resident values throughout the save, using one keyspace
  lookup per module save callback. A value that Flex keeps on flash holds its own copy of
  every string, so the writer only lets it reference strings that values in RAM
  defined; strings that are shared among flash values alone are written in full, and a
  database with most of its values on flash shrinks little. That is the difference
  between RAM and its RDB that the ticket describes, no more: flash stores each value in
  full as well. A save could define the strings of cold values and pin them until it ends,
  and later cold values would then match them by address, but that spends RAM that only
  `json-rdb-dictionary-max-def-bytes` bounds, and the module does not do it. The reader
  accepts streams on Flex with no configured reader memory bound: every defined string
  stays resident until its dictionary resets or loading ends, even when Flex spills its keys,
  so a dictionary with more
  distinct shared strings than a Flex
  node holds in RAM can exhaust it; a stream that Flex wrote holds the strings of its RAM
  values only. Measured on a stream that defines 48 MiB of strings, with the RAM limit in
  bytes (`bigredis-max-ram`): a limit of 192 MiB or 72 MiB loads it, and a limit of 24 MiB
  stops the server during the load, because the pinned strings are RAM that spilling
  values does not free. `json-rdb-dictionary-max-def-bytes` bounds what a stream pins; it
  is 0, no bound, by default on RoR and on Flex alike, and a Flex deployment that loads
  streams under a small RAM limit sets it to a fraction of that limit. Flex would copy a record's RDB bytes to SSD without decoding it, and a
  stream record cannot be resolved later, so on Flex the module sets
  `REQUIRE_LOADED_KEYS_IN_RAM` at init. That affects all RDB loads and RESTOREs.
  Replicas that ask for an uncompressed RDB (diskless load) copy the bytes of values on
  flash without decoding them, so such a transfer carries them in full.
- A stream with many expired keys holds their strings until a reset or its end, as the core decodes
  every value before it can drop it.

## Rollout gates

Implementation does not wait for these; turning the writer on by default does.

- **G1.** Do Enterprise replica-of, Active-Active, ASM scale-out and import or
  backup-restore consume module values through RESTORE conversion (`rdb.c`,
  `rl_syncer`, `rl_rdbloader`)? A stream record cannot decode there, and the module
  cannot tell its consumer. If any does: a per-database opt-out and a fresh inline save
  before every export. This decides the release default. Strings of 8 bytes and up are
  shared, object keys included, so a stream is written for nearly every dataset that
  repeats a key, not only for one that holds long shared strings.
- **G2.** The minimum Flex core that carries `REQUIRE_LOADED_KEYS_IN_RAM`, and the
  production `rdbcompression` and `bigredis-compression` values (the default pair is the
  re-serialising path, which the earlier evidence never exercised).
- **G3.** The ticket customer runs RoR: its replica had `rdbcompression yes`, the RoR
  default, where Flex defaults to no. Its real number of distinct shared strings, their
  sizes and multiplicity are still modelled. A customer on 8.6.5 stores JSON text
  (encoding version 3), so delivering there needs a CBOR release or a separate design.
- Rollout order: readers first, then writers.

### Future core improvement

Expose whether the current save value is temporary directly on `RedisModuleIO`, with an
explicit allocation-lifetime guarantee for resident values until the save ends. Core
already distinguishes RAM and disk-origin values. Such an API would remove the extra
residency lookup and detached context; the current implementation needs no core change.

The SDK and macros are pinned together to `cca53606a1b8bfaa78f96399cb10bb9d3a1f1099`,
whose generated bindings expose the optional `RedisModule_IsKeyInRam` function. This is
the SDK master line (package version 2.0.8), not a newer 2.1 release tag.

## Source and verification

| Repository | Files |
|---|---|
| RedisJSON | `redis_json/src/rdb_persistence.rs`: sessions, framing, events, command filter, Flex option; `rdb_persistence/tests.rs` and `module_api_tests.rs`: unit tests, the latter against a fake module API; `redisjson.rs`: version dispatch; `lib.rs`: configs; `defrag.rs` |
| ijson | `src/cbor/stream.rs`: `StreamEncoder`, `StreamDecoder`, tags 6 and 7; `src/cbor.rs`: serializer and decoder hooks; `src/string.rs`, `unsafe_string.rs`: `heap_identity`; `README.md` |
| Flow tests | `tests/pytest/test_rdb_dictionary.py` |

```sh
# ijson: serial, because the intern cache is global
cargo test -- --test-threads=1
cargo test --features thread_safe -- --test-threads=1
# RedisJSON
cargo test -p redis_json
cargo test -p redis_json --features as-library
# flow tests need a built module and redis-server. With TEST=, tests.sh adds -s, which RLTest
# refuses together with --parallelism, hence LOG=1 PARALLEL=0. Leave QUICK unset so that the
# replica and AOF modes run as well; the file skips on cluster, hence CLUSTER=0.
MODULE=/path/to/librejson.so CLUSTER=0 LOG=1 PARALLEL=0 TEST=test_rdb_dictionary.py ./tests/pytest/tests.sh
```

ijson has a fuzz target for the decoder, `fuzz_cbor_stream_decode`. `Cargo.toml` points
at the sibling `../ijson` checkout; a published ijson revision with the stream API has
to replace it before the change is distributed.

The flow tests cover SAVE, BGSAVE, `DEBUG RELOAD`, several databases, DUMP and RESTORE
before, during and after a save, RESTORE of a stream record, a damaged stream, a BGSAVE
under concurrent writes, an expired key holding the only definition, the AOF preamble,
full replication, the 8 byte minimum for strings and keys, and the writer off while streams
still load. On Flex most of them run
again with the values kept in RAM (the Flex harness evicts them after each command), and
seven are Flex only: values on flash that define no string and reference the ones in
RAM, strings of one length in values on flash that must not be mixed up, a stream
that is loaded while every value spills, a stream loaded under a RAM limit in bytes,
full replication with values on flash, and two that spill the values of a load and
write them out again. They skip on RAM-only Redis. Exact RDB sizes depend on hash order, so sizes are
asserted as ratios.

Status: the Rust tests above pass. The flow tests pass on macOS with RAM-only Redis, in
the general, replica and AOF modes (the Flex tests skip there), and on Flex, in a Linux
ARM64 container with Flex core `40db934`, in the same three modes, with no anomaly in the
module's logs. A comparison on macOS (Redis 8.8, small corpora) against the earlier
snapshot build wrote RDBs that every build read back exactly: for 100,000 distinct 1 KiB
strings the RDB is 0.26 of its size without sharing, where the snapshot dictionary gave
0.81. A probe module that drives a real redis-server 8.9 from its own load (no socket
needed) ran SAVE, BGSAVE, BGSAVE under concurrent writes, `DEBUG RELOAD`, DUMP and
RESTORE, several databases, expired keys, a save that fails midway and a save beside an
active module fork. Those runs used a minimum string length of 64 bytes. It is now fixed at
8, so keys and strings of 8 to 63 bytes that documents share are shared as well. The
probe scenarios were run again at 8 on the current build and pass; the flow tests have not
been run since. Not run: the flow tests written after those runs (the
cap on defined bytes, the byte-limit comparison against a capped stream, and the 8 byte
minimum),
BGSAVE copy-on-write measurements, the Flex read path through io_uring (the core in use
is built without it), cluster mode on Flex, a Flex RAM-only RDB with SST persistence
(`bigredis-sst-persistence`), full replication on Flex beyond the branch that copies the
bytes of a cold value without decoding it, a Flex AOF preamble with values on flash (the
run kept every value in RAM), the Flex limit on pinned strings beyond the one measurement
above (the harness limits RAM by key count), and gates G1 to G3.

### Flex byte-limit regression (2026-10-12)

The definition cap counts UTF-8 payload bytes, not total reader memory. Flex reserves
its memtable budget (8 MiB in this test); string allocation overhead, the ID table and
decode/spill buffers also consume memory. A 6 MiB definition cap did not load reliably
under a 24 MiB RAM limit; even the inline control failed in a repeat. At 32 and 40 MiB,
inline and capped files loaded exactly while the 48 MiB unbounded stream failed. The
regression now requires all three outcomes at 40 MiB and checks the writer emitted
exactly 6 MiB of definitions. These fixture limits are not production sizing guidance.

### Residency query and SDK validation (2026-10-19)

The resident-after-temporary-decode regression passes with the residency query.
All 33 persistence unit tests pass, including context
cleanup, unknown database/API fallback, and exact reads after temporary address reuse.
The complete Rust workspace passes with default features (429 tests) and `as-library`
(430 tests). Clippy passes for all targets with default features and warnings denied.

Docker validation uses Linux ARM64 and unstable core `40db9347`, in RAM and Flex modes.
The SDK validation ran the complete standalone pytest suite: 306 passed, 22 skipped,
including the rebuilt LLAPI consumer, defrag, and every legacy RDB truncation point.
The final persistence implementation passed the dedicated RDB matrix below. Skips are
excluded from the passed counts; RLTest's summary includes them.

| Storage | Mode | Passed | Skipped | Failed |
|---|---|---:|---:|---:|
| RAM | Standalone | 14 | 9 | 0 |
| RAM | Replica | 15 | 8 | 0 |
| RAM | AOF | 1 | 22 | 0 |
| Flex | Standalone | 18 | 5 | 0 |
| Flex | Replica | 20 | 3 | 0 |
| Flex | AOF | 1 | 22 | 0 |
