# Context

The automated benchmark definitions included within `tests/benchmarks` folder, provides a framework for evaluating and comparing feature branches and catching regressions prior to letting them into the master branch.

To be able to run local benchmarks you need `redisbench_admin>=0.1.74` [[tool repo for full details](https://github.com/RedisLabsModules/redisbench-admin)] and the benchmark tool specified on each configuration file. You can install `redisbench-admin` via PyPi as any other package.
```
pip3 install redisbench_admin>=0.1.74
```

## Usage

- Local benchmarks: `make benchmark`
- Remote benchmarks:  `make benchmark REMOTE=1`


## Included benchmarks

Each benchmark requires a benchmark definition yaml file to present on the current directory. The benchmark spec file is fully explained on the following link: https://github.com/RedisLabsModules/redisbench-admin/tree/master/docs

## Performance regression gate

`perf_regression_gate.py` turns CI red when throughput regresses. It runs no benchmark of its
own: the EC2 suite above already pushes every result to the performance RedisTimeSeries instance
(`--push_results_redistimeseries`), and the gate reads those series back. GitHub runners are not
a trustworthy source of performance signal, so the only numbers it trusts are the ones measured
on the perf EC2 fleet.

It runs from `.github/workflows/flow-perf-regression.yml`, wired into two places:

- **after each merge to master** (`event-push-to-integ.yml`), once the new results have landed in
  RedisTimeSeries, so a regression is attributed to the commit that caused it;
- **nightly** (`event-nightly.yml`), as a safety net.

### How the baseline works

For each (test, metric) the baseline is the **best trailing-window median** in the query window
— the best *sustained* throughput, not the single luckiest run. That gives two properties:

- **It ratchets.** The baseline is a max over history, so a slow run can never lower it.
  Performance cannot erode silently one small step at a time.
- **It is robust.** Ratcheting on a median-of-N rather than on individual runs is what makes the
  ratchet usable: EC2 run-to-run variance is large, and a max over single noisy samples would
  latch onto an outlier and then fail forever.

The current value is the median of the newest `--window` runs, which are excluded from their own
baseline. A test is only judged once it has `2 × window` samples; below that it reports
`insufficient-data` rather than passing. The threshold widens to the noise floor actually
measured for that test, so a drop smaller than the spread the test normally shows does not fire;
past `--max-threshold-pct` observed noise the test is reported as not gateable instead of
flapping.

Because the window is finite (`--days`, default 180), a regression left unapproved and unfixed
for longer than the window eventually ages out of the baseline. Fix it or approve it; don't wait.

### Lowering the bar

Nothing the gate does can lower the bar. Accepting a regression as the new normal is a reviewed
change to `perf-baselines.json`, recording the new value, who approved it, when, and why. The
gate keeps reporting the derived baseline next to the approved one so the accepted cost stays
visible.

### Running it by hand

```
pip3 install 'redis==5.*' PyYAML
PERFORMANCE_RTS_HOST=... PERFORMANCE_RTS_PORT=... PERFORMANCE_RTS_AUTH=... \
  python3 perf_regression_gate.py --branch master
```

`--discover` prints the series and label values that actually exist for the configured filters,
which is the quickest way to debug an empty result set. Note that `--triggering-env` must match
`benchmark-flow.yml`'s `triggering_env` input, still `circleci`; a wrong value matches nothing.
An empty result set is always a hard failure — a gate that finds nothing must never be mistaken
for a gate that passed.

Exit codes: `0` clean (or warn-only), `1` regressions with `--enforce`, `2` the gate itself could
not run.

### Enabling enforcement

Both wirings currently pass `enforce: false`: the gate reports its verdict but stays green. This
is deliberate. Run it in warn-only mode long enough to see the real per-test variance in the job
summaries, adjust `--threshold-pct` / `--window` (or add per-test `threshold_pct` entries to
`perf-baselines.json`) for tests that are inherently noisy, then flip `enforce` to `true`. A gate
that flaps red is a gate people learn to ignore.

The decision logic has unit tests that need no RedisTimeSeries instance:

```
python3 -m unittest discover -s tests/benchmarks -p 'test_perf_*.py'
```

