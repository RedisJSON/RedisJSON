#!/usr/bin/env python3
"""Raise the per-benchmark KPI floors from the results of a benchmark run.

Each ``tests/benchmarks/<test>.yml`` carries its performance baseline as a
redisbench-admin ``kpis`` block::

    kpis:
      - ge:
          "$.Tests.Overall.rps": 123456.0

redisbench-admin checks those floors itself at the end of every run
(``results_dict_kpi_check`` -> non-zero exit), so a run below the floor fails
the job without any extra comparison step.

This script only ever *raises* a floor. A run slower than its baseline leaves
the yml untouched -- it has already failed -- so the baseline cannot drift
downwards on its own. Lowering one takes a hand edit in a reviewed PR.

Usage (from tests/benchmarks, after a run left its result json files there):

    python3 update_kpis.py                 # raise floors, 5% margin
    python3 update_kpis.py --margin 0.10
    python3 update_kpis.py --self-test
"""

import argparse
import glob
import json
import os
import re
import sys

# Throughput metric per benchmark tool, in the order we probe for it. Which one
# a result file carries depends on its `clientconfig.tool`: redis-benchmark
# results are keyed under Tests.Overall, memtier_benchmark under "ALL STATS".
# Both paths are among the comparison metrics in defaults.yml.
METRICS = [
    ("$.Tests.Overall.rps", ("Tests", "Overall", "rps")),
    ('$."ALL STATS".Totals."Ops/sec"', ("ALL STATS", "Totals", "Ops/sec")),
]
BLOCK = "kpis:\n  - ge:\n      {key}: {value}\n"


def yaml_key(metric):
    """Quote a jsonpath for use as a YAML mapping key."""
    return "'{}'".format(metric) if '"' in metric else '"{}"'.format(metric)


def value_re(metric):
    """Matches this metric's line inside an existing kpis block, quoted or not."""
    return re.compile(
        r"^(?P<indent>[ \t]*)(?P<key>[\"']?"
        + re.escape(metric)
        + r"[\"']?)[ \t]*:[ \t]*(?P<value>[0-9][0-9.eE+-]*)[ \t]*$",
        re.MULTILINE,
    )


def benchmark_name(path):
    with open(path) as f:
        for line in f:
            if line.startswith("name:"):
                return line.split(":", 1)[1].strip().strip("\"'")
    return None


def result_metric(results_dir, name):
    """(metric, value) measured for `name`.

    Returns (None, None) when the run did not cover this benchmark, and
    (None, reason) when it produced a result we cannot read a throughput from.

    Result files are named
    <start>-<org>-<repo>-<branch>-<test_name>-<deployment>-<sha>.json
    (redisbench_admin.utils.remote.get_run_full_filename).
    """
    # The filename joins branch and test_name with the same '-' separator, so
    # anchoring on the test name alone lets a branch that happens to contain a
    # benchmark name bind the wrong result. The deployment type always follows
    # the test name and always starts "oss-" (oss-standalone, oss-cluster), so
    # require that too.
    #
    # Many benchmark names also contain [0] / [web-app], which glob reads as
    # character classes -- escape them.
    pattern = "*-{}-oss-*.json".format(glob.escape(name))
    matches = glob.glob(os.path.join(results_dir, pattern))
    if not matches:
        return None, None
    if len(matches) > 1:
        return None, "ambiguous result files {}".format(matches)
    with open(matches[0]) as f:
        results = json.load(f)
    for metric, path in METRICS:
        node = results
        for key in path:
            if not isinstance(node, dict) or key not in node:
                node = None
                break
            node = node[key]
        if node is not None:
            return metric, float(node)
    return None, "no known throughput metric in {}".format(os.path.basename(matches[0]))


def raise_floor(text, metric, floor):
    """Return (new_text, applied). Rewrites or appends this metric's kpis floor."""
    match = value_re(metric).search(text)
    if match is None:
        if "kpis:" in text:
            raise SystemExit("kpis block present but has no {} floor".format(metric))
        sep = "" if text.endswith("\n") else "\n"
        block = BLOCK.format(key=yaml_key(metric), value=floor)
        return text + sep + "\n" + block, True
    if floor <= float(match.group("value")):
        return text, False
    line = "{}{}: {}".format(match.group("indent"), match.group("key"), floor)
    return text[: match.start()] + line + text[match.end() :], True


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--margin",
        type=float,
        default=0.05,
        help="fraction below the measured value to place the floor at",
    )
    parser.add_argument("--results-dir", default=".")
    parser.add_argument("--benchmarks-dir", default=".")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    updated = []
    unreadable = []
    for path in sorted(glob.glob(os.path.join(args.benchmarks_dir, "*.yml"))):
        if os.path.basename(path) == "defaults.yml":
            continue
        name = benchmark_name(path)
        if name is None:
            continue
        metric, value = result_metric(args.results_dir, name)
        if metric is None:
            # A benchmark this run did not cover is silent; one that produced an
            # unreadable result is reported, so a gap is never mistaken for a
            # baseline that is simply already high enough.
            if value is not None:
                unreadable.append(name)
                print("SKIPPED {}: {}".format(name, value))
            continue
        floor = round(value * (1.0 - args.margin), 2)
        with open(path) as f:
            text = f.read()
        new_text, applied = raise_floor(text, metric, floor)
        if applied:
            with open(path, "w") as f:
                f.write(new_text)
            updated.append(path)
            print("raised {} floor to {} ({} measured)".format(name, floor, value))
        else:
            print("kept {} floor ({} measured)".format(name, value))

    print("{} baseline(s) raised".format(len(updated)))
    if unreadable:
        print("{} benchmark(s) skipped: {}".format(len(unreadable), ", ".join(unreadable)))
        return 1
    return 0


def self_test():
    rps, ops = METRICS[0][0], METRICS[1][0]

    text = 'version: 0.2\nname: "t"\n'
    text, applied = raise_floor(text, rps, 100.0)
    assert applied and '"$.Tests.Overall.rps": 100.0' in text, text

    text, applied = raise_floor(text, rps, 150.0)
    assert applied and "150.0" in text, text

    # A slower run must not lower the floor.
    text, applied = raise_floor(text, rps, 120.0)
    assert not applied and "150.0" in text, text

    # Unquoted keys and deeper indentation are recognised, not duplicated.
    other = "kpis:\n  - ge:\n        $.Tests.Overall.rps: 10\n"
    other, applied = raise_floor(other, rps, 20.0)
    assert applied and other.count("$.Tests.Overall.rps") == 1, other
    assert other == "kpis:\n  - ge:\n        $.Tests.Overall.rps: 20.0\n", other

    # The memtier metric contains double quotes, so its YAML key is
    # single-quoted, and it must round-trip like the redis-benchmark one.
    memtier = 'name: "t"\n'
    memtier, applied = raise_floor(memtier, ops, 149.02)
    assert applied and "'$.\"ALL STATS\".Totals.\"Ops/sec\"': 149.02" in memtier, memtier
    memtier, applied = raise_floor(memtier, ops, 100.0)
    assert not applied and memtier.count("Ops/sec") == 1, memtier

    print("self-test ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
