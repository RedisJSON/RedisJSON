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
splits the two things that were conflated in one exit code:

* **Every** benchmark that comes in under its floor is a finding, whatever the
  shortfall -- the floor is already 5% below a measured baseline, so being below
  it at all is worth a ticket. Findings go to the step summary and to
  ``kpi-state/findings.json``, which the nightly analysis reads to decide what
  to file.
* Only a breach the previous run saw too fails the job. No benchmark breached on
  consecutive nights across that four-night sample, so this keeps job status
  meaning "something is wrong" instead of "it was Tuesday", while a genuine
  slowdown -- which does not go away overnight -- still fails, one night later
  than it otherwise would.

A first-time breach is therefore never silently passed; it is passed *and
recorded*. Nothing here files a ticket by itself.

This is a stopgap for the gate, not a fix for the measurement: a regression
smaller than the harness's own spread stays invisible either way. Narrowing
that spread is MOD-18823; once it reports a residual figure, the flat margin
should become per-benchmark and this repeat requirement can be revisited.

Usage (from tests/benchmarks, after a run left its result json files there):

    python3 check_kpis.py --previous prev.json --state-out state.json \
        --findings-out findings.json
    python3 check_kpis.py --self-test

Exit status is 0 when nothing breached twice in a row -- first-time breaches are
still listed and still written to the findings file -- and 1 on a confirmed
breach, or when the run produced no results at all.
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


def findings(rows):
    """The rows the nightly analysis has to look at, worst shortfall first."""
    out = []
    for name, floor, value, status in rows:
        if status in (BREACH, CONFIRMED):
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
    run without opening the log. Every breach is ticket-worthy; which ones get
    filed is decided during the nightly analysis."""
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not path:
        return
    lines = ["## Benchmark KPI floors", ""]
    if not found:
        lines.append("Every gated benchmark met its floor.")
    else:
        lines += [
            "| benchmark | floor | measured | short | |",
            "| --- | --- | --- | --- | --- |",
        ]
        for f in found:
            if f["status"] == UNGATED:
                lines.append(
                    "| `{}` | _none_ | {} | | ungated (MOD-18654) |".format(
                        f["benchmark"], "no result" if f["measured"] is None else "{:.0f}".format(f["measured"])
                    )
                )
            else:
                lines.append(
                    "| `{}` | {:.0f} | {:.0f} | -{:.1f}% | {} |".format(
                        f["benchmark"],
                        f["floor"],
                        f["measured"],
                        f["shortfall_pct"],
                        "**breached twice -- fails the job**" if f["status"] == CONFIRMED else "breached once",
                    )
                )
        lines += [
            "",
            "A benchmark under its floor is under a baseline already set 5% low, so each row "
            "above is worth a ticket -- file during the nightly analysis. The job only fails "
            "on a breach the previous run saw too (MOD-18822).",
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
    parser.add_argument("--previous", help="state file written by the previous run")
    parser.add_argument("--state-out", help="where to write this run's state")
    parser.add_argument(
        "--findings-out",
        help="where to write every breach found, for the nightly analysis to triage",
    )
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

    found = findings(rows)
    if args.findings_out:
        with open(args.findings_out, "w") as f:
            json.dump(found, f, indent=2)
    step_summary(found)

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
    if counts[BREACH] or counts[CONFIRMED]:
        # Said once, plainly, because the exit code no longer says it: a breach
        # is a breach whether or not it repeats, and the nightly analysis is
        # where it turns into a ticket.
        print(
            "{} benchmark(s) below their floor -- each one is worth a ticket; "
            "file during the nightly analysis".format(counts[BREACH] + counts[CONFIRMED])
        )
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

        rows, _ = verdict(tmp, [tmp], {"fast"})
        found = findings(rows)
        # Every breach is listed whether or not it repeated, worst first, and a
        # benchmark with no floor is listed without one rather than dropped.
        # "unquoted" has a floor but no result, so it is not measured rather
        # than ungated, and only a benchmark with no floor at all is listed as
        # ungated ("bare", standing in for json_nummultby_num_2).
        assert [(f["benchmark"], f["status"]) for f in found] == [("bare", UNGATED)], found
        write("slow", 100.0, 60.0)
        rows, _ = verdict(tmp, [tmp], {"slow"})
        found = findings(rows)
        assert found[0]["benchmark"] == "slow" and found[0]["status"] == CONFIRMED, found
        assert found[0]["shortfall_pct"] == 40.0, found

        out = os.path.join(tmp, "findings.json")
        assert main(["--benchmarks-dir", tmp, "--results-dir", tmp, "--findings-out", out]) == 0
        assert json.load(open(out))[0]["benchmark"] == "slow"
        write("slow", 100.0, 110.0)
        assert main(["--benchmarks-dir", tmp, "--results-dir", tmp]) == 0
        assert main(["--benchmarks-dir", tmp, "--results-dir", os.path.join(tmp, "empty")]) == 1
    finally:
        shutil.rmtree(tmp)

    print("self-test ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
