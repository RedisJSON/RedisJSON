# Context

The automated benchmark definitions included within `tests/benchmarks` folder, provides a framework for evaluating and comparing feature branches and catching regressions prior to letting them into the master branch.

To be able to run local benchmarks you need `redisbench_admin>=0.1.74` [[tool repo for full details](https://github.com/RedisLabsModules/redisbench-admin)] and the benchmark tool specified on each configuration file. You can install `redisbench-admin` via PyPi as any other package.
```
pip3 install redisbench_admin>=0.1.74
```

## Usage

- Local benchmarks: `make benchmark`
- Remote benchmarks:  `make benchmark REMOTE=1`

## Local master/PR throughput comparison

Event CI calls `flow-command-benchmark.yml` on each PR run. It builds master and
the PR head with one Rust toolchain in a shared build job. Five parallel jobs
then run master's YAML workloads. Each workload runs against both modules
sequentially on the same runner, using the shared binaries.
New PR-only workloads enter this comparison once merged into master.
The YAML request counts and durations are unchanged. Each test/revision gets a
fresh Redis instance and the same starting dataset.

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

The report contains throughput only, with one row per benchmark and Master,
PR, and Change % columns (Baseline and Master for nightly). Memory counters
are not collected or compared by this harness.

Only Change % cells receive markers: 🟢 for improvement, 🟡 for degradation
below 5%, and 🔴 for degradation of 5% or more, using unrounded measurements.
Higher throughput is better. Unchanged values are unmarked; errors remain red.

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

A final job combines all groups into one throughput report and checks
coverage and binary identities. Failed or missing measurements fail the job and
appear in the report; other groups continue running. Raw logs remain available
in artifacts. Rerunning failed jobs reuses the shared build and successful groups.

```sh
python -m unittest discover -s tests/benchmarks -p 'test_*.py'
```


## Nightly AWS comparison

Scheduled and manually dispatched Event Nightly runs call
`flow-benchmark-nightly.yml`: one shared build, five parallel benchmark jobs,
and one combined report. The same planner used by Event CI assigns every master
YAML workload exactly once. Each shard provisions its own `defaults.yml` AWS
server/client pair, then runs baseline and current master sequentially for each
assigned workload. Up to five AWS pairs are active at once. Request counts
and durations are unchanged. Every revision/test gets a fresh Redis instance;
the report includes the same throughput metric as the PR comparison.
Each shard destroys its own AWS resources in an `always()` step, including after
failures. Other shards continue when one fails. The final merge reports missing
results and checks module hashes, commit hashes, and Redis versions across jobs.
Stable artifact names allow failed jobs to be rerun using the same shared build
and the successful shards from the previous attempt.

For the same-binary debugging experiment, each AWS pair configures CPU affinity
once before measuring: Redis uses one fixed CPU (`server-cpulist`), and both
client tools use the same two physical cores via `taskset`. NIC interrupts are
assigned to two housekeeping cores, irqbalance is stopped if active, and software
receive steering (RPS) is disabled. Benchmark cores are then chosen to exclude
both current and requested IRQ destinations, including SMT siblings. This allows
idle IRQs to migrate later without entering the benchmark cores. Setup waits up
to 10 seconds only if too few isolated cores are available, then fails with CPU
and IRQ details. The chosen cores remain fixed for both comparison runs.
These settings affect only the disposable nightly hosts;
PR CI is unchanged. Each shard saves `affinity.json`, and diagnostics capture
the effective IRQ affinity and Redis process affinity. This controls CPU placement,
but does not guarantee identical timing or eliminate other OS/hypervisor noise.

Each measurement also saves `diagnostics/server-{before,after}.json` and
`diagnostics/client-{before,after}.json` alongside its raw results. These Linux
snapshots run outside the measured interval and record CPU/network counters and
Redis process/thread scheduler counters. The server's final snapshot includes
Redis command execution times and the remote module hash. Use them to investigate
same-binary throughput differences; snapshot errors are recorded without failing
the benchmark. The throughput report and workload settings are unchanged.

The baseline defaults to `master`, resolved to the exact same commit as the
current-master checkout, so initially this measures master-versus-master noise.
Set the repository variable `BENCHMARK_BASELINE_REF` to a fixed commit SHA to
pin the scheduled baseline. The manual `benchmark-baseline-ref` input overrides
that variable. Both resolved commit hashes appear in the report.

Results and logs are uploaded as `nightly-aws-benchmarks-<run>-<attempt>` and the
comparison appears in the job summary. Relative changes are report-only;
benchmark errors fail the job. This comparison uses its own per-run artifacts,
not historical RedisTimeSeries samples or the old absolute KPI floors.
The PR comparison retains its five parallel jobs. Push-triggered Event Nightly
runs skip the AWS comparison, as the existing integration-push workflow already
has its own AWS benchmark lane.


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
a run comes in below one. The legacy AWS lanes (`run-benchmark` PR label and
integration pushes) retain that gate. The nightly and five-job PR comparisons
remove floors from temporary specs and report relative changes instead.

Baselines are never written by CI. Raising one is proposed out of band, from the
nightly analysis:

1. Legacy AWS runs upload raw results as
   `benchmark-results-<environment>-<group>` artifacts (30 day retention).
   The nightly comparison includes raw results in its per-test artifact folders.
2. When a run comes in faster than the committed floors, the nightly analysis
   proposes a bump — `update_kpis.py` applied to those results, opened as a PR.
3. A human merges it, accepting the higher bar. Leaving it unmerged keeps the
   current baselines.

One benchmark, `json_nummultby_num_2`, fails on every run — `redis-benchmark`
exits 1 with an empty result set, and has since at least 2026-08-09
(MOD-18654). It still runs, and it has no floor of its own.

That means **affected benchmark jobs fail until MOD-18654 is fixed**,
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

Nightly and Event CI comparisons retry both revisions when the absolute throughput
change exceeds 5%, with at most three retries (four pairs total). Exactly 5%
does not trigger a retry. The report uses the last pair and shows its attempt
number, even if the final difference still exceeds 5%. Earlier raw results and
all pair measurements are retained under each benchmark's `attempts/` directory.
Execution errors are not retried by this policy. Selecting results this way can
bias comparisons toward smaller differences; retain the history when assessing
stability. A 🟠 marker beside the benchmark name identifies retried tests in both
reports; percentage colors continue to describe the final result.
