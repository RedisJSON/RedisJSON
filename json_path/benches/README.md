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

Non-draft, non-documentation-only PRs benchmark the current `master` tip and the
PR head in the same job. Master is pinned to the SHA checked out at job start;
it is not the PR's merge base. Pushes to `master` compare against the previous
master tip (`github.event.before`). These benchmarks do not run in the nightly
workflow.

Measurement caches are neither restored nor published. Results from different
hosted runners are unsuitable for the 5% gate: two attempts of the same PR used
Intel Xeon 8573C and AMD EPYC 7763 CPUs, and median candidate timings differed by
32%. Measuring both revisions on one runner removes that hardware mismatch;
within-job timing noise can still occur.

Each job:

1. Builds both revisions with the candidate's Rust toolchain, using separate Cargo
   build directories. Each revision retains its own production dependencies.
2. Lists the original benchmark names in both revisions, then copies the candidate
   harness into the master checkout. Master measures the shared names; the candidate
   measures every workload. New or renamed workloads are reported as **No master
   baseline** until they exist on master. Both revisions must support the shared
   harness. Empty, invalid, or disjoint workload lists fail the job.
3. Measures master with `--save-baseline master`, then the candidate with
   `--baseline-lenient master`, sharing only the job's `CRITERION_HOME`. Criterion
   compares existing workloads and permits candidate-only workloads without a
   baseline. No previous workflow run is needed, even for the first PR run.
4. Remeasures every workload at least 5% slower on both revisions, on the same
   runner. CI fails only if the same workload is still at least 5% slower in that
   second comparison. The threshold is inclusive; a slowdown that disappears
   passes. Build failures, incorrect results, and missing confirmation measurements
   also fail.

Criterion's default warmup and measurement periods remain unchanged. The job has
a 90-minute timeout for both builds, both full measurement passes, and any flagged
workload reruns. Both builds finish before measurements start. The comparison
action stays report-only; the Python confirmation gate controls failure.

### Native timing investigation

The workflow currently runs additional diagnostics before the full measurements
to investigate platform-dependent regressions in `eval/simple`, `eval/deep-field`,
`eval/projection-function`, and `eval/filter-and`. At both 100 and 500 samples it
runs **reference, PR, PR, reference** twice, using the already-built executables. Warmup
remains 3 seconds and measurement time 5 seconds. The binary hashes, individual
logs, raw Criterion samples, and unrounded median estimates are retained under
`diagnostics/` in the job artifact.

For the cache-storage experiment the reference is the unchanged PR at
`1a86095d7fe11b36d4761cb4df70e8934a9ea179`, built separately on the same runner.
Its SHA is recorded in `diagnostic-reference.txt`. The full regression gate still
compares the candidate against master; diagnostic measurements do not replace it.

The diagnostic summary separates candidate/reference comparisons by run order and reports
changes between successive runs of each identical binary. Same-binary changes
reveal measurement drift; order-dependent candidate/reference changes suggest a timing bias.
Neither establishes that a particular code change is harmless. These diagnostics
do not change the existing 5% gate or its inputs. They are investigation work and
add roughly 15 minutes plus the extra build; remove them when native runner behavior is understood.

### Results and artifacts

GitHub Actions displays colored bars for the largest time changes and a collapsible
table of every workload's baseline, candidate, percentage change, first-pass delta,
and status. Missing baselines are explicitly reported without claiming a comparison.
Both tables include the exact JSONPath emitted by the benchmark harness to
`paths.jsonl`, including generated nested filters. Metadata is written outside the
timed loops. Nanosecond timings are displayed as ns, µs, ms, or s per iteration;
raw measurements and the 5% comparison remain unchanged.

The summary and chart use confirmation timings for remeasured workloads; the
first-pass column preserves their initial same-runner delta. Other workloads use
the first pass. The summary is generated even when the regression gate fails.

The `jsonpath-performance-*` artifact is retained for 30 days. It contains
`summary.md`, both revisions' text output, comparison JSON, pinned commit details,
CPU/compiler details, workload lists, and Criterion HTML reports under
`criterion/report/`. Initial measurements remain intact; confirmation output and
HTML reports are retained separately under `confirmation/`. Artifacts are for
inspection, not baselines for future jobs. No PR comments or GitHub Pages
publishing are enabled.

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
