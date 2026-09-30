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

The suite contains 72 benchmarks. Each of the 27 path forms below has separate
`compile/<name>` and `eval/<name>` measurements with an expected-result assertion.
The original 18 workloads cover nested-filter compilation, recursive searches,
existence tests, root-relative comparisons, string membership/equality, and regexes.

| Path form | Examples |
| --- | --- |
| Root and fields | `$`, `$.name`, `$['display.name']`, `$["a\"b"]`, `$.profile.address.city` |
| Missing selections | `$.missing`, `$.numbers[128]` |
| Wildcards | `$.metrics.*`, `$.numbers[*]` |
| Negative index and slices | `$.numbers[-1]`, `[8:24]`, `[-8:]`, `[::4]` |
| Unions, including duplicate results | `$.numbers[3,0,3]`, `$.metrics['c','a','c']` |
| Logical filters | `&&`, `\|\|`, `!` over 256 rows with matching and nonmatching candidates |
| Arithmetic/function filters | `(@.score + 1) * 2 >= 510`, `length(@.name) == 5` |
| Projections | Arithmetic, `length()`, `.first().length()`, `.sum()`, `~`, `.append()`, missing operands |

Projection benchmarks use `calc_once_projection`, with query cloning outside the
timed routine via Criterion's batched setup. Path benchmarks use the reusable
calculator. Both include result cleanup in the measured time. All fixtures use
`IValue`. Coverage is representative of these forms, not every function, operator,
or input shape.

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
Both revisions need the Criterion harness. For a quick correctness check without
timing, run `cargo test -p json_path --bench path_performance`.

## CI reports

Non-draft, non-documentation-only PRs benchmark only the PR head. Pushes to
`master` also run the suite to publish shared baselines. These benchmarks do not
run in the nightly workflow. CI never checks out or rebuilds the base revision.

Each successful run saves Criterion's measurements and a single-run benchmark
JSON report using GitHub Actions' cache. The next run restores the latest accessible
compatible cache: a previous run of the same PR, or a cache from its target/default
branch. PR caches are scoped to that PR and cannot become another PR's baseline.
The summary identifies the actual saved commit and cache key; this is not
necessarily the PR's base SHA.

The cache key includes the Ubuntu runner version and architecture, benchmark
source, Rust toolchain file, Criterion version, and this workflow. Changes to
production code or dependencies other than Criterion do not reset the baseline.
When no compatible cache exists, CI measures and saves results without applying
the slowdown gate. The same happens when the saved commit equals the current
commit. Adding or changing benchmarks starts a fresh baseline for the suite.
GitHub may evict caches; a cache miss safely starts a new baseline again.

Criterion runs with `--save-baseline saved`: it compares against restored
measurements when present, then saves the current measurements for future runs.
Confirmation runs use a separate copy of the original saved baseline.

Criterion's default warmup and measurement periods remain unchanged. The job has
a 60-minute timeout for building, measuring, and any confirmation reruns;
only flagged workloads are rerun.

GitHub Actions displays a comparison table in the job summary and retains the
`jsonpath-performance-*` artifact for 30 days. It contains text output, comparison
JSON, commit/cache provenance, and Criterion HTML reports under `criterion/report/`.
The cache supplies future baselines; the artifact retains reports for inspection.

Any workload at least 5% slower is rerun on the current revision against the same
saved measurements. CI fails only when the same workload is still at least 5%
slower in that second comparison. A slowdown that disappears on rerun passes.
The comparison action stays report-only; the confirmation gate enforces the
inclusive 5% threshold. Failed runs do not publish a new baseline. Build failures,
incorrect results, and missing confirmation measurements also fail.

Initial reports remain intact; confirmation output and HTML reports are retained
under `confirmation/` in the same artifact, including when the gate fails.
Hosted-runner timings can vary between machines and runs. Repeating the candidate
does not eliminate differences from the saved baseline's machine, so inspect
reports when investigating a failure.
No PR comments or GitHub Pages publishing are enabled.

Run the gate's boundary, rerun, and data-validation tests with:

```sh
python3 -m unittest discover -s .github/scripts -p 'test_jsonpath_benchmark_gate.py'
```

## Adding workloads

Add an entry to `path_forms` to benchmark both compilation and evaluation, or add
another `c.bench_function` or `evaluate(c, ...)` call for a focused workload. Use a
unique, stable name and keep fixtures outside timed closures. Evaluation helpers
assert expected results before timing. The first CI run with a changed suite saves
a fresh baseline; subsequent compatible runs compare automatically. Local reports
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
