"""Measure native runner drift without changing the JSONPath regression gate."""

import argparse
import hashlib
import json
import math
import os
import re
import subprocess
import time
from pathlib import Path


WORKLOADS = ("eval/simple", "eval/deep-field", "eval/projection-function", "eval/filter-and")
ORDER = ("reference", "candidate", "candidate", "reference") * 2


def analyze(runs):
    """Keep candidate/reference ratios directional; compare successive runs of each binary."""
    if tuple(run["revision"] for run in runs) != ORDER:
        raise ValueError("Expected two complete reference/candidate/candidate/reference blocks")
    names = set(runs[0]["medians_ns"])
    if not names or any(set(run["medians_ns"]) != names for run in runs):
        raise ValueError("Diagnostic runs must contain the same workloads")
    if any(not math.isfinite(value) or value <= 0
           for run in runs for value in run["medians_ns"].values()):
        raise ValueError("Diagnostic timings must be finite and positive")
    results = {}
    for name in sorted(names):
        values = [run["medians_ns"][name] for run in runs]

        def change(before, after):
            return (values[after] / values[before] - 1) * 100

        results[name] = {
            "reference_first": [change(0, 1), change(4, 5)],
            "candidate_first": [change(3, 2), change(7, 6)],
            "reference_repeat": [change(0, 3), change(3, 4), change(4, 7)],
            "candidate_repeat": [change(1, 2), change(2, 5), change(5, 6)],
        }
    return results


def executable(build_log):
    artifacts = [json.loads(line) for line in build_log.read_text().splitlines()]
    paths = [Path(item["executable"]).resolve(strict=True) for item in artifacts
             if item.get("reason") == "compiler-artifact"
             and item["target"]["name"] == "path_performance" and item.get("executable")]
    if len(paths) != 1:
        raise ValueError(f"Expected one benchmark executable in {build_log}")
    return paths[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference_build", type=Path)
    parser.add_argument("candidate_build", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--reference-name", default="master")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    binaries = {"reference": executable(args.reference_build), "candidate": executable(args.candidate_build)}
    manifest = {role: {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                for role, path in binaries.items()}
    manifest["reference"]["name"] = args.reference_name
    (output / "binaries.json").write_text(json.dumps(manifest, indent=2))
    pattern = "^(" + "|".join(re.escape(name) for name in WORKLOADS) + ")$"
    runs = []
    summary = ["## Native JSONPath timing diagnostics", "",
               f"Reference: **{args.reference_name}**; candidate: current PR. "
               "Diagnostic only; the existing 5% confirmation gate is unchanged. "
               "Each sample setting runs reference → PR → PR → reference twice, with "
               "3 s warmup, 5 s measurement and 100,000 resamples. "
               "Positive candidate/reference changes mean slower; repeat columns compare "
               "successive invocations of the exact same binary.", ""]
    for samples in (100, 500):
        block = []
        for position, role in enumerate(ORDER, 1):
            folder = output / str(samples) / f"{position}-{role}"
            folder.mkdir(parents=True)
            command = [str(binaries[role]), "--bench", pattern, "--save-baseline", "diagnostic",
                       "--warm-up-time", "3", "--measurement-time", "5", "--sample-size", str(samples),
                       "--nresamples", "100000", "--noplot", "--output-format", "bencher"]
            print(f"{samples} samples, run {position}/{len(ORDER)}: {role}", flush=True)
            started = time.monotonic()
            with (folder / "output.txt").open("w") as log:
                subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT,
                               env={**os.environ, "CRITERION_HOME": str(folder / "criterion")})
            medians = {}
            for estimate in (folder / "criterion").glob("*/diagnostic/estimates.json"):
                name = json.loads(estimate.with_name("benchmark.json").read_text())["full_id"]
                medians[name] = json.loads(estimate.read_text())["median"]["point_estimate"]
            if set(medians) != set(WORKLOADS):
                raise ValueError(f"Missing diagnostic measurements in {folder}")
            run = {"samples": samples, "position": position, "revision": role,
                   "seconds": time.monotonic() - started, "medians_ns": medians}
            block.append(run)
            runs.append(run)
            (output / "runs.json").write_text(json.dumps(runs, indent=2))
        changes = analyze(block)
        (output / str(samples) / "changes.json").write_text(json.dumps(changes, indent=2))
        lines = [f"### {samples} samples", "",
                 "Ranges across repeated comparisons, using unrounded Criterion median estimates.", "",
                 "| Workload | Candidate/reference, reference first | Candidate/reference, candidate first | Reference repeat | Candidate repeat |",
                 "| --- | ---: | ---: | ---: | ---: |"]
        for name, groups in changes.items():
            cells = [f"{min(values):+.2f}% to {max(values):+.2f}%" for values in groups.values()]
            lines.append(f"| `{name}` | " + " | ".join(cells) + " |")
        summary.extend(lines + [""])
        (output / "summary.md").write_text("\n".join(summary))
        print("\n".join(lines), flush=True)
    if path := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(path, "a") as destination:
            destination.write("\n".join(summary) + "\n")


if __name__ == "__main__":
    main()
