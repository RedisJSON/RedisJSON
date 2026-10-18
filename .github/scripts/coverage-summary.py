#!/usr/bin/env python3
"""Render this run's LCOV artifact for GitHub and the nightly Slack summary."""
import json
import os
from pathlib import Path

status = os.environ.get("COVERAGE_STATUS", "unknown")
scope = os.environ["COVERAGE_SCOPE"]
url = f"https://app.codecov.io/github/{os.environ['GITHUB_REPOSITORY']}/commit/{os.environ['GITHUB_SHA']}"
reports = list(Path("coverage-report").rglob("cov.info"))
metrics = "Coverage report unavailable."
if len(reports) == 1:
    try:
        totals = {key: 0 for key in ("LF", "LH", "FNF", "FNH")}
        seen = set()
        for line in reports[0].read_text().splitlines():
            key, _, value = line.partition(":")
            if key in totals:
                totals[key] += int(value)
                seen.add(key)
        def percentage(hit, found):
            if hit not in seen or found not in seen:
                return "unavailable"
            if not 0 <= totals[hit] <= totals[found]:
                raise ValueError("Invalid coverage counters")
            return f"{100 * totals[hit] / totals[found]:.1f}%" if totals[found] else "N/A"
        metrics = f"Lines: {percentage('LH', 'LF')} | Functions: {percentage('FNH', 'FNF')}"
    except (OSError, ValueError):
        metrics = "Coverage report unreadable."
if status != "success":
    metrics += " Results may be incomplete."
text = f"*Code coverage*\n{metrics}\nJob: {status} | {scope}\n<{url}|View Codecov report>"
Path("coverage-summary-block.json").write_text(json.dumps({
    "type": "section", "text": {"type": "mrkdwn", "text": text}
}))
with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
    summary.write(f"\n## Code coverage\n\n{metrics}\n\nJob: {status} | {scope}\n\n[View Codecov report]({url})\n")
print(metrics)
