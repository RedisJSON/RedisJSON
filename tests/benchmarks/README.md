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

## Legacy absolute KPI floors

Some benchmark YAML files contain an absolute throughput floor:

```yaml
kpis:
  - ge:
      "$.Tests.Overall.rps": 194750.47
```

`redisbench-admin run-remote` checks these floors when they are present in the
input configuration. The local/Event CI and nightly comparison scripts remove
`kpis` from their temporary configurations; they do not enforce these floors or
modify the source YAML files. Their relative comparisons and nightly promotion
criteria are described above.

The nightly comparison uploads merged reports as
`nightly-initial-benchmarks-<run_id>` and, when a confirmation round runs,
`nightly-confirmation-benchmarks-<run_id>`, with 30-day retention. It does not run
`update_kpis.py` or automatically open PRs to update YAML floors. The
`benchmark-baseline` branch and the YAML floors are separate mechanisms.

For manual maintenance of the legacy floors, `update_kpis.py` uses raw benchmark
result JSON files in the current directory. It can raise a floor to
`measured * (1 - margin)` (default margin: 5%), but never lowers an existing floor.
This guarantee applies only to those YAML values, not to the moving
`benchmark-baseline` branch. Review and commit any proposed YAML changes manually.

Run from `tests/benchmarks` after placing the raw result files there; merged
`comparison.json` reports are not the input format:

```bash
python3 update_kpis.py --margin 0.05
python3 update_kpis.py --self-test   # checks raise/never-lower behaviour
```
