#!/usr/bin/env python3
"""Fail the benchmark job on any result below its committed KPI floor.

Each ``tests/benchmarks/<test>.yml`` carries its performance baseline as a
redisbench-admin ``kpis`` floor, set at 95% of a measured value by
``update_kpis.py``::

    kpis:
      - ge:
          "$.Tests.Overall.rps": 123456.0

A result under that floor is a result more than 5% below the baseline, and the
policy for one is: the job goes red, a ticket is filed, and the floor is *not*
moved. ``update_kpis.py`` can only ever raise a floor, so lowering one stays a
hand edit in a reviewed PR.

redisbench-admin already checks the floors at the end of ``run-remote``, but its
exit code answers one question with two meanings -- a floor breach and a
benchmark that failed to run at all both just make the step red, which is why
telling them apart currently means grepping the log. This script takes the
verdict over so that each one is named, and so the breaches leave the run as
data:

* ``kpi-state/findings.json`` and a table in the run summary list every breach,
  worst shortfall first, for the nightly analysis to file from.
* A benchmark that produced no result is reported as such, not as a breach
  (``json_nummultby_num_2`` errors on every run -- MOD-18654, Won't Do -- and has
  no floor, so it is listed as ungated and does not decide the job either way).
* A run where nothing at all measured fails, so swallowing run-remote's exit
  code cannot hide a broken harness.

Nothing here files a ticket by itself, and nothing here touches a baseline.

Usage (from tests/benchmarks, after a run left its result json files there):

    python3 check_kpis.py --findings-out kpi-state/findings.json
    python3 check_kpis.py --self-test

Note on the numbers: the harness's own run-to-run spread is 22-49% for the
benchmarks above ~100k rps (measured across four nightlies on one commit,
MOD-18822), which is wider than the 5% margin the floors sit at, so some of what
this gate reports is the measurement rather than the module. Narrowing that
spread is MOD-18823; the tickets filed from these findings are the record of it.
"""

import argparse
import glob
import json
import os
import sys

from update_kpis import METRICS, benchmark_name, result_metric, value_re

OK, BREACH, MISSING, UNGATED = "ok", "breach", "missing", "ungated"


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


def verdict(benchmarks_dir, results_dirs):
    """[(name, floor, value, status)] for every benchmark with a yml."""
    rows = []
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
            # No floor to check. update_kpis.py only writes one for a benchmark
            # that has produced a result, so this is how a benchmark broken
            # since before the baselines were seeded looks.
            rows.append((name, floor, value, UNGATED))
        elif value is None:
            rows.append((name, floor, value, MISSING))
        else:
            rows.append((name, floor, value, BREACH if value < floor else OK))
    return rows


def findings(rows):
    """The rows worth a ticket, worst shortfall first, plus the ungated ones."""
    out = []
    for name, floor, value, status in rows:
        if status == BREACH:
            out.append(
                {
                    "benchmark": name,
                    "status": status,
                    "floor": floor,
                    "measured": value,
                    "shortfall_pct": round((1.0 - value / floor) * 100, 2),
                }
            )
        elif status == UNGATED:
            out.append({"benchmark": name, "status": status, "floor": None, "measured": value})
    out.sort(key=lambda f: -(f.get("shortfall_pct") or 0))
    return out


def step_summary(found):
    """A markdown table for $GITHUB_STEP_SUMMARY, so a breach is visible in the
    run without opening the log."""
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not path:
        return
    lines = ["## Benchmark KPI floors", ""]
    if not found:
        lines.append("Every gated benchmark met its floor.")
    else:
        lines += ["| benchmark | floor | measured | short | |", "| --- | --- | --- | --- | --- |"]
        for f in found:
            if f["status"] == UNGATED:
                shown = "no result" if f["measured"] is None else "{:.0f}".format(f["measured"])
                lines.append("| `{}` | _none_ | {} | | ungated (MOD-18654) |".format(f["benchmark"], shown))
            else:
                lines.append(
                    "| `{}` | {:.0f} | {:.0f} | -{:.1f}% | breached -- file a ticket |".format(
                        f["benchmark"], f["floor"], f["measured"], f["shortfall_pct"]
                    )
                )
        lines += [
            "",
            "A benchmark under its floor is more than 5% below its baseline: file a ticket "
            "during the nightly analysis, and do not move the floor.",
        ]
    with open(path, "a") as f:
        f.write("\n".join(lines) + "\n")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--benchmarks-dir", default=".")
    parser.add_argument(
        "--results-dir",
        action="append",
        dest="results_dirs",
        help="dir holding this run's result json files; repeatable",
    )
    parser.add_argument(
        "--findings-out",
        help="where to write every breach found, for the nightly analysis to triage",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    rows = verdict(args.benchmarks_dir, args.results_dirs or ["."])
    found = findings(rows)
    if args.findings_out:
        with open(args.findings_out, "w") as f:
            json.dump(found, f, indent=2)
    step_summary(found)

    for name, floor, value, status in rows:
        if status == OK:
            continue
        if status == BREACH:
            print("BREACH: {} ({:.2f} vs floor {:.2f}, -{:.1f}%)".format(
                name, value, floor, (1.0 - value / floor) * 100))
        elif status == UNGATED:
            print("UNGATED: {} (no floor{})".format(name, "" if value is not None else ", no result"))
        # A benchmark the shards did not give this job is not a finding, and
        # update_kpis.py stays silent on one too.

    counts = {status: sum(1 for row in rows if row[3] == status) for status in (OK, BREACH, MISSING, UNGATED)}
    print(
        "{} benchmark(s): {} ok, {} breached, {} not measured, {} ungated".format(
            len(rows), counts[OK], counts[BREACH], counts[MISSING], counts[UNGATED]
        )
    )

    if counts[OK] + counts[BREACH] == 0:
        print("FAIL: no benchmark produced a result -- the run itself is broken")
        return 1
    if counts[BREACH]:
        print(
            "FAIL: {} benchmark(s) more than 5% below baseline -- file a ticket for each, "
            "and do not update the floors".format(counts[BREACH])
        )
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
        # No floor at all -- json_nummultby_num_2's shape.
        with open(os.path.join(tmp, "bare.yml"), "w") as f:
            f.write('name: "bare"\n')
        # Unquoted key and deeper indentation still read as a floor.
        with open(os.path.join(tmp, "deep.yml"), "w") as f:
            f.write('name: "deep"\nkpis:\n  - ge:\n        {}: 50\n'.format(rps))

        status = {row[0]: row[3] for row in verdict(tmp, [tmp])}
        assert status == {"fast": OK, "slow": BREACH, "gone": MISSING, "bare": UNGATED, "deep": MISSING}, status

        # A single breach is a finding, with its shortfall, and the floor-less
        # benchmark rides along without one.
        found = findings(verdict(tmp, [tmp]))
        assert [(f["benchmark"], f["status"]) for f in found] == [("slow", BREACH), ("bare", UNGATED)], found
        assert found[0]["shortfall_pct"] == 10.0, found

        # ... and it fails the job on its own, first time, no second run needed.
        out = os.path.join(tmp, "findings.json")
        assert main(["--benchmarks-dir", tmp, "--results-dir", tmp, "--findings-out", out]) == 1
        assert json.load(open(out))[0]["benchmark"] == "slow"

        # Meeting the floor exactly is not a breach; being a hair under it is.
        write("slow", 100.0, 100.0)
        assert main(["--benchmarks-dir", tmp, "--results-dir", tmp]) == 0
        write("slow", 100.0, 99.99)
        assert main(["--benchmarks-dir", tmp, "--results-dir", tmp]) == 1

        # Nothing measured at all is a broken run, not a clean one.
        assert main(["--benchmarks-dir", tmp, "--results-dir", os.path.join(tmp, "empty")]) == 1
    finally:
        shutil.rmtree(tmp)

    print("self-test ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
