#!/usr/bin/env python3
"""Decide the benchmark verdict, failing only on a breach seen two runs in a row.

Each ``tests/benchmarks/<test>.yml`` carries its performance baseline as a
redisbench-admin ``kpis`` floor (see ``update_kpis.py``). redisbench-admin
checks those floors itself, but it judges a single sample: one run below the
floor fails the job.

That does not work here. Measured over four consecutive nightlies that all ran
the identical commit ``a3b2edb63`` (MOD-18822), the harness's own run-to-run
spread is 22-49% for the benchmarks above ~100k rps -- roughly six times the 5%
margin the floors are set at -- so 20 breaches were recorded on code that could
not have regressed. Re-seeding the floors does not help: they were re-seeded
the week before and every one of those breaches is on the new floors.

So this script takes the verdict over from redisbench-admin's own check and
requires a breach to repeat before it fails the job. No benchmark breached on
consecutive nights in that four-night sample, so the repeat requirement removes
every false alarm observed, while a genuine slowdown -- which does not go away
overnight -- still fails, one night later than it otherwise would.

This is a stopgap for the gate, not a fix for the measurement: a regression
smaller than the harness's own spread stays invisible either way. Narrowing
that spread is MOD-18823; once it reports a residual figure, the flat margin
should become per-benchmark and this repeat requirement can be revisited.

Usage (from tests/benchmarks, after a run left its result json files there):

    python3 check_kpis.py --previous prev.json --state-out state.json
    python3 check_kpis.py --self-test

Exit status is 0 when nothing breached twice in a row (first-time breaches are
reported as warnings) and 1 on a confirmed breach, or when the run produced no
results at all.
"""

import argparse
import glob
import json
import os
import sys

from update_kpis import METRICS, benchmark_name, result_metric, value_re

OK, BREACH, CONFIRMED, MISSING, UNGATED = "ok", "breach", "breach-confirmed", "missing", "ungated"


def floor_of(text):
    """(metric, floor) declared in this yml's kpis block, or (None, None)."""
    for metric, _ in METRICS:
        match = value_re(metric).search(text)
        if match:
            return metric, float(match.group("value"))
    return None, None


def measured(results_dirs, name):
    """Throughput measured for `name` in the first results dir that has it."""
    for results_dir in results_dirs:
        metric, value = result_metric(results_dir, name)
        if metric is not None:
            return value
    return None


def verdict(benchmarks_dir, results_dirs, previous):
    """(rows, breached) for this run.

    `previous` is the set of benchmark names that breached in the last run.
    `rows` is (name, floor, value, status); `breached` is the name set to carry
    into the next run.
    """
    rows, breached = [], set()
    for path in sorted(glob.glob(os.path.join(benchmarks_dir, "*.yml"))):
        if os.path.basename(path) == "defaults.yml":
            continue
        name = benchmark_name(path)
        if name is None:
            continue
        with open(path) as f:
            metric, floor = floor_of(f.read())
        value = measured(results_dirs, name)
        if metric is None:
            # No floor to check -- update_kpis.py only writes one for a
            # benchmark that has produced a result, so this is how a benchmark
            # broken since before the baselines were seeded looks
            # (json_nummultby_num_2, MOD-18654). Reported, never failed: that
            # one is resolved Won't Do, and failing on it would put the job back
            # to red every night, which is the problem this script exists to fix.
            rows.append((name, floor, value, UNGATED))
            continue
        if value is None:
            # Not measured (MOD-18654 errors one benchmark on every run). An
            # absent result is not evidence either way, so carry the previous
            # verdict rather than letting a gap clear a real breach.
            if name in previous:
                breached.add(name)
            rows.append((name, floor, value, MISSING))
        elif value < floor:
            breached.add(name)
            rows.append((name, floor, value, CONFIRMED if name in previous else BREACH))
        else:
            rows.append((name, floor, value, OK))
    return rows, breached


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--benchmarks-dir", default=".")
    parser.add_argument(
        "--results-dir",
        action="append",
        dest="results_dirs",
        help="dir holding this run's result json files; repeatable",
    )
    parser.add_argument("--previous", help="state file written by the previous run")
    parser.add_argument("--state-out", help="where to write this run's state")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    results_dirs = args.results_dirs or ["."]
    previous = set()
    if args.previous and os.path.exists(args.previous):
        with open(args.previous) as f:
            previous = set(json.load(f).get("breached", []))
        print("previous run breached: {}".format(len(previous)))
    else:
        # Nothing to compare against, so no breach can be confirmed this run.
        print("no previous state: breaches will be reported, not failed")

    rows, breached = verdict(args.benchmarks_dir, results_dirs, previous)

    if args.state_out:
        with open(args.state_out, "w") as f:
            json.dump({"breached": sorted(breached)}, f, indent=2)

    for name, floor, value, status in rows:
        if status == OK:
            continue
        # The nightly shards the benchmarks across three jobs, so most of them
        # are simply not this job's to run. update_kpis.py stays silent on a
        # benchmark a run did not cover, and so do we -- except when the
        # previous run had it breaching, where the carried-forward verdict is
        # worth seeing.
        if status == MISSING and name not in previous:
            continue
        if value is None:
            shown = "no result"
        elif floor is None:
            shown = "{:.2f}, no floor".format(value)
        else:
            shown = "{:.2f} vs floor {:.2f}".format(value, floor)
        print("{}: {} ({})".format(status.upper(), name, shown))

    counts = {
        status: sum(1 for row in rows if row[3] == status)
        for status in (OK, BREACH, CONFIRMED, MISSING, UNGATED)
    }
    print(
        "{} benchmark(s): {} ok, {} breached once, {} breached twice, "
        "{} not measured, {} ungated".format(
            len(rows), counts[OK], counts[BREACH], counts[CONFIRMED], counts[MISSING], counts[UNGATED]
        )
    )

    if counts[OK] + counts[BREACH] + counts[CONFIRMED] == 0:
        print("FAIL: no benchmark produced a result -- the run itself is broken")
        return 1
    if counts[CONFIRMED]:
        print("FAIL: {} benchmark(s) below their floor twice in a row".format(counts[CONFIRMED]))
        return 1
    return 0


def self_test():
    import shutil
    import tempfile

    rps = METRICS[0][0]
    tmp = tempfile.mkdtemp()
    try:

        def write(name, floor, value):
            with open(os.path.join(tmp, name + ".yml"), "w") as f:
                f.write('name: "{}"\nkpis:\n  - ge:\n      "{}": {}\n'.format(name, rps, floor))
            if value is not None:
                result = "1-org-repo-master-{}-oss-standalone-sha.json".format(name)
                with open(os.path.join(tmp, result), "w") as f:
                    json.dump({"Tests": {"Overall": {"rps": str(value)}}}, f)

        write("fast", 100.0, 120.0)
        write("slow", 100.0, 90.0)
        write("gone", 100.0, None)

        rows, breached = verdict(tmp, [tmp], set())
        status = {row[0]: row[3] for row in rows}
        assert status == {"fast": OK, "slow": BREACH, "gone": MISSING}, status
        assert breached == {"slow"}, breached

        # Second run: the same breach now fails, and the unmeasured benchmark
        # keeps whatever verdict it had rather than clearing it.
        rows, breached = verdict(tmp, [tmp], {"slow", "gone"})
        status = {row[0]: row[3] for row in rows}
        assert status == {"fast": OK, "slow": CONFIRMED, "gone": MISSING}, status
        assert breached == {"slow", "gone"}, breached

        # A recovered benchmark drops out of the state, so a later breach is
        # again only a first breach.
        write("slow", 100.0, 110.0)
        rows, breached = verdict(tmp, [tmp], {"slow"})
        assert {row[0]: row[3] for row in rows}["slow"] == OK
        assert breached == set(), breached

        # Floors are read whatever the quoting, and a yml without one is not gated.
        with open(os.path.join(tmp, "bare.yml"), "w") as f:
            f.write('name: "bare"\n')
        with open(os.path.join(tmp, "unquoted.yml"), "w") as f:
            f.write('name: "unquoted"\nkpis:\n  - ge:\n        {}: 50\n'.format(rps))
        rows, _ = verdict(tmp, [tmp], set())
        assert ("bare", None, None, UNGATED) in rows, rows
        assert ("unquoted", 50.0, None, MISSING) in rows, rows

        assert main(["--benchmarks-dir", tmp, "--results-dir", tmp]) == 0
        assert main(["--benchmarks-dir", tmp, "--results-dir", os.path.join(tmp, "empty")]) == 1
    finally:
        shutil.rmtree(tmp)

    print("self-test ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
