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
telling them apart used to mean grepping the log. This script takes the verdict
over so that each one is named, and so the breaches leave the run as data:

* ``kpi-state/findings.json`` and a table in the run summary list every breach,
  worst shortfall first, for the nightly analysis to file from.
* A benchmark ``run-remote`` failed to run is a finding of its own, read from the
  run log, because with ``run-remote`` continuing on error nothing else would
  fail the job for it. ``--known-broken`` can exempt a benchmark already known
  not to run; nothing uses it now that #1643 fixed ``json_nummultby_num_2``
  (MOD-18654), and it should stay empty unless another one breaks for good.
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
import re
import sys

from update_kpis import METRICS, benchmark_name, result_metric, value_re

OK, BREACH, MISSING, UNGATED, ERRORED, UNREADABLE = (
    "ok",
    "breach",
    "missing",
    "ungated",
    "errored",
    "unreadable",
)

# redisbench_admin.run_remote logs exactly this per benchmark it could not run.
RUN_FAILED_RE = re.compile(r"Failed to run remote benchmark for test '([^']+)'")


def floor_of(text):
    """(metric, floor) declared in this yml's kpis block, or (None, None)."""
    for metric, _ in METRICS:
        match = value_re(metric).search(text)
        if match:
            return metric, float(match.group("value"))
    return None, None


def measured(results_dirs, name):
    """(value, problem) for `name`, from the first results dir that has it.

    `result_metric` returns (None, reason) when a result file exists but cannot
    be read -- ambiguous duplicates, or no known throughput metric in it. That
    reason has to be carried out of here: dropping it leaves the benchmark
    looking unmeasured, which is how a shard says "not mine", and its floor
    would never be applied.
    """
    for results_dir in results_dirs:
        metric, value = result_metric(results_dir, name)
        if metric is not None:
            return value, None
        if value is not None:
            return None, value
    return None, None


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
        value, problem = measured(results_dirs, name)
        # Ungated is checked first: with no floor there is nothing for an
        # unusable result to be measured against, and UNREADABLE carries its
        # floor into the summary table as a number.
        if metric is None:
            # No floor to check. update_kpis.py writes one only from a run that
            # returned a result, so a benchmark that has just started producing
            # results (json_nummultby_num_2, #1643) sits here until a floor is
            # seeded for it -- reported, and not deciding the job.
            rows.append((name, floor, value, UNGATED))
        elif problem is not None:
            rows.append((name, floor, None, UNREADABLE))
        elif value is None:
            rows.append((name, floor, value, MISSING))
        else:
            rows.append((name, floor, value, BREACH if value < floor else OK))
    return rows


def log_failures(run_log, known_broken):
    """(unexpected, known) benchmarks run-remote reported it could not run.

    `run-remote` is given continue-on-error so that this script owns the verdict,
    which means an errored benchmark no longer fails the step by itself. Its
    result file is simply absent, and absent is indistinguishable from "this
    shard was never given that benchmark" -- so the log is the only place that
    says a benchmark was meant to run and did not.
    """
    if not run_log or not os.path.exists(run_log):
        return [], []
    with open(run_log, errors="replace") as f:
        names = set(RUN_FAILED_RE.findall(f.read()))
    return sorted(names - known_broken), sorted(names & known_broken)


def findings(rows, failed_to_run=()):
    """The rows worth a ticket, worst shortfall first, plus the ungated ones."""
    out = [{"benchmark": name, "status": ERRORED, "floor": None, "measured": None} for name in failed_to_run]
    # A benchmark that failed to run also has no result, and if it has no floor
    # either it would come back from `rows` as ungated -- two lines for one
    # benchmark, both asking for a ticket. The errored row is the useful one.
    already = set(failed_to_run)
    for name, floor, value, status in rows:
        if name in already:
            continue
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
        elif status == UNREADABLE:
            out.append({"benchmark": name, "status": status, "floor": floor, "measured": None})
        elif status == UNGATED:
            out.append({"benchmark": name, "status": status, "floor": None, "measured": value})
    out.sort(key=lambda f: -(f.get("shortfall_pct") or 0))
    return out


def step_summary(found):
    """A markdown table for $GITHUB_STEP_SUMMARY, so a breach is visible in the
    run without opening the log.

    An ungated benchmark is listed but is not a finding, so it must not drag the
    file-a-ticket footer onto an otherwise clean run -- on push-to-integ this
    table is the only KPI report anyone sees.
    """
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not path:
        return
    lines = ["## Benchmark KPI floors", ""]
    if not found:
        lines.append("Every gated benchmark met its floor.")
    else:
        lines += ["| benchmark | floor | measured | short | |", "| --- | --- | --- | --- | --- |"]
        for f in found:
            if f["status"] == UNREADABLE:
                lines.append(
                    "| `{}` | {:.0f} | unreadable | | result unusable -- file a ticket |".format(
                        f["benchmark"], f["floor"]
                    )
                )
            elif f["status"] == ERRORED:
                lines.append("| `{}` | | did not run | | errored -- file a ticket |".format(f["benchmark"]))
            elif f["status"] == UNGATED:
                shown = "no result" if f["measured"] is None else "{:.0f}".format(f["measured"])
                lines.append("| `{}` | _none_ | {} | | no floor seeded yet |".format(f["benchmark"], shown))
            else:
                lines.append(
                    "| `{}` | {:.0f} | {:.0f} | -{:.1f}% | breached -- file a ticket |".format(
                        f["benchmark"], f["floor"], f["measured"], f["shortfall_pct"]
                    )
                )
        if any(f["status"] in (BREACH, ERRORED, UNREADABLE) for f in found):
            lines += [
                "",
                "Each row above is worth a ticket: a benchmark under its floor is more than 5% "
                "below its baseline, and one that failed to run or returned an unusable result "
                "was never checked against its floor at all. File during the nightly analysis, "
                "and do not move the floor.",
            ]
        else:
            lines += ["", "Every gated benchmark met its floor."]
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
    parser.add_argument("--run-log", help="run-remote's captured output, read for benchmarks that failed to run")
    parser.add_argument(
        "--run-outcome",
        help="run-remote's step outcome; a non-success nothing in its log explains fails the job",
    )
    parser.add_argument(
        "--known-broken",
        action="append",
        default=[],
        help="benchmark already known not to run, exempt from failing the job; repeatable",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    rows = verdict(args.benchmarks_dir, args.results_dirs or ["."])
    failed_to_run, known_failed = log_failures(args.run_log, set(args.known_broken))
    found = findings(rows, failed_to_run)
    if args.findings_out:
        with open(args.findings_out, "w") as f:
            json.dump(found, f, indent=2)
    step_summary(found)

    for name in failed_to_run:
        print("ERRORED: {} (run-remote could not run it)".format(name))
    for name, floor, value, status in rows:
        if status == OK:
            continue
        if status == BREACH:
            print("BREACH: {} ({:.2f} vs floor {:.2f}, -{:.1f}%)".format(
                name, value, floor, (1.0 - value / floor) * 100))
        elif status == UNREADABLE:
            print("UNREADABLE: {} (result file present but unusable)".format(name))
        elif status == UNGATED:
            print("UNGATED: {} (no floor{})".format(name, "" if value is not None else ", no result"))
        # A benchmark the shards did not give this job is not a finding, and
        # update_kpis.py stays silent on one too.

    counts = {
        status: sum(1 for row in rows if row[3] == status)
        for status in (OK, BREACH, MISSING, UNGATED, UNREADABLE)
    }
    print(
        "{} benchmark(s): {} ok, {} breached, {} unreadable, {} not measured, {} ungated".format(
            len(rows), counts[OK], counts[BREACH], counts[UNREADABLE], counts[MISSING], counts[UNGATED]
        )
    )

    if counts[OK] + counts[BREACH] == 0:
        print("FAIL: no benchmark produced a result -- the run itself is broken")
        return 1
    if counts[UNREADABLE]:
        print("FAIL: {} result(s) could not be read, so their floors were never applied".format(counts[UNREADABLE]))
    if failed_to_run:
        print("FAIL: {} benchmark(s) failed to run: {}".format(len(failed_to_run), ", ".join(failed_to_run)))

    # run-remote runs under continue-on-error, so a timeout or a crash partway
    # through leaves its remaining benchmarks simply absent -- and absent is how
    # a shard says "not mine". If the step did not succeed and nothing in its
    # log accounts for that (no breach, no failed-to-run line), the run ended
    # for a reason we cannot see, and green would be a lie.
    # ponytail: a timeout on a night when a known-broken benchmark also errored
    # still reads as accounted-for. Closing that needs each shard's expected
    # benchmark list, which redisbench-admin does not expose -- neither i % 3
    # nor contiguous thirds of the sorted yml list reproduces the real split.
    unaccounted = (
        args.run_outcome not in (None, "success")
        and not counts[BREACH]
        and not failed_to_run
        and not known_failed
    )
    if unaccounted:
        print(
            "FAIL: run-remote ended as '{}' and nothing in its log explains it -- "
            "treat the results as incomplete".format(args.run_outcome)
        )
    if counts[BREACH]:
        print(
            "FAIL: {} benchmark(s) more than 5% below baseline -- file a ticket for each, "
            "and do not update the floors".format(counts[BREACH])
        )
    return 1 if (counts[BREACH] or failed_to_run or counts[UNREADABLE] or unaccounted) else 0


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
        # No floor at all -- a benchmark awaiting its first seeded floor.
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

        # A benchmark run-remote could not run fails the job on its own, since
        # continue-on-error means nothing else will -- except the ones passed as
        # already known broken (json_nummultby_num_2's case).
        write("slow", 100.0, 110.0)  # back above its floor: isolate the error case
        log = os.path.join(tmp, "run.log")
        with open(log, "w") as f:
            f.write(
                "Failed to run remote benchmark for test 'json_nummultby_num_2'\n"
                "some other line\n"
                "Failed to run remote benchmark for test 'fast'\n"
            )
        assert log_failures(log, {"json_nummultby_num_2"}) == (["fast"], ["json_nummultby_num_2"])
        assert log_failures(log, set()) == (["fast", "json_nummultby_num_2"], [])
        assert log_failures(None, set()) == ([], [])
        assert log_failures(os.path.join(tmp, "nope.log"), set()) == ([], [])
        args = ["--benchmarks-dir", tmp, "--results-dir", tmp, "--run-log", log]
        assert main(args + ["--known-broken", "json_nummultby_num_2", "--known-broken", "fast"]) == 0
        assert main(args + ["--known-broken", "json_nummultby_num_2"]) == 1
        out = os.path.join(tmp, "f2.json")
        main(args + ["--known-broken", "json_nummultby_num_2", "--findings-out", out])
        assert [f["status"] for f in json.load(open(out))][0] == ERRORED

        # A floor-less benchmark whose result cannot be read has nothing to be
        # checked against, so it stays ungated -- and its row must not try to
        # format a floor that does not exist.
        bare_broken = os.path.join(tmp, "1-org-repo-master-bare-oss-standalone-sha.json")
        with open(bare_broken, "w") as f:
            json.dump({"Tests": {"Overall": {"nope": 1}}}, f)
        assert {r[0]: r[3] for r in verdict(tmp, [tmp])}["bare"] == UNGATED
        assert [f for f in findings(verdict(tmp, [tmp])) if f["benchmark"] == "bare"][0]["floor"] is None
        os.environ["GITHUB_STEP_SUMMARY"] = os.path.join(tmp, "summary.md")
        try:
            assert main(["--benchmarks-dir", tmp, "--results-dir", tmp]) == 0
        finally:
            del os.environ["GITHUB_STEP_SUMMARY"]
        os.remove(bare_broken)

        # A result file that exists but carries no known throughput metric must
        # fail, not pass as "this shard did not run it".
        broken = os.path.join(tmp, "1-org-repo-master-gone-oss-standalone-sha.json")
        with open(broken, "w") as f:
            json.dump({"Tests": {"Overall": {"something_else": 1}}}, f)
        assert {r[0]: r[3] for r in verdict(tmp, [tmp])}["gone"] == UNREADABLE
        assert main(["--benchmarks-dir", tmp, "--results-dir", tmp]) == 1
        os.remove(broken)

        # A benchmark that failed to run and has no floor is one row, not two,
        # and the row that survives is the actionable one.
        dual = os.path.join(tmp, "dual.log")
        with open(dual, "w") as f:
            f.write("Failed to run remote benchmark for test 'bare'\n")
        rows = verdict(tmp, [tmp])
        both = findings(rows, ["bare"])
        assert [f["status"] for f in both if f["benchmark"] == "bare"] == [ERRORED], both

        # The footer follows the rows that need a ticket, so an ungated-only run
        # does not tell anyone to file one.
        summary = os.path.join(tmp, "s.md")
        os.environ["GITHUB_STEP_SUMMARY"] = summary
        try:
            step_summary(findings(verdict(tmp, [tmp])))
            text = open(summary).read()
            assert "no floor seeded yet" in text, text
            assert "worth a ticket" not in text, text
            assert "Every gated benchmark met its floor." in text, text
        finally:
            del os.environ["GITHUB_STEP_SUMMARY"]

        # A non-success run-remote is tolerated while its log accounts for it --
        # a breach, an errored benchmark, or a known-broken one -- and fails when
        # nothing does.
        clean = ["--benchmarks-dir", tmp, "--results-dir", tmp]
        assert main(clean + ["--run-outcome", "success"]) == 0
        assert main(clean + ["--run-outcome", "failure"]) == 1
        assert main(clean + ["--run-outcome", "failure", "--run-log", log,
                             "--known-broken", "json_nummultby_num_2", "--known-broken", "fast"]) == 0
    finally:
        shutil.rmtree(tmp)

    print("self-test ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
