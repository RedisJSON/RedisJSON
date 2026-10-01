"""Require a slowdown of at least 5% in two measurements of the same workload."""

import argparse
import json
import os
import re
from decimal import Decimal
from html import escape
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


def render_summary(comparison: Path, confirmation=None) -> str:
    data = json.loads(comparison.read_text(), parse_float=Decimal, parse_constant=Decimal)
    if len(data["entries"]) != 1:
        raise ValueError("Expected one benchmark suite")
    entries = next(iter(data["entries"].values()))
    if len(entries) not in (1, 2):
        raise ValueError("Expected candidate, optionally preceded by baseline")
    changes = read_comparison(comparison)[1] if len(entries) == 2 else {}
    if confirmation and not changes:
        raise ValueError("Cannot confirm without a baseline")
    confirmed = set(confirmed_regressions(comparison, confirmation)) if confirmation else set()
    repeated = read_comparison(confirmation)[1] if confirmation else {}
    baseline = {b["name"]: b for b in entries[0]["benches"]} if changes else {}
    candidate = entries[-1]["benches"]
    if not candidate or len({b["name"] for b in candidate}) != len(candidate):
        raise ValueError("Expected unique, nonempty candidate workloads")

    def label(text):
        return escape(text).replace("|", "&#124;").replace("`", "&#96;").replace("\n", " ")

    def timing(bench):
        value = Decimal(bench["value"])
        if not value.is_finite() or value < 0:
            raise ValueError("Invalid candidate timing")
        return f'{value:,.2f} {label(bench["unit"])}'

    lines = ["## JSONPath benchmark results", "", f"Measured **{len(candidate)} workloads**.", ""]
    if changes:
        suspects = sum(change >= THRESHOLD for change in changes.values())
        lines += [
            f"**{len(confirmed)} confirmed regressions**, {suspects} initially flagged. "
            "CI fails only when the same workload is at least 5% slower in both runs.",
            "", "### Largest changes", "",
            "Green = faster; red = slower. Each square represents 5 percentage points, capped at 100%.",
            "", "| Workload | Time change | Visual |", "| --- | ---: | --- |",
        ]
        for name in sorted(changes, key=lambda n: abs(changes[n]), reverse=True)[:8]:
            change = changes[name]
            bar = ("🟥" if change > 0 else "🟩") * min(20, int(abs(change) / THRESHOLD)) or "·"
            lines.append(f"| `{label(name)}` | {change:+.2f}% | {bar} |")
    else:
        lines += ["No compatible baseline: measurements only; no regression comparison yet."]
    lines += [
        "", f"<details><summary>All {len(candidate)} workloads</summary>", "",
        "| Workload | Baseline | Candidate | Time change | Confirmation | Status |",
        "| --- | ---: | ---: | ---: | ---: | --- |",
    ]
    for bench in sorted(candidate, key=lambda b: b["name"]):
        name = bench["name"]
        change = changes.get(name)
        if change is None:
            status = "ℹ️ Baseline only"
        elif name in confirmed:
            status = "❌ Confirmed ≥5%"
        elif change >= THRESHOLD:
            status = "✅ Not reproduced" if confirmation else "⚠️ Awaiting confirmation"
        elif change <= -THRESHOLD:
            status = "🟢 Faster"
        else:
            status = "⚪ Within 5%"
        before = timing(baseline[name]) if name in baseline else "—"
        delta = f"{change:+.2f}%" if change is not None else "—"
        repeat = f"{repeated[name]:+.2f}%" if name in repeated else "—"
        lines.append(f"| `{label(name)}` | {before} | {timing(bench)} | {delta} | {repeat} | {status} |")
    lines += ["", "</details>", "", "Criterion HTML reports and raw measurements are available in the job artifact."]
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("comparison", type=Path)
    parser.add_argument("--confirmation", type=Path)
    parser.add_argument("--summary", action="store_true", help="Render a job summary without applying the gate")
    args = parser.parse_args()
    if args.summary:
        message = render_summary(args.comparison, args.confirmation)
        status = 0
    elif args.confirmation:
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
