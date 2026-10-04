"""Require a slowdown of at least 5% in two measurements of the same workload."""

import argparse
import json
import os
import re
from decimal import Decimal
from html import escape
from pathlib import Path


THRESHOLD = Decimal("5")


def shared_workload_filter(baseline: Path, candidate: Path) -> str:
    """Select names registered in both revisions' Criterion --list output."""
    workloads = []
    for path in (baseline, candidate):
        lines = [line for line in path.read_text().splitlines() if line]
        suffix = ": benchmark"
        if not lines or any(not line.endswith(suffix) for line in lines):
            raise ValueError(f"Invalid Criterion workload list: {path}")
        names = {line[:-len(suffix)] for line in lines}
        if "" in names or len(names) != len(lines):
            raise ValueError(f"Expected unique, nonempty benchmark names: {path}")
        workloads.append(names)
    shared = workloads[0] & workloads[1]
    if not shared:
        raise ValueError("No shared workloads to compare")
    return "^(" + "|".join(re.escape(name) for name in sorted(shared)) + ")$"


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
    if not baseline or not candidate:
        raise ValueError("Baseline and candidate workloads must be nonempty")
    for values in measurements:
        for name, bench in values.items():
            value = Decimal(bench["value"])
            if not value.is_finite() or value < 0 or (values is baseline and value == 0):
                raise ValueError(f"Invalid timing for {name}")
    changes = {}
    for name in baseline.keys() & candidate.keys():
        before = baseline[name]
        after = candidate[name]
        if before["unit"] != after["unit"]:
            raise ValueError(f"Measurement units differ for {name}")
        old, new = Decimal(before["value"]), Decimal(after["value"])
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


def render_summary(comparison: Path, confirmation=None, paths_file=None) -> str:
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
    initial = changes
    changes = {name: repeated.get(name, change) for name, change in initial.items()}
    baseline = {b["name"]: b for b in entries[0]["benches"]} if changes else {}
    candidate = entries[-1]["benches"]
    if confirmation:
        fresh = next(iter(json.loads(confirmation.read_text(), parse_float=Decimal)["entries"].values()))
        baseline.update({b["name"]: b for b in fresh[0]["benches"]})
        fresh_candidate = {b["name"]: b for b in fresh[1]["benches"]}
        candidate = [fresh_candidate.get(b["name"], b) for b in candidate]
    if not candidate or len({b["name"] for b in candidate}) != len(candidate):
        raise ValueError("Expected unique, nonempty candidate workloads")
    paths = {}
    if paths_file is not None:
        for line in paths_file.read_text().splitlines():
            entry = json.loads(line)
            name, path = entry["name"], entry["path"]
            if not isinstance(name, str) or not isinstance(path, str) or name in paths:
                raise ValueError("Expected unique benchmark names and JSONPath strings")
            paths[name] = path
        if any(bench["name"] not in paths for bench in candidate):
            raise ValueError("Missing JSONPath metadata for a candidate workload")

    def label(text):
        return escape(text).replace("|", "&#124;").replace("`", "&#96;").replace("\n", " ")

    def timing(bench):
        value = Decimal(bench["value"])
        if not value.is_finite() or value < 0:
            raise ValueError("Invalid candidate timing")
        unit = bench["unit"]
        if unit == "ns/iter":
            for scale, scaled_unit in [(10**9, "s/iter"), (10**6, "ms/iter"), (10**3, "µs/iter")]:
                if value >= scale:
                    value /= scale
                    unit = scaled_unit
                    break
        return f'{value:,.2f} {label(unit)}'

    def path_cell(name):
        return f" <code>{label(paths[name])}</code> |" if paths_file is not None else ""

    path_header = " JSONPath |" if paths_file is not None else ""
    path_separator = " --- |" if paths_file is not None else ""

    lines = ["## JSONPath benchmark results", "", f"Measured **{len(candidate)} workloads**.", ""]
    if changes:
        suspects = sum(change >= THRESHOLD for change in initial.values())
        lines += [
            f"Compared **{len(changes)}** workloads against master measured on this runner; "
            f"**{len(candidate) - len(changes)}** without a master baseline.",
            "",
            f"**{len(confirmed)} confirmed regressions**, {suspects} initially flagged. "
            "CI fails only when a flagged workload remains at least 5% slower "
            "against freshly measured master on the same runner.",
            "",
            "All comparisons use measurements from this job. Timings and chart use "
            "confirmation results for remeasured workloads; First pass retains the original delta.",
            "", "### Largest changes", "",
            "Green = faster; red = slower. Each square represents 5 percentage points, capped at 100%.",
            "", f"| Workload |{path_header} Time change | Visual |",
            f"| --- |{path_separator} ---: | --- |",
        ]
        for name in sorted(changes, key=lambda n: abs(changes[n]), reverse=True)[:8]:
            change = changes[name]
            bar = ("🟥" if change > 0 else "🟩") * min(20, int(abs(change) / THRESHOLD)) or "·"
            lines.append(f"| `{label(name)}` |{path_cell(name)} {change:+.2f}% | {bar} |")
    else:
        lines += ["No matching master baseline: measurements only; no regression comparison yet."]
    lines += [
        "", f"<details><summary>All {len(candidate)} workloads</summary>", "",
        f"| Workload |{path_header} Baseline | Candidate | Time change | First pass | Status |",
        f"| --- |{path_separator} ---: | ---: | ---: | ---: | --- |",
    ]
    for bench in sorted(candidate, key=lambda b: b["name"]):
        name = bench["name"]
        change = changes.get(name)
        if change is None:
            status = "ℹ️ No master baseline"
        elif name in confirmed:
            status = "❌ Confirmed ≥5%"
        elif initial[name] >= THRESHOLD:
            status = "✅ Not reproduced" if confirmation else "⚠️ Awaiting confirmation"
        elif change <= -THRESHOLD:
            status = "🟢 Faster"
        else:
            status = "⚪ Within 5%"
        before = timing(baseline[name]) if name in baseline else "—"
        delta = f"{change:+.2f}%" if change is not None else "—"
        first = f"{initial[name]:+.2f}%" if name in repeated else "—"
        lines.append(f"| `{label(name)}` |{path_cell(name)} {before} | {timing(bench)} | {delta} | {first} | {status} |")
    lines += ["", "</details>", "", "Criterion HTML reports and raw measurements are available in the job artifact."]
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("comparison", type=Path)
    parser.add_argument("--confirmation", type=Path)
    parser.add_argument("--summary", action="store_true", help="Render a job summary without applying the gate")
    parser.add_argument("--paths-file", type=Path, help="Benchmark JSONPath metadata in JSON Lines format")
    args = parser.parse_args()
    if args.summary:
        message = render_summary(args.comparison, args.confirmation, args.paths_file)
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
            else "No compared workloads reached the 5% slowdown threshold."
            if changes
            else "No matching master workloads; measurements only."
        )
        status = 0
    print(message)
    if "GITHUB_STEP_SUMMARY" in os.environ:
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
            summary.write(f"\n{message}\n")
    return status


if __name__ == "__main__":
    raise SystemExit(main())
