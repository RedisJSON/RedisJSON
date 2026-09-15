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

METRIC = "$.Tests.Overall.rps"
BLOCK = 'kpis:\n  - ge:\n      "{metric}": {value}\n'
# Matches the metric line inside an existing kpis block, quoted or not.
VALUE_RE = re.compile(
    r'^(?P<indent>[ \t]*)(?P<key>["\']?\$\.Tests\.Overall\.rps["\']?)'
    r"[ \t]*:[ \t]*(?P<value>[0-9][0-9.eE+-]*)[ \t]*$",
    re.MULTILINE,
)


def benchmark_name(path):
    with open(path) as f:
        for line in f:
            if line.startswith("name:"):
                return line.split(":", 1)[1].strip().strip("\"'")
    return None


def result_value(results_dir, name):
    """rps of this run for `name`, or None when the run did not cover it.

    Result files are named
    <start>-<org>-<repo>-<branch>-<test_name>-<deployment>-<sha>.json
    (redisbench_admin.utils.remote.get_run_full_filename).
    """
    # Many benchmark names contain [0] / [web-app], which glob reads as
    # character classes -- escape them.
    pattern = "*-{}-*.json".format(glob.escape(name))
    matches = glob.glob(os.path.join(results_dir, pattern))
    if not matches:
        return None
    if len(matches) > 1:
        raise SystemExit("{}: ambiguous result files {}".format(name, matches))
    with open(matches[0]) as f:
        tests = json.load(f).get("Tests", {})
    if "Overall" not in tests or "rps" not in tests["Overall"]:
        raise SystemExit("{}: no Tests.Overall.rps in {}".format(name, matches[0]))
    return float(tests["Overall"]["rps"])


def raise_floor(text, floor):
    """Return (new_text, applied). Rewrites or appends the kpis floor."""
    match = VALUE_RE.search(text)
    if match is None:
        if "kpis:" in text:
            raise SystemExit("kpis block present but has no {} floor".format(METRIC))
        sep = "" if text.endswith("\n") else "\n"
        return text + sep + "\n" + BLOCK.format(metric=METRIC, value=floor), True
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
    for path in sorted(glob.glob(os.path.join(args.benchmarks_dir, "*.yml"))):
        if os.path.basename(path) == "defaults.yml":
            continue
        name = benchmark_name(path)
        if name is None:
            continue
        value = result_value(args.results_dir, name)
        if value is None:
            continue
        floor = round(value * (1.0 - args.margin), 2)
        with open(path) as f:
            text = f.read()
        new_text, applied = raise_floor(text, floor)
        if applied:
            with open(path, "w") as f:
                f.write(new_text)
            updated.append(path)
            print("raised {} floor to {} ({} measured)".format(name, floor, value))
        else:
            print("kept {} floor ({} measured)".format(name, value))

    print("{} baseline(s) raised".format(len(updated)))
    return 0


def self_test():
    text = 'version: 0.2\nname: "t"\n'
    text, applied = raise_floor(text, 100.0)
    assert applied and '"$.Tests.Overall.rps": 100.0' in text, text

    text, applied = raise_floor(text, 150.0)
    assert applied and "150.0" in text, text

    # A slower run must not lower the floor.
    text, applied = raise_floor(text, 120.0)
    assert not applied and "150.0" in text, text

    # Unquoted keys and deeper indentation are recognised, not duplicated.
    other = "kpis:\n  - ge:\n        $.Tests.Overall.rps: 10\n"
    other, applied = raise_floor(other, 20.0)
    assert applied and other.count("$.Tests.Overall.rps") == 1, other
    assert other == "kpis:\n  - ge:\n        $.Tests.Overall.rps: 20.0\n", other

    print("self-test ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
