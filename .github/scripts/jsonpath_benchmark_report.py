"""Report master-versus-candidate counts and Gungraun's regression verdicts."""

import argparse
import json
import os
import re
from decimal import Decimal
from html import escape
from pathlib import Path

METRICS = ("Ir", "Dr", "Dw", "EstimatedCycles")
PHASES = {"compile_path": "compile", "eval_path": "eval"}


def read_results(path):
    results = {}
    for line in path.read_text().splitlines():
        if not line.strip():
            continue
        try:
            row = json.loads(line)
            if (row["version"] != "7" or row["kind"] != "LibraryBenchmark"
                    or row["group"] != "jsonpath"
                    or not re.fullmatch(r"[a-z][a-z0-9_]*", row["id"])):
                raise ValueError("Unexpected Gungraun benchmark schema")
            name = f'{PHASES[row["function_name"]]}/{row["id"].replace("_", "-")}'
            profiles = row["profiles"]
            if len(profiles) != 1 or profiles[0]["tool"] != "Callgrind":
                raise ValueError(f"Expected one Callgrind profile for {name}")
            total = profiles[0]["data"]["total"]
            metrics = total["metrics"]
            values = {key: metrics[key]["values"]["new"] for key in METRICS}
            if any(type(value) is not int or value < 0 for value in values.values()):
                raise ValueError(f"Invalid event counts for {name}")
            if values["Ir"] == 0:
                raise ValueError(f"No instructions collected for {name}")
            previous = {key: metrics[key]["values"].get("old") for key in METRICS}
            if any(value is not None and (type(value) is not int or value < 0)
                   for value in previous.values()):
                raise ValueError(f"Invalid baseline counts for {name}")
            if not isinstance(total["regressions"], list):
                raise ValueError(f"Invalid Gungraun regression verdict for {name}")
        except (KeyError, TypeError, IndexError) as error:
            raise ValueError(f"Incomplete or invalid Gungraun result in {path}") from error
        if name in results:
            raise ValueError(f"Duplicate benchmark: {name}")
        results[name] = {
            "counts": values, "baseline": previous,
            "regressed": bool(total["regressions"]),
        }
    if not results:
        raise ValueError("No Gungraun measurements found")
    return results


def read_paths(path, names):
    paths = {}
    for line in path.read_text().splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        name, expression = row.get("name"), row.get("path")
        if not isinstance(name, str) or not isinstance(expression, str) or not expression:
            raise ValueError("Invalid benchmark path metadata")
        if name in paths:
            raise ValueError(f"Duplicate path metadata: {name}")
        paths[name] = expression
    if paths.keys() != names:
        raise ValueError("Path metadata and measured workloads must match exactly")
    return paths


def code_cell(text):
    text = escape(text).replace("|", "&#124;").replace("`", "&#96;")
    return "<code>" + text.replace("\r", " ").replace("\n", " ") + "</code>"


def render_summary(results_file, paths_file, baseline_file, baseline_paths_file):
    results = read_results(results_file)
    baseline = read_results(baseline_file)
    if results.keys() != baseline.keys():
        raise ValueError("Master and candidate workloads must match exactly")
    paths = read_paths(paths_file, results.keys())
    if paths != read_paths(baseline_paths_file, baseline.keys()):
        raise ValueError("Master and candidate JSONPaths must match exactly")
    changes = {}
    for name, result in results.items():
        if result["baseline"] != baseline[name]["counts"]:
            raise ValueError(f"Gungraun did not compare {name} against this master run")
        old, new = result["baseline"]["Ir"], result["counts"]["Ir"]
        changes[name] = Decimal(new - old) * 100 / Decimal(old)
    compiles = sum(name.startswith("compile/") for name in results)
    regressions = sum(result["regressed"] for result in results.values())
    lines = [
        "## JSONPath master vs candidate (Gungraun / Callgrind)",
        "",
        f"Compared **{len(results)} workloads**: {compiles} compilation, "
        f"{len(results) - compiles} evaluation.",
        "",
        f"**Regressions: {regressions}** (Gungraun verdict). "
        "Both revisions were measured in this job with the same harness and toolchain.",
        "The CI gate fails on instruction-count increases over 5%.",
        "Instructions and data reads/writes are event counts; estimated cycles are "
        "simulated costs, not elapsed time or hardware CPU cycles.",
        "",
    ]
    largest = sorted((name for name in results if changes[name]),
                     key=lambda name: (-abs(changes[name]), name))[:8]
    if largest:
        lines += [
            "### Largest instruction-count changes", "",
            "Each block is approximately 1%; bars are capped at 20 blocks.", "",
            "| Workload | JSONPath | Change | |", "| --- | --- | ---: | --- |",
        ]
        for name in largest:
            change = changes[name]
            color = "🟥" if results[name]["regressed"] else "🟩" if change < 0 else "🟨"
            bar = color * min(20, round(abs(change))) or "·"
            lines.append(f"| {code_cell(name)} | {code_cell(paths[name])} | {change:+.2f}% | {bar} |")
    else:
        lines += ["No instruction-count changes."]
    lines += [
        "", "<details>", f"<summary>All {len(results)} workloads</summary>", "",
        "| Workload | JSONPath | Master Ir | Candidate Ir | Change | Status |",
        "| --- | --- | ---: | ---: | ---: | --- |",
    ]
    for name, result in sorted(results.items()):
        change = changes[name]
        status = ("🔴 Regression" if result["regressed"] else
                  "🟢 Fewer instructions" if change < 0 else "Within limit")
        lines.append(
            f"| {code_cell(name)} | {code_cell(paths[name])} | "
            f'{result["baseline"]["Ir"]:,} | {result["counts"]["Ir"]:,} | '
            f"{change:+.2f}% | {status} |"
        )
    lines += [
        "", "</details>", "", "<details>",
        "<summary>Data accesses and simulated costs (master → candidate)</summary>", "",
        "| Workload | Reads (Dr) | Writes (Dw) | Estimated cycles |",
        "| --- | ---: | ---: | ---: |",
    ]
    for name, result in sorted(results.items()):
        counts = " | ".join(
            f'{result["baseline"][key]:,} → {result["counts"][key]:,}' for key in METRICS[1:]
        )
        lines.append(f"| {code_cell(name)} | {counts} |")
    lines += [
        "", "</details>", "",
        "Raw Gungraun JSON, cache metrics, Callgrind profiles, and tool versions "
        "are available in the job artifact.", "",
    ]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path)
    parser.add_argument("--paths-file", type=Path, required=True)
    parser.add_argument("--baseline-file", type=Path, required=True)
    parser.add_argument("--baseline-paths-file", type=Path, required=True)
    args = parser.parse_args()
    summary = render_summary(args.results, args.paths_file, args.baseline_file, args.baseline_paths_file)
    print(summary, end="")
    if "GITHUB_STEP_SUMMARY" in os.environ:
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as output:
            output.write(summary)


if __name__ == "__main__":
    main()
