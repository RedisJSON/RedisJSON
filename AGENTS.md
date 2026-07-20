# [AGENTS.md](http://AGENTS.md)

Guidelines for AI coding agents (Claude Code, Cursor, etc.) working in this repository.

## Project Overview

RedisJSON is a Redis module implementing the JSON data type (ECMA-404) with JSONPath  
support. It's a Rust Cargo workspace compiled to a native Redis module (`.so`/`.dylib`)  
loaded via the Redis Modules API.

## Project Structure

```
.
├── Cargo.toml              # workspace root: members = [json_path, redis_json]
├── json_path/               # JSONPath parser/evaluator (RFC 9535-oriented), engine-agnostic
│   ├── src/
│   │   ├── grammar.pest    # pest grammar for the JSONPath language
│   │   ├── json_path.rs    # AST -> query compilation (`Query`, `compile()`)
│   │   ├── select_value.rs # `SelectValue` trait: abstracts over the JSON value backend
│   │   ├── json_node.rs    # a `SelectValue` impl for tree-walking
│   │   └── lib.rs
│   └── tests/               # Rust integration tests for path parsing/evaluation
├── redis_json/               # the Redis module itself
│   └── src/
│       ├── lib.rs           # module entry point, command registration
│       ├── commands.rs      # JSON.* command implementations
│       ├── manager.rs       # storage-agnostic manager trait + shared error helpers
│       ├── ivalue_manager.rs# manager impl backed by `ijson`/ `serde_json` values
│       ├── key_value.rs     # per-key JSON value wrapper used by commands
│       ├── redisjson.rs     # the `RedisJSON` value type + RDB (de)serialization
│       ├── backward.rs      # legacy RDB encoding-version compatibility
│       ├── array_index.rs   # array index parsing/normalization (supports negative indices)
│       ├── c_api.rs         # LLAPI: C ABI surface for other modules (e.g. RediSearch)
│       ├── defrag.rs        # active defrag support
│       ├── formatter.rs     # JSON output formatting (indent/newline/space options)
│       └── include/rejson_api.h  # C header for the LLAPI
├── tests/
│   ├── pytest/              # RLTest-based flow tests, run against a built `.so`
│   ├── files/                # JSON parsing fixtures (`pass-*.json` / `fail-*.json`)
│   ├── benchmarks/           # `redis-benchmark`/memtier YAML benchmark specs + results
│   ├── qa/                   # QA test suite references
│   └── memcheck/             # ASan/Valgrind suppression lists
├── docs/docs/                # user-facing documentation source
├── deps/                     # git submodules (RedisModulesSDK, readies build tooling)
├── licenses/                 # RSALv2 / SSPLv1 / AGPLv3 full text
└── .cursor/rules/            # editor-agnostic coding rules, imported below
```



### Crate responsibilities

- `json_path` has no Redis dependency — it's a standalone JSONPath engine. Changes
here affect path parsing/evaluation semantics used by every command in `redis_json`.
- `redis_json` depends on `json_path` and the `redis-module` crate (from
`RedisLabsModules/redismodule-rs`) for the Redis Modules API bindings.
- JSON values are represented via `ijson`, a compact interned/inlined `serde_json`-like
value type. Its object/map type preserves insertion order, so iteration order matters
when comparing behavior against plain `serde_json::Map`.



## Build & Test

```bash
# Compile the workspace
cargo build --release -p redis_json

# Fast unit/integration tests (no Redis server needed)
cargo test -p json_path
cargo test -p redis_json

# Full module build + package (see `make help` for all targets/flags)
make build
make pytest QUICK=1        # fast subset of the RLTest flow-test suite(One shard only)
make pytest                # full flow-test suite(All Topologies)
```

Flow tests (`tests/pytest/`) load the compiled module and drive it over the real Redis
protocol via [RLTest](https://github.com/RedisLabsModules/RLTest); they need a built
`.so`/`.dylib` and a `redis-server` binary on `PATH` (or `MODULE=<path>` set explicitly).

## Coding Guidelines

The canonical Rust and testing conventions for this repo are maintained as editor-agnostic
rule files under `.cursor/rules/` and imported here so there is a single source of truth:

### Rust

@.cursor/rules/rust.mdc

### Testing

@.cursor/rules/testing.mdc

## Extending the LLAPI (C API)

`redis_json/src/c_api.rs` implements RedisJSON's shared C API (the LLAPI) — the
mechanism other modules (e.g. RediSearch) use to read JSON values directly, in-process,
without going through the Redis command protocol. It's versioned and exported via
`RedisModule_ExportSharedAPI` under names `RedisJSON_V1` .. `RedisJSON_V<N>`
(`REDIS_JSONAPI_LATEST_API_VER`); a consumer binds to one version name and gets a
`RedisJSONAPI` struct of function pointers valid as of that version.

### The version contract

- `RedisJSONAPI_CURRENT` (Rust, in `c_api.rs`) and `RedisJSONAPI` (C, in
  `include/rejson_api.h`) must have byte-identical field order and layout — the struct
  crosses the FFI boundary as raw memory, not through Rust's type system.
- Because each version name resolves to the *same* current struct, existing fields must
  **never move, change type, or be removed** — only append new fields at the end, under
  a new version's grouping (see the `// V7 entries` / `// V8 entries` comments already
  in both files for the pattern).
- Adding a capability without bumping the version is a correctness bug: a consumer that
  resolved an older `RedisJSON_V<n>` name is only guaranteed the fields that existed at
  that version.

### Steps for adding a new LLAPI function

1. **Bump the version** in `redis_json/src/c_api.rs`:
   ```rust
   pub const REDIS_JSONAPI_LATEST_API_VER: usize = 9; // was 8
   ```
2. **Implement the manager-generic function**, following the existing `json_api_*`
   naming (e.g. `json_api_foo`), generic over `M: Manager` when it needs to reach a
   stored value.
3. **Wire it into `redis_json_module_export_shared_api!`** (still in `c_api.rs`):
   - add a `#[no_mangle] pub extern "C" fn JSONAPI_foo(...)` wrapper using the same
     `run_on_manager!` pattern as every other `JSONAPI_*` function,
   - add a `// V9 entries` block to the `JSONAPI_CURRENT` static, wiring
     `foo: JSONAPI_foo,`,
   - add the matching `// V9 entries` field to the `RedisJSONAPI_CURRENT` struct
     definition, in the same order.
4. **Mirror the new field in `redis_json/src/include/rejson_api.h`**:
   - add a `//// V9 entries ////` block with the C function-pointer field, matching the
     Rust field name and order exactly,
   - bump `#define RedisJSONAPI_LATEST_API_VER 9`.
5. **Add a Rust unit test** to the `#[cfg(test)] mod tests` block at the bottom of
   `c_api.rs` (see `test_json_api_get_at` for the pattern) — call the manager-generic
   function directly with a `RedisIValueJsonKeyManager` and an `IValue` fixture; no
   Redis server needed.
6. **Add flow-test coverage** so the function is exercised through the real C ABI, not
   just the Rust body:
   - in `tests/pytest/llapi_test_module/module.c`, add an `LLAPI.FOO` command function
     following the pattern of `GetAtCmd`/`GetArrayCmd` (call `japi->foo(...)`, reply
     with the result or an error), and register it with
     `REGISTER("LLAPI.FOO", FooCmd);` in `RedisModule_OnLoad`,
   - in `tests/pytest/test_llapi.py`, add a `testLLAPIFoo()` using `_env_with_doc()` and
     `env.cmd('LLAPI.FOO', 'doc', '$.path')`, covering both the happy path and the
     relevant error path (see the "Negative / error paths" section of that file for the
     existing style),
   - `llapi_test_module` always binds `RedisJSONAPI_LATEST_API_VER` (the newest name),
     so it must be rebuilt alongside the module under test — `tests/pytest/tests.sh`
     does this automatically before running `test_llapi.py`.

## Repo-Specific Notes

- **Error helpers live in** `redis_json/src/manager.rs` (`err_json`, `err_invalid_path`,
`err_invalid_path_or`, `err_recursion_limit_exceeded`) — use them instead of
constructing new `RedisError` strings ad hoc, per the Rust rules above.
- **RDB backward compatibility**: `redisjson.rs::rdb_load` dispatches on `encver`, and
`backward.rs` decodes the legacy RDB1 format for old values. When changing the on-disk
format, add a new `encver` case rather than breaking old readers.
- `unsafe` **blocks require a** `// SAFETY:` **comment** — this is enforced by the Rust rules
above and is especially load-bearing in `c_api.rs`, where lifetimes cross the FFI
boundary into caller-owned memory (e.g. RediSearch's LLAPI usage).
- Licensing is a tri-license (RSALv2 / SSPLv1 / AGPLv3); see `CONTRIBUTING.md` before
proposing large/behavioral changes — major features should be discussed as an issue
first, per that file.

