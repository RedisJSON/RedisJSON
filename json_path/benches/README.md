# JSONPath performance benchmarks

Run the Criterion suite with optimized code:

```sh
cargo bench -p json_path --bench path_performance
```

Open `target/criterion/report/index.html` for timings, distributions, and changes
against the previous run. `compile/*` measures JSONPath compilation; `eval/*`
measures evaluation of precompiled paths against `IValue`. Fixtures and result
checks are outside the timed loops. Redis commands and network overhead are not
measured.

## Coverage

The suite contains 81 benchmarks. Each of the 28 path forms below has separate
`compile/<name>` and `eval/<name>` measurements with an expected-result assertion.
The other 25 workloads cover nested-filter compilation, recursive searches,
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

Projection benchmarks use `calc_once_projection`, with query cloning outside the
timed routine via Criterion's batched setup. Path benchmarks use the reusable
calculator. Both include result cleanup in the measured time. All fixtures use
`IValue`. Coverage is representative of these forms, not every function, operator,
or input shape.

The CI job runs the entire target without a workload filter on its first pass.
These cases cover the syntax affected by the evaluator and grammar optimizations:

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

## Comparing revisions

To compare two revisions, save a named baseline, switch revisions, and compare
without deleting the benchmark data:

```sh
# On the base revision:
cargo bench -p json_path --bench path_performance -- --save-baseline main
# On the candidate revision:
cargo bench -p json_path --bench path_performance -- --baseline main
```

Use the same benchmark source, Rust toolchain, and machine for both revisions.
When using separate worktrees, set `CRITERION_HOME` to the same absolute directory.
Keep their Cargo build directories separate; only share the Criterion measurements.
Both revisions need the Criterion harness. For a quick correctness check without
timing, run `cargo test -p json_path --bench path_performance`.

## CI reports

Non-draft, non-documentation-only PRs benchmark the PR head first. Pushes to
`master` also run the suite to publish shared baselines. These benchmarks do not
run in the nightly workflow. Master is checked out and rebuilt only when the
saved comparison flags a slowdown requiring confirmation.

Only successful pushes to `master` publish baseline caches. Every run selects the
newest compatible cache explicitly from `refs/heads/master`, then restores its
exact key. PR measurements remain in artifacts and never replace the baseline.
The summary identifies the saved master commit and cache key; this is the latest
available measurement, not necessarily the current master tip or PR's base SHA.

Cache selection matches the Ubuntu runner version, architecture, and Criterion
version. It also accepts existing master caches created with the old suite-hash
keys. Benchmark additions and workflow/reporting edits no longer discard existing
measurements. Workloads with matching names are compared; new or renamed workloads
are marked **No master baseline** until measured on master. Names must change when
fixtures or measured operations change; see below. Compiler and production
dependency changes are included in the comparison.

If no master cache exists (including after eviction), or the saved commit equals
the candidate, CI reports measurements without applying the slowdown gate. PRs
cannot seed a shared baseline; the next successful master run does that.

Criterion runs with `--save-baseline saved`: it compares against restored
measurements when present, then saves the current measurements in the job artifact.
Only successful master runs also publish them as future baselines.
Confirmation measures the exact saved master commit and the candidate on the same
runner, using the candidate's benchmark source and toolchain. Cargo build directories
are separate so artifacts cannot be reused across revisions. Both builds must
support the shared benchmark harness. Fresh Criterion measurements are kept under
`confirmation/criterion`, separate from the saved comparison.

Criterion's default warmup and measurement periods remain unchanged. The job has
a 60-minute timeout for building, measuring, and any confirmation reruns;
only flagged workloads are rerun.

GitHub Actions displays colored bars for the largest time changes and a collapsible
table of every workload's baseline, candidate, percentage change, first-pass delta,
and status. Missing baselines are explicitly reported without claiming a comparison.
Both tables include the exact JSONPath emitted by the benchmark harness to
`paths.jsonl`, including generated nested filters. Metadata is written outside the
timed loops. Nanosecond timings are displayed as ns, µs, ms, or s per iteration;
raw measurements and the 5% comparison remain unchanged.
The summary is still generated when the regression gate fails; `summary.md` is
included in the artifact. The workflow retains the
`jsonpath-performance-*` artifact for 30 days. It contains text output, comparison
JSON, commit/cache provenance, CPU/compiler details, and Criterion HTML reports
under `criterion/report/`. New master caches also retain environment details so
later artifacts can include `baseline-environment.txt` beside `environment.txt`.
The cache supplies future baselines; the artifact retains reports for inspection.

Any workload at least 5% slower is measured again on both master and the current
revision, on the same runner. CI fails only when the same workload is still at
least 5% slower in that fresh comparison. A slowdown that disappears passes.
The comparison action stays report-only; the confirmation gate enforces the
inclusive 5% threshold. Failed runs do not publish a new baseline. Build failures,
incorrect results, and missing confirmation measurements also fail.

Initial reports remain intact; confirmation output and HTML reports are retained
under `confirmation/` in the same artifact, including when the gate fails.
The summary and chart use fresh timings for remeasured workloads; the first-pass
column preserves their cached-baseline delta. Other workloads retain the saved
comparison. Hosted-runner timings can still fluctuate, but confirmation no longer
compares measurements taken on different machines.
No PR comments or GitHub Pages publishing are enabled.

Run the gate's boundary, rerun, and data-validation tests with:

```sh
python3 -m unittest discover -s .github/scripts -p 'test_jsonpath_benchmark_gate.py'
```

## Adding workloads

Add an entry to `path_forms` to benchmark both compilation and evaluation, or add
another `c.bench_function` or `evaluate(c, ...)` call for a focused workload. Use a
unique, stable name and keep fixtures outside timed closures. Evaluation helpers
assert expected results before timing. Adding a workload preserves comparisons for
existing names; only the new workload waits for a master baseline. If changing an
existing fixture or measured operation, rename its benchmark (for example, append
`-v2`) so CI does not compare different workloads under the same name. Local reports
and the confirmation gate discover workload names automatically.

For another crate, add Criterion as a dev dependency, create a file under its
`benches/` directory, and register a `[[bench]]` target with `harness = false`.
For example, a future `redis_json` target named `value_performance` would run as:

```sh
cargo bench -p redis_json --features as-library --bench value_performance
```

`redis_json` already exposes an `rlib` named `rejson`. Pure Rust operations can be
benchmarked directly; operations that call Redis APIs need the appropriate Redis
environment. The current workflow is specific to `json_path/path_performance`;
adding another suite requires CI wiring for its package, target, and features.
The 5% gate accepts any single-suite benchmark-action comparison file and does
not need new workload names hardcoded into it.
