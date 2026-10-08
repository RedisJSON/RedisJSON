# Context

The automated benchmark definitions included within `tests/benchmarks` folder, provides a framework for evaluating and comparing feature branches and catching regressions prior to letting them into the master branch.

Local benchmarks require the runner pinned in `tests/benchmarks/requirements.txt`
and the benchmark tool specified in each workload. See the
[runner documentation](https://github.com/RedisLabsModules/redisbench-admin) for details.
Install the runner from the repository root:
```
python3 -m pip install -r tests/benchmarks/requirements.txt
```

## Usage

- Local benchmarks: `make benchmark`
- Remote benchmarks:  `make benchmark REMOTE=1`


## Branch throughput comparisons

Event CI builds master and the PR head separately, then distributes master's
YAML workloads across five GitHub runners. Each workload runs both modules
sequentially on the same runner with a fresh Redis instance and the same dataset.
Request counts, durations and module options come from the original workloads.
New PR-only workloads enter this comparison after merging into master.

For a local comparison, build both modules in release mode, install the runner
using the requirements file above, and put `redis-server`, `redis-benchmark` and
`memtier_benchmark` on PATH:

```sh
python tests/benchmarks/compare_local.py \
  --baseline-module /path/to/master/target/release/librejson.so \
  --candidate-module /path/to/pr/target/release/librejson.so \
  --output /path/to/new-results-directory
```

Use `.dylib` on macOS. Repeat `--benchmark <filename.yml>` to select workloads.
Temporary dataset copies escape non-ASCII JSON characters to work around the
pinned runner's ASCII output parser; source fixtures and JSON values are unchanged.

Reports contain throughput only. Percentage markers are 🟢 improvement,
🟡 degradation below 5%, and 🔴 degradation of 5% or more. A difference strictly
greater than 5% in either direction triggers three retries of both revisions,
without stopping early if a later pair falls within 5%. Execution order alternates
between baseline-first and candidate-first. 🟠 beside a test name indicates a retry.
The report and promotion use the median of all paired percentage changes. Throughput
columns show each revision's median independently, so their ratio need not match
the reported percentage. All original measurements remain under `attempts/`;
the last run's logs stay in the label directory and earlier logs are archived.
Execution errors stop retries and remain failures; incomplete sets are not aggregated.
Relative differences are report-only; execution errors,
missing workloads and inconsistent builds fail the comparison. Temporary specs
omit the legacy absolute `kpis` floors.
Local comparisons (including Event CI) use a per-run Redis PID file to clean up
unresponsive servers after verifying their executable and working directory.
If cleanup cannot be verified, remaining measurements in that shard are skipped.

## Nightly AWS comparison

Scheduled runs compare the pinned `benchmark-baseline` commit against master;
manual runs compare it against the selected dispatch commit. Identical commits
skip builds and AWS provisioning. Five shards each own an AWS server/client pair,
run both revisions sequentially, and destroy their resources even on failure.
The candidate's YAML suite supplies the workloads for both sides.

Each pair pins Redis to one CPU and clients to two physical cores. NIC interrupts
are assigned to housekeeping cores, irqbalance is stopped, and RPS is disabled.
Benchmark cores exclude current and requested IRQ destinations and SMT siblings.
Chosen cores stay fixed across revisions and retries; `affinity.json` records the
setup. This reduces interference without guaranteeing identical timings. Event CI
does not apply the AWS affinity configuration.

Only scheduled runs may advance `benchmark-baseline`. Every workload must pass,
none may degrade by 5% or more, and the geometric mean throughput improvement
must exceed 5%. A complete confirmation round uses the same binaries without
retries and must satisfy the same criteria, with matching revisions, server
version and coverage. The promotion job updates only `benchmark-baseline` to the
tested commit using a lease-protected push; concurrent changes are not overwritten.
Branch protection must permit this update. Manual runs never promote the baseline.
The nightly report's Master column denotes the candidate, including manual branches.

Merged reports verify workload coverage and binary identities. Logs, hashes,
commit IDs and attempt histories are retained in Actions artifacts:
`command-performance-<run>-<attempt>` for Event CI and
`nightly-<initial|confirmation>-benchmarks-<run>` for nightly.
Failed jobs can reuse the shared build and successful shards when rerun.
Nightly round artifacts use stable names across attempts, so confirmation reruns
can reuse the initial report. Rerunning a report replaces its round artifact.

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

These YAML floors are never written by CI. Raising one is proposed out of band, from the
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
