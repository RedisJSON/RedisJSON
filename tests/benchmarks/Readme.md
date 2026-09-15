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

