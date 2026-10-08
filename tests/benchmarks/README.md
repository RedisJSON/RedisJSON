# Context

The automated benchmark definitions included within `tests/benchmarks` folder, provides a framework for evaluating and comparing feature branches and catching regressions prior to letting them into the master branch.

To be able to run local benchmarks you need `redisbench_admin>=0.1.74` [[tool repo for full details](https://github.com/RedisLabsModules/redisbench-admin)] and the benchmark tool specified on each configuration file. You can install `redisbench-admin` via PyPi as any other package.
```
pip3 install redisbench_admin>=0.1.74
```

## Usage

- Local benchmarks: `make benchmark`
- Remote benchmarks:  `make benchmark REMOTE=1`

## Local master/PR performance and memory comparison

Event CI calls `flow-command-benchmark.yml` on each PR run. It builds master and
the PR head with one Rust toolchain in a shared build job. Five parallel jobs
then run master's YAML workloads. Each workload runs against both modules
sequentially on the same runner, using the shared binaries.
New PR-only workloads enter this comparison once merged into master.
The YAML request counts and durations are unchanged. Each test/revision gets a
fresh Redis instance and the same starting dataset; random-key workloads can
still end with slightly different key counts, which the report includes.

For a local comparison, build both modules in release mode, install
`redisbench-admin==0.12.39`, and put a compatible `redis-server`,
`redis-benchmark`, and `memtier_benchmark` on PATH. Then run:

```sh
python tests/benchmarks/compare_local.py \
  --baseline-module /path/to/master/target/release/librejson.so \
  --candidate-module /path/to/pr/target/release/librejson.so \
  --output /path/to/new-results-directory
```

Use `.dylib` on macOS. Add `--benchmark <filename.yml>` to select a workload;
repeat it to select several. Passing the same module twice provides a master vs
master check of measurement noise. No AWS credentials or results database are
needed. Dataset URLs are downloaded and cached within the output directory.

The pinned `redisbench-admin` version decodes redis-benchmark output as ASCII.
The Google Maps q3/q5 fixtures contain Unicode, which otherwise causes result
parsing to fail after the benchmark. Temporary JSON dataset copies therefore
escape non-ASCII characters using JSON `\u` escapes; the parsed documents and
numeric spellings are unchanged. The source fixtures are never modified.

The report contains throughput and these `INFO MEMORY` counters in bytes:

| Counter | Meaning |
| --- | --- |
| `used_memory` | Redis allocator-tracked memory after the clients finish |
| `used_memory_dataset` | Redis's dataset-memory estimate at that point |
| `used_memory_peak` | Redis's peak for this fresh process, **including dataset loading** |
| `used_memory_rss` | Resident memory; informational because of allocator/OS effects |

These are whole-server counters, not per-document `JSON.DEBUG MEMORY` values.
Redis tracks the peak itself, so there is no extra memory-polling loop competing
with the timed clients. A read-only workload still measures its loaded dataset
and the process peak. A write workload also measures the resulting dataset.

Changes are initially **report-only**, pending master-vs-master calibration.
Benchmark errors and missing results still fail the job and appear in the
report. AWS `kpis` floors are removed only from temporary workload copies.
The full suite can be expensive; this change does not shorten its workloads or
silently skip the known `json_nummultby_num_2` failure described below.

Results are saved as `comparison.json`, `summary.md`, and per-run raw client
JSON/logs, and uploaded as CI artifacts. The summary appears in the Actions job.

The five groups cover every workload exactly once. Initial grouping balances
request counts against existing throughput floors and memtier durations; these
are scheduling estimates, not runtime guarantees. Each result records
`run_seconds`; `benchmark_jobs.py plan --timings <previous-comparison.json>` can
use measured durations for subsequent plans. CI currently uses the estimates.

A final job combines all groups into one performance/memory report and checks
coverage and binary identities. Failed or missing measurements fail the job and
appear in the report; other groups continue running. Raw logs remain available
in artifacts. Rerunning failed jobs reuses the shared build and successful groups.

```sh
python -m unittest discover -s tests/benchmarks -p 'test_*.py'
```


## Included benchmarks

Each benchmark requires a benchmark definition yaml file to present on the current directory. The benchmark spec file is fully explained on the following link: https://github.com/RedisLabsModules/redisbench-admin/tree/master/docs

## Performance baselines

Each benchmark carries its own baseline as a `kpis` floor at the end of its yaml:

```yaml
kpis:
  - ge:
      "$.Tests.Overall.rps": 194750.47
```

`redisbench-admin run-remote` checks those floors itself and exits non-zero when
a run comes in below one, so any lane that runs the benchmarks also gates on
them — the `run-benchmark` PR label, every push to `master` / `feature-*` /
`X.Y`, and the nightly.

Baselines are never written by CI. Raising one is proposed out of band, from the
nightly analysis:

1. The nightly benchmark run uploads its result json files as
   `benchmark-results-<group>` artifacts (30 day retention).
2. When a run comes in faster than the committed floors, the nightly analysis
   proposes a bump — `update_kpis.py` applied to those results, opened as a PR.
3. A human merges it, accepting the higher bar. Leaving it unmerged keeps the
   current baselines.

One benchmark, `json_nummultby_num_2`, fails on every run — `redis-benchmark`
exits 1 with an empty result set, and has since at least 2026-08-09
(MOD-18654). It still runs, and it has no floor of its own.

That means **the benchmark job is red on every run until MOD-18654 is fixed**,
so job status alone does not tell you whether a baseline was breached. Read the
log instead:

| Log line | Meaning |
|---|---|
| `Condition on <metric> <measured> ge <floor> is False` | performance regression |
| `Failed to run remote benchmark for test '<name>'` | the benchmark itself errored (MOD-18654) |

The other 42 benchmarks run and report regardless — `run-remote` is not given
`--fail_fast`, so one failing test does not stop the rest.

`update_kpis.py` can only **raise** a floor, to `measured * (1 - margin)`
(margin 5% by default), so a baseline cannot drift downwards even by accident —
a slower run proposes nothing, and lowering a floor is always a hand edit in a
reviewed PR.

The 5% margin is a starting point taken from `redisbench-admin compare`'s own
regression waterline; revisit it against the run-to-run spread visible in the
[CI benchmarks dashboard](https://benchmarksrediscom.grafana.net/d/UErSC0jGk/redisjson-ci-benchmarks).

To seed or re-seed floors from a local run's results:

```
python3 update_kpis.py --margin 0.05
python3 update_kpis.py --self-test   # checks raise/never-lower behaviour
```
