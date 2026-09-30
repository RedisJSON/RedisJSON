"""Require a slowdown of at least 5% in two measurements of the same workload."""

import argparse
import json
import os
import re
from decimal import Decimal
from pathlib import Path


THRESHOLD = Decimal("5")


def read_comparison(path: Path) -> tuple[tuple[str, str], dict[str, Decimal]]:
    data = json.loads(path.read_text(), parse_float=Decimal, parse_constant=Decimal)
    suites = data["entries"]
    if len(suites) != 1:
        raise ValueError("Expected one benchmark suite")
    entries = next(iter(suites.values()))
    if len(entries) != 2:
        raise ValueError("Expected baseline and candidate measurements")
    commits = (entries[0]["commit"]["id"], entries[1]["commit"]["id"])
    if commits[0] == commits[1]:
        raise ValueError("Baseline and candidate commits must differ")
    measurements = [
        {bench["name"]: bench for bench in entry["benches"]} for entry in entries
    ]
    if any(
        len(values) != len(entry["benches"])
        for values, entry in zip(measurements, entries)
    ):
        raise ValueError("Duplicate benchmark names")
    baseline, candidate = measurements
    if not baseline or baseline.keys() != candidate.keys():
        raise ValueError("Baseline and candidate workloads must match")
    changes = {}
    for name, before in baseline.items():
        after = candidate[name]
        if before["unit"] != after["unit"]:
            raise ValueError(f"Measurement units differ for {name}")
        old, new = Decimal(before["value"]), Decimal(after["value"])
        if not old.is_finite() or not new.is_finite() or old <= 0 or new < 0:
            raise ValueError(f"Invalid timing for {name}")
        changes[name] = (new / old - 1) * 100
    return commits, changes


def regression_filter(changes: dict[str, Decimal]) -> str:
    names = sorted(name for name, change in changes.items() if change >= THRESHOLD)
    return "^(" + "|".join(re.escape(name) for name in names) + ")$" if names else ""


def confirmed_regressions(first: Path, confirmation: Path) -> list[str]:
    commits, initial = read_comparison(first)
    repeated_commits, repeated = read_comparison(confirmation)
    if commits != repeated_commits:
        raise ValueError("Confirmation must compare the same commits")
    suspects = {name for name, change in initial.items() if change >= THRESHOLD}
    if not suspects.issubset(repeated):
        raise ValueError("Confirmation is missing a flagged workload")
    return sorted(name for name in suspects if repeated[name] >= THRESHOLD)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("comparison", type=Path)
    parser.add_argument("--confirmation", type=Path)
    args = parser.parse_args()
    if args.confirmation:
        confirmed = confirmed_regressions(args.comparison, args.confirmation)
        message = (
            "Confirmed slowdown of at least 5%: " + ", ".join(confirmed)
            if confirmed
            else "No slowdown of at least 5% repeated; confirmation passed."
        )
        status = int(bool(confirmed))
    else:
        _, changes = read_comparison(args.comparison)
        pattern = regression_filter(changes)
        if "GITHUB_OUTPUT" in os.environ:
            with open(os.environ["GITHUB_OUTPUT"], "a") as output:
                output.write(f"filter={pattern}\n")
        message = (
            "Rerunning workloads with slowdowns of at least 5%."
            if pattern
            else "No workloads reached the 5% slowdown threshold."
        )
        status = 0
    print(message)
    if "GITHUB_STEP_SUMMARY" in os.environ:
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
            summary.write(f"\n{message}\n")
    return status


if __name__ == "__main__":
    raise SystemExit(main())
