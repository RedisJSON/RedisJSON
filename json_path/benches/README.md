# JSONPath instruction benchmarks

The complete `path_performance` suite uses [Gungraun](https://github.com/gungraun/gungraun)
and Valgrind Callgrind. It measures JSONPath compilation
and evaluation against `IValue`, without Redis commands or network overhead.

## Run

Use Linux with Valgrind, the repository's Rust toolchain, and a runner matching
the exact Gungraun version in `json_path/Cargo.toml`:

```sh
cargo install --locked --version 0.20.0 gungraun-runner
bash json_path/benches/run_instructions.sh --test
bash json_path/benches/run_instructions.sh --save-baseline=initial
```

`--test` runs every fixture and expected-result assertion without Valgrind.
The measurement run saves an instruction baseline. To compare another revision,
run the same harness in that checkout with the same absolute `GUNGRAUN_HOME`:

```sh
bash json_path/benches/run_instructions.sh --baseline=initial --callgrind-limits='ir=5%'
```

Gungraun fails with exit code 3 when a workload executes more than 5% additional
instructions. Exactly 5% passes. Comparisons use matching workload IDs; keep
fixture inputs and measured operations identical across revisions.

The helper builds `path_performance` and copies Cargo's reported executable to a
stable path under `target/gungraun-instructions/bin/`. Reports are stored alongside
it. Keeping the executable path fixed avoids allocator-state differences caused
by different build paths.

For a direct run, `cargo bench -p json_path --bench path_performance` also works,
but does not stabilize the executable path. If Docker blocks `setarch` with
`Operation not permitted`, add `--allow-aslr=true` to measurement commands.
ASLR can affect simulated cache metrics.

## Measurement boundaries

- `compile/*` counts parsing and destruction of the compiled query. Path creation,
  validation, warmup, and input-string destruction are excluded.
- `eval/*` counts one evaluation and result destruction. Fixture creation, parsing,
  expected-result assertions, and one warmup evaluation are excluded.
- Path evaluation uses `create().calc()` and retains its query until collection
  ends. Projection evaluation uses `calc_once_projection`, which consumes its
  prepared query; cloning for validation and warmup happens outside collection.
  Document destruction is excluded in both cases.

Gungraun's setup functions prepare the inputs before collection starts. A compiled
query borrows its path, so setup retains one generated path string for the
benchmark process's lifetime. Each measured case runs in a separate short-lived
process. Returning the query/document from the benchmark keeps fixture destruction
outside collection. The projection/path dispatch is included in evaluation counts.

`Ir` counts executed instructions. Reads (`Dr`) and writes (`Dw`) count data
accesses. Estimated cycles and cache metrics come from Callgrind's simulation;
they are not elapsed time or hardware measurements. Instruction counts avoid
hosted-runner timing noise, but randomized maps and allocator state can still
change the work performed. Keep the toolchain, target, dependencies, fixtures,
and Valgrind version fixed when interpreting changes.

## Coverage

The suite contains 83 workloads: 36 compilation and 47 evaluation
cases. Each of the 28 path forms below has separate `compile/<name>` and
`eval/<name>` measurements. Every evaluation validates its expected result.
The other 27 workloads cover nested-filter compilation, recursive searches,
existence tests, root-relative comparisons, string membership/equality, and regexes.

| Path form | Examples |
| --- | --- |
| Root and fields | `$`, `$.name`, `$['display.name']`, `$["a\"b"]`, `$.profile.address.city` |
| Missing selections | `$.missing`, `$.numbers[128]` |
| Wildcards | `$.metrics.*`, `$.numbers[*]` |
| Negative index and slices | `$.numbers[-1]`, `[8:24]`, `[-8:]`, `[::4]` |
| Unions, including duplicate results | `$.numbers[3,0,3]`, `$.metrics['c','a','c']` |
| Logical filters | `&&`, `\|\|`, `!`, parenthesized groups over 256 rows with matching and nonmatching candidates |
| Arithmetic/function filters | `(@.score + 1) * 2 >= 510`, `length(@.name) == 5` |
| Projections | Arithmetic, `length()`, `.first().length()`, `.sum()`, `~`, `.append()`, missing operands |

| Optimized behavior | Benchmark cases and syntax |
| --- | --- |
| Factored nested grammar | `compile/nested-{7,8,9}` for nested existence filters; `compile/nested-{grouped,comparison,arithmetic}-9` for `(@.path)`, `(@.path > 0)`, and `(@.path) > 0` |
| Grouped filter evaluation | `compile/filter-grouped`, `eval/filter-grouped`: `[?(@.score >= 128 && @.active == true)]` |
| Lazy object traversal | `eval/recursive-objects`, `eval/recursive-no-match`, `eval/object-wildcard`: `$..uid`, `$..absent`, `$.metrics.*` |
| Existence early exit | `eval/existence-{early-match,no-match}`: `[?@..flag]`, `[?@..absent]` |
| Root cache and its boundaries | `eval/root-{scalar,descendant-list,descendant-sum,single-candidate}`: `[?@.score > $.threshold]`, root descendant lists and `.sum()`; `eval/root-existence`, `eval/root-existence-missing`: `[?$.thresholds..limit]`, `[?$.thresholds..absent]` |
| Single-use projections | `eval/projection-{arithmetic,aggregate,method-chain,nothing}` guard against caching/dispatch overhead outside repeated filters |
| Regex cache modes | `eval/regex-search-cache`, `eval/regex-search-function`, `eval/regex-match-cache`: `=~`, `search()`, `match()` |
| Borrowed string comparisons | `eval/string-membership`, `eval/deep-string-equality`: `in`, equality of objects containing strings |
| Result buffer | `eval/simple`, `eval/array-wildcard`, `eval/recursive-objects` cover small and large selections |

## CI reports

`.github/workflows/flow-jsonpath-benchmark.yml` compares the PR head against the
`master` tip pinned at checkout. Pushes to `master` compare against the preceding
master commit (`github.event.before`). Both revisions run on the same runner.

Each job:

1. Copies the candidate benchmark harness, runner helper, and dev dependencies
   into the baseline checkout. Production sources and dependency requirements
   remain those of each revision. `cargo update --workspace` resolves the harness
   dependencies in the baseline lockfile; subsequent builds use `--locked`.
2. Builds and validates both suites using the candidate Rust toolchain and pinned
   Gungraun runner. Cargo build directories stay separate. Both checkouts must
   support the shared harness and its expected-result assertions.
3. Measures master with `--save-baseline=master`, then the candidate with
   `--baseline=master --save-baseline=candidate --callgrind-limits='ir=5%'`.
   Both use the same `GUNGRAUN_HOME`, executable path, and metadata path.
4. Publishes the comparison summary, then fails CI if Gungraun reported an
   instruction-count increase over 5%. Every workload finishes before gating;
   regression failures retain the summary and profiles.

The GitHub job summary shows both commit SHAs, colored bars for the largest
instruction-count changes, and a collapsible table of every workload's exact
JSONPath, master count, candidate count, percentage change, and regression status.
A second table shows data reads/writes and simulated cycle costs for both runs.
The report uses Gungraun's regression verdicts, without a separate Python gate.

The report requires matching workload names and JSONPaths, and checks that each
candidate result's native baseline counts match the master measurements from this
job. Missing baselines cannot silently pass. Build failures, fixture assertions,
measurement errors, and incomplete or invalid reports also fail CI.

The `jsonpath-performance-*` artifact is retained for 30 days and contains:

- `summary.md`, `master.jsonl`, `results.jsonl`, and both sets of JSONPath metadata.
- Both commit SHAs, resolved lockfiles, and CPU/compiler/Valgrind/Gungraun versions.
- Gungraun summaries and raw Callgrind profiles for both named baselines under
  `gungraun/`.

Every job measures a fresh master baseline. No measurements from earlier jobs are
restored. Results appear in the GitHub Actions summary linked from the PR check;
no PR comments or GitHub Pages publishing are enabled.

To render a downloaded comparison locally:

```sh
python3 .github/scripts/jsonpath_benchmark_report.py benchmark-results/results.jsonl \
  --paths-file benchmark-results/paths.jsonl \
  --baseline-file benchmark-results/master.jsonl \
  --baseline-paths-file benchmark-results/master-paths.jsonl > benchmark-results/summary.md
```

The metadata file must be empty before each measurement run. It is written during
setup, outside collection. Save master's metadata before emptying the same file
for the candidate. Keep measurements sequential when sharing `GUNGRAUN_HOME` or
a metadata file.

Run comparison, baseline-validation, formatting, escaping, and CLI tests with:

```sh
python3 -m unittest discover -s .github/scripts -p 'test_jsonpath_benchmark_report.py'
```

## Adding workloads

Add fixtures to `path_form` or `eval_case`, then register named `#[bench::...]`
cases on `compile_path` and/or `eval_path`. Every name must be unique within its
phase. Gungraun IDs use underscores; report names replace them with hyphens.
Supply the same hyphenated name to the setup helper so metadata matches the ID.
Keep expected-result assertions in setup and measured work in the benchmark body.

Rename a workload when its inputs or measured operation change. Run `--test` and
one full Callgrind pass; the report rejects missing metadata and zero instruction
counts.
