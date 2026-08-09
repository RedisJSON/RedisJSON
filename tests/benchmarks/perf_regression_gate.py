#!/usr/bin/env python3
"""Gate CI on performance regressions, using the benchmark results already in RedisTimeSeries.

The EC2 benchmark suite (``redisbench-admin run-remote``, driven by benchmark-flow.yml) pushes
every run to the performance RedisTimeSeries instance with ``--push_results_redistimeseries``.
This script reads those series back and decides whether the most recent results regressed. It
does not run any benchmark itself -- GitHub runners are not a trustworthy source of performance
signal, so the only numbers we trust are the ones measured on the perf EC2 fleet.

Baseline semantics
------------------
For each (test, metric) pair the baseline is the *best trailing-window median* observed inside
the query window -- the best sustained performance, not the single luckiest run. Two properties
follow directly:

* It ratchets. Because the baseline is a max (for higher-better metrics) over history, a slow
  run can never lower it. Performance cannot erode silently one small step at a time.
* It is robust. Ratcheting on a median-of-N rather than on individual runs is what makes the
  ratchet usable at all: EC2 run-to-run variance is large, and a max over single noisy samples
  would latch onto an outlier and then fail forever.

Deliberately lowering the bar is a human decision, so it is a reviewed change in git: add an
entry to the approved-baselines file (``--baselines-file``) recording the new value, the date and
why. Nothing this script does can lower the bar on its own.

Exit codes
----------
0   no regressions, or regressions found while running without ``--enforce`` (warn-only mode)
1   regressions found and ``--enforce`` was given
2   the gate itself could not run (bad connection, no matching series, unreadable config).
    This is always fatal, including in warn-only mode: a gate that silently finds nothing must
    never be mistaken for a gate that passed.
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import sys
import time
from collections import defaultdict
from typing import Any, Dict, Iterable, List, Optional, Sequence, Tuple

import redis
import yaml

DAY_MS = 24 * 60 * 60 * 1000

# The architecture the exporter assumes when it writes a series; see build_filters().
ARCH_DEFAULT = "x86_64"

# Series whose key contains this segment hold SLO targets rather than measurements.
TARGET_SEGMENT = "/target/"

VERDICT_OK = "ok"
VERDICT_IMPROVED = "improved"
VERDICT_REGRESSED = "regressed"
VERDICT_SKIPPED_NO_DATA = "skipped:insufficient-data"
VERDICT_SKIPPED_UNSTABLE = "skipped:too-noisy"


# --------------------------------------------------------------------------------------
# configuration
# --------------------------------------------------------------------------------------
def leaf_metric_name(metric: str) -> str:
    """Return the ``metric`` label value that redisbench-admin stores for a defaults.yml entry.

    The exporter labels each series with ``str(metric.path)`` -- the *leaf* of the jsonpath match
    -- so ``$.Tests.Overall.rps`` is stored as ``rps``. defaults.yml comparison lists mix bare
    leaf names ("Ops/sec") with full jsonpaths, and querying with the raw jsonpath matches
    nothing, so normalise everything down to the leaf here.
    """
    leaf = metric.strip()
    if leaf.startswith("$"):
        leaf = leaf.rsplit(".", 1)[-1]
    return leaf.strip('"').strip("'").strip("[]")


def load_defaults(path: str) -> Tuple[List[str], str]:
    """Read the comparison metrics and metric mode out of a benchmark defaults.yml."""
    with open(path) as fd:
        defaults = yaml.safe_load(fd) or {}
    comparison = (defaults.get("exporter") or {}).get("comparison") or {}
    metrics = [leaf_metric_name(m) for m in comparison.get("metrics") or []]
    # Preserve order while dropping duplicates introduced by normalisation.
    metrics = list(dict.fromkeys(m for m in metrics if m))
    return metrics, comparison.get("mode", "higher-better")


def load_approved_baselines(path: Optional[str]) -> Dict[str, Dict[str, Any]]:
    """Load the human-approved baseline overrides, keyed by ``"<test>|<metric>"``.

    Absent file means "nothing approved", which is the normal state.
    """
    if not path or not os.path.exists(path):
        return {}
    with open(path) as fd:
        doc = json.load(fd) or {}
    approved: Dict[str, Dict[str, Any]] = {}
    for entry in doc.get("baselines") or []:
        test = entry.get("test_name")
        metric = entry.get("metric")
        if not test or not metric:
            raise ValueError(
                f"{path}: every baselines[] entry needs both 'test_name' and 'metric'; got {entry!r}"
            )
        if "baseline" not in entry and "threshold_pct" not in entry:
            raise ValueError(
                f"{path}: entry for {test}/{metric} sets neither 'baseline' nor 'threshold_pct'"
            )
        approved[f"{test}|{metric}"] = entry
    return approved


# --------------------------------------------------------------------------------------
# RedisTimeSeries access
# --------------------------------------------------------------------------------------
def connect(args: argparse.Namespace) -> redis.Redis:
    conn = redis.Redis(
        host=args.host,
        port=args.port,
        password=args.password,
        username=args.user,
        decode_responses=True,
        retry_on_timeout=True,
        socket_timeout=args.socket_timeout,
        socket_connect_timeout=args.socket_timeout,
    )
    conn.ping()
    return conn


def build_filters(args: argparse.Namespace, metric: str) -> List[str]:
    """Build the TS.QUERYINDEX label filters for one metric.

    Mirrors the filter set redisbench-admin's own ``compare`` builds, so that this gate and the
    PR-comment tooling look at exactly the same populations.
    """
    filters = [
        f"branch={args.branch}",
        f"metric={metric}",
        f"deployment_name={args.deployment_name}",
        f"triggering_env={args.triggering_env}",
        f"github_org={args.github_org}",
        f"github_repo={args.github_repo}",
    ]
    if args.running_platform:
        filters.append(f"running_platform={args.running_platform}")
    # Only constrain the architecture when it is not the default one, exactly as compare does.
    # The `arch` label was added to the exporter later than these series started being written,
    # so filtering on arch=x86_64 would silently drop the older history the ratchet depends on.
    if args.arch and args.arch != ARCH_DEFAULT:
        filters.append(f"arch={args.arch}")
    return filters


def normalise_mrange(reply: Any) -> List[Tuple[str, Dict[str, str], List[Tuple[int, float]]]]:
    """Flatten a TS.MRANGE reply into ``(key, labels, datapoints)`` triples.

    redis-py has returned a few different shapes for MRANGE across versions, so accept the ones
    we may plausibly meet and fail loudly on anything else rather than silently reporting no data.
    """
    out: List[Tuple[str, Dict[str, str], List[Tuple[int, float]]]] = []

    def add(key: Any, labels: Any, points: Any) -> None:
        if isinstance(labels, list):  # [[label, value], ...]
            labels = {lbl: val for lbl, val in labels}
        out.append(
            (
                str(key),
                {str(k): str(v) for k, v in (labels or {}).items()},
                [(int(ts), float(val)) for ts, val in (points or [])],
            )
        )

    if isinstance(reply, dict):
        # redis-py >= 5 with RESP3: {key: [labels, datapoints]}
        for key, payload in reply.items():
            add(key, payload[0], payload[1])
        return out

    if isinstance(reply, list):
        for item in reply:
            if isinstance(item, dict):
                # redis-py 4/5 with RESP2: [{key: [labels, datapoints]}, ...]
                for key, payload in item.items():
                    add(key, payload[0], payload[1])
            elif isinstance(item, (list, tuple)) and len(item) == 3:
                # raw RESP2: [[key, [[label, value], ...], [[ts, value], ...]], ...]
                add(item[0], item[1], item[2])
            else:
                raise TypeError(f"unrecognised TS.MRANGE entry: {item!r}")
        return out

    raise TypeError(f"unrecognised TS.MRANGE reply of type {type(reply).__name__}")


def fetch_series(
    conn: redis.Redis, filters: Sequence[str], from_ts: int, to_ts: int
) -> List[Tuple[str, Dict[str, str], List[Tuple[int, float]]]]:
    reply = conn.ts().mrange(from_ts, to_ts, list(filters), with_labels=True)
    series = normalise_mrange(reply)
    return [s for s in series if TARGET_SEGMENT not in s[0]]


# --------------------------------------------------------------------------------------
# analysis
# --------------------------------------------------------------------------------------
def rolling_medians(values: Sequence[float], window: int) -> List[float]:
    """Median of every consecutive ``window``-sized slice of ``values``."""
    if len(values) < window:
        return []
    return [
        statistics.median(values[i : i + window]) for i in range(len(values) - window + 1)
    ]


def spread_pct(values: Sequence[float]) -> float:
    """Coefficient of variation, as a percentage. 0.0 when it cannot be computed."""
    if len(values) < 2:
        return 0.0
    mean = statistics.fmean(values)
    if mean == 0:
        return 0.0
    return abs(statistics.stdev(values) / mean) * 100.0


def evaluate_group(
    test_name: str,
    metric: str,
    datapoints: List[Tuple[int, float]],
    args: argparse.Namespace,
    higher_better: bool,
    approved: Dict[str, Dict[str, Any]],
) -> Dict[str, Any]:
    """Compare the newest results for one (test, metric) against its ratcheted baseline."""
    datapoints = sorted(datapoints, key=lambda p: p[0])
    values = [v for _, v in datapoints]
    window = args.window

    result: Dict[str, Any] = {
        "test_name": test_name,
        "metric": metric,
        "samples": len(values),
        "window": window,
        "mode": "higher-better" if higher_better else "lower-better",
        "last_datapoint_ms": datapoints[-1][0] if datapoints else None,
    }

    # `current` is the trailing window; `history` is everything before it, so that the run under
    # test never contributes to the baseline it is judged against.
    if len(values) < 2 * window:
        result.update(
            verdict=VERDICT_SKIPPED_NO_DATA,
            note=f"needs {2 * window} samples, have {len(values)}",
        )
        return result

    current = statistics.median(values[-window:])
    history = values[:-window]
    candidates = rolling_medians(history, window)
    derived_baseline = max(candidates) if higher_better else min(candidates)

    baseline = derived_baseline
    threshold_pct = args.threshold_pct
    baseline_source = "derived:best-trailing-median"

    override = approved.get(f"{test_name}|{metric}")
    if override:
        if "baseline" in override:
            baseline = float(override["baseline"])
            baseline_source = "approved:" + str(override.get("approved_on", "unknown-date"))
        if "threshold_pct" in override:
            threshold_pct = float(override["threshold_pct"])

    # EC2 variance is large and test-dependent. Widen the threshold to the noise floor we can
    # actually see in the baseline window, so we only ever fire on a drop bigger than the spread
    # this test normally shows. Mirrors the "waterline" idea in redisbench-admin's compare.
    noise_pct = spread_pct(history[-args.noise_samples :])
    effective_pct = max(threshold_pct, noise_pct)

    result.update(
        current=current,
        baseline=baseline,
        derived_baseline=derived_baseline,
        baseline_source=baseline_source,
        threshold_pct=threshold_pct,
        noise_pct=noise_pct,
        effective_threshold_pct=effective_pct,
    )

    # Guard on the *observed* noise, not on the effective threshold: an explicitly approved
    # per-test threshold is a deliberate choice and must not be second-guessed here.
    if noise_pct > args.max_threshold_pct:
        result.update(
            verdict=VERDICT_SKIPPED_UNSTABLE,
            note=(
                f"noise {noise_pct:.1f}% exceeds --max-threshold-pct "
                f"{args.max_threshold_pct:.1f}%; not gateable"
            ),
        )
        return result

    if baseline == 0:
        result.update(verdict=VERDICT_SKIPPED_NO_DATA, note="baseline is zero")
        return result

    change_pct = (current / baseline - 1.0) * 100.0
    if not higher_better:
        change_pct = -change_pct
    result["change_pct"] = change_pct

    if change_pct < -effective_pct:
        result["verdict"] = VERDICT_REGRESSED
    elif change_pct > effective_pct:
        result["verdict"] = VERDICT_IMPROVED
    else:
        result["verdict"] = VERDICT_OK
    return result


def analyse(
    conn: redis.Redis, args: argparse.Namespace, metrics: Sequence[str], higher_better: bool
) -> Tuple[List[Dict[str, Any]], int]:
    """Evaluate every (test, metric) group. Returns the results and the series count seen."""
    approved = load_approved_baselines(args.baselines_file)
    now_ms = int(time.time() * 1000)
    from_ts = now_ms - args.days * DAY_MS

    results: List[Dict[str, Any]] = []
    series_seen = 0

    for metric in metrics:
        filters = build_filters(args, metric)
        series = fetch_series(conn, filters, from_ts, now_ms)
        series_seen += len(series)
        if not series:
            print(f"note: no series for metric={metric} (filters: {' '.join(filters)})")
            continue

        # A single test can own several series for one metric (one per metric_context_path, i.e.
        # per command measured). Those are different populations, so keep them apart and label
        # the group by its context path.
        grouped: Dict[Tuple[str, str], List[Tuple[int, float]]] = defaultdict(list)
        for _key, labels, points in series:
            test_name = labels.get("test_name")
            if not test_name:
                continue
            context = labels.get("metric_context_path") or ""
            grouped[(test_name, context)].extend(points)

        for (test_name, context), points in sorted(grouped.items()):
            label = f"{metric} @ {context}" if context and context != "None" else metric
            results.append(
                evaluate_group(test_name, label, points, args, higher_better, approved)
            )

    return results, series_seen


# --------------------------------------------------------------------------------------
# reporting
# --------------------------------------------------------------------------------------
def fmt(value: Optional[float]) -> str:
    if value is None:
        return "-"
    if abs(value) >= 100:
        return f"{value:,.0f}"
    return f"{value:.2f}"


def render_summary(
    results: Sequence[Dict[str, Any]], args: argparse.Namespace, enforced: bool
) -> str:
    by_verdict: Dict[str, List[Dict[str, Any]]] = defaultdict(list)
    for r in results:
        by_verdict[r["verdict"]].append(r)

    regressed = by_verdict[VERDICT_REGRESSED]
    lines: List[str] = []
    lines.append("## Performance regression gate")
    lines.append("")
    mode = "**enforcing** (regressions fail this job)" if enforced else (
        "**warn-only** (regressions are reported but do not fail this job)"
    )
    lines.append(f"Mode: {mode}")
    lines.append(
        f"Branch `{args.branch}` &middot; last {args.days} days &middot; "
        f"window {args.window} runs &middot; threshold {args.threshold_pct:.1f}%"
    )
    lines.append("")
    lines.append(
        f"{len(regressed)} regressed &middot; {len(by_verdict[VERDICT_IMPROVED])} improved "
        f"&middot; {len(by_verdict[VERDICT_OK])} stable &middot; "
        f"{len(by_verdict[VERDICT_SKIPPED_UNSTABLE])} too noisy to gate &middot; "
        f"{len(by_verdict[VERDICT_SKIPPED_NO_DATA])} insufficient data"
    )
    lines.append("")

    if regressed:
        lines.append("### Regressions")
        lines.append("")
        lines.append("| Test | Metric | Baseline | Current | Change | Gate | Baseline from |")
        lines.append("| --- | --- | --: | --: | --: | --: | --- |")
        for r in sorted(regressed, key=lambda x: x.get("change_pct", 0.0)):
            lines.append(
                "| {test} | {metric} | {base} | {cur} | {chg:+.1f}% | {gate:.1f}% | {src} |".format(
                    test=r["test_name"],
                    metric=r["metric"],
                    base=fmt(r.get("baseline")),
                    cur=fmt(r.get("current")),
                    chg=r.get("change_pct", 0.0),
                    gate=r.get("effective_threshold_pct", 0.0),
                    src=r.get("baseline_source", "-"),
                )
            )
        lines.append("")
        lines.append(
            "The baseline is the best trailing-%d-run median in the window, so it only ever "
            "moves in the faster direction. To accept one of these as the new bar, add an entry "
            "to `%s` explaining why -- that change is reviewed like any other."
            % (args.window, args.baselines_file)
        )
        lines.append("")

    skipped = by_verdict[VERDICT_SKIPPED_UNSTABLE] + by_verdict[VERDICT_SKIPPED_NO_DATA]
    if skipped:
        lines.append(f"<details><summary>Not gated ({len(skipped)})</summary>")
        lines.append("")
        lines.append("| Test | Metric | Samples | Reason |")
        lines.append("| --- | --- | --: | --- |")
        for r in sorted(skipped, key=lambda x: (x["test_name"], x["metric"])):
            lines.append(
                "| {test} | {metric} | {n} | {note} |".format(
                    test=r["test_name"],
                    metric=r["metric"],
                    n=r.get("samples", 0),
                    note=r.get("note", r["verdict"]),
                )
            )
        lines.append("")
        lines.append("</details>")
        lines.append("")

    return "\n".join(lines)


def write_summary(text: str) -> None:
    print(text)
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        with open(summary_path, "a") as fd:
            fd.write(text + "\n")


def run_discovery(conn: redis.Redis, args: argparse.Namespace) -> int:
    """Print what actually exists in RTS for this repo, to validate the query configuration.

    The label values this gate filters on (notably ``triggering_env``, still ``circleci``) are
    easy to get wrong, and a wrong filter yields an empty result set that could be mistaken for
    a clean run. This mode makes the real schema visible.
    """
    prefix = (
        f"ci.benchmarks.redislabs/{args.triggering_env}/{args.github_org}/{args.github_repo}"
    )
    print(f"testcases set: {prefix}:testcases")
    for name, key in (
        ("test cases", f"{prefix}:testcases"),
        ("branches", f"{prefix}:branches"),
        ("deployment names", f"{prefix}:deployment_names"),
        ("archs", f"{prefix}:archs"),
    ):
        try:
            members = sorted(conn.smembers(key))
        except redis.exceptions.ResponseError as exc:
            print(f"  {name}: unreadable ({exc})")
            continue
        print(f"  {name} ({len(members)}): {', '.join(members[:20])}")
        if len(members) > 20:
            print(f"    ... and {len(members) - 20} more")

    print()
    print("Sampling series for the configured filters:")
    now_ms = int(time.time() * 1000)
    from_ts = now_ms - args.days * DAY_MS
    total = 0
    for metric in args.metrics or []:
        filters = build_filters(args, metric)
        try:
            series = fetch_series(conn, filters, from_ts, now_ms)
        except Exception as exc:  # noqa: BLE001 - discovery must report, not crash
            print(f"  metric={metric}: query failed: {exc}")
            continue
        total += len(series)
        print(f"  metric={metric}: {len(series)} series  (filters: {' '.join(filters)})")
        for key, labels, points in series[:3]:
            print(f"    {key}")
            print(f"      datapoints={len(points)} labels={json.dumps(labels, sort_keys=True)}")
    print()
    print(f"total series matched: {total}")
    return 0 if total else 2


# --------------------------------------------------------------------------------------
# entry point
# --------------------------------------------------------------------------------------
def parse_args(argv: Optional[Sequence[str]] = None) -> argparse.Namespace:
    here = os.path.dirname(os.path.abspath(__file__))
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])

    conn = parser.add_argument_group("RedisTimeSeries connection")
    conn.add_argument("--host", default=os.getenv("PERFORMANCE_RTS_HOST", "localhost"))
    conn.add_argument("--port", type=int, default=int(os.getenv("PERFORMANCE_RTS_PORT", 6379)))
    conn.add_argument("--user", default=os.getenv("PERFORMANCE_RTS_USER") or None)
    conn.add_argument("--password", default=os.getenv("PERFORMANCE_RTS_AUTH") or None)
    conn.add_argument("--socket-timeout", type=float, default=30.0)

    scope = parser.add_argument_group("what to compare")
    scope.add_argument("--github-org", default="RedisJSON")
    scope.add_argument("--github-repo", default="RedisJSON")
    scope.add_argument("--branch", default="master")
    scope.add_argument("--deployment-name", default="oss-standalone")
    scope.add_argument(
        "--triggering-env",
        default="circleci",
        help="must match benchmark-flow.yml's triggering_env input (still 'circleci')",
    )
    scope.add_argument(
        "--arch",
        default=ARCH_DEFAULT,
        help=(
            f"architecture to gate; only becomes a query filter when it differs from "
            f"{ARCH_DEFAULT} (see build_filters). Empty string disables it outright."
        ),
    )
    scope.add_argument("--running-platform", default=None)
    scope.add_argument(
        "--defaults-file",
        default=os.path.join(here, "defaults.yml"),
        help="benchmark defaults.yml supplying the comparison metrics and metric mode",
    )
    scope.add_argument(
        "--metric",
        dest="metric_overrides",
        action="append",
        help="metric label to gate on; repeatable. Overrides the defaults file when given.",
    )

    gate = parser.add_argument_group("gate parameters")
    gate.add_argument("--days", type=int, default=180, help="how far back to read history")
    gate.add_argument(
        "--window",
        type=int,
        default=3,
        help="runs per median; the current window and each baseline candidate are this wide",
    )
    gate.add_argument("--threshold-pct", type=float, default=5.0)
    gate.add_argument(
        "--max-threshold-pct",
        type=float,
        default=25.0,
        help="above this much observed noise a test is reported as not gateable",
    )
    gate.add_argument(
        "--noise-samples",
        type=int,
        default=10,
        help="how many recent historical runs to measure the noise floor over",
    )
    gate.add_argument(
        "--baselines-file",
        default=os.path.join(here, "perf-baselines.json"),
        help="human-approved baseline overrides; the only way the bar can be lowered",
    )

    out = parser.add_argument_group("behaviour and output")
    out.add_argument(
        "--enforce",
        action="store_true",
        help="exit non-zero on regressions. Without it the gate reports but stays green.",
    )
    out.add_argument(
        "--discover",
        action="store_true",
        help="print the series and label values that exist for these filters, then exit",
    )
    out.add_argument("--json-out", default=None, help="write the full results as JSON here")

    args = parser.parse_args(argv)
    if args.arch == "":
        args.arch = None
    return args


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = parse_args(argv)

    try:
        metrics, mode = load_defaults(args.defaults_file)
    except (OSError, yaml.YAMLError) as exc:
        print(f"error: cannot read {args.defaults_file}: {exc}", file=sys.stderr)
        return 2
    if args.metric_overrides:
        metrics = [leaf_metric_name(m) for m in args.metric_overrides]
    args.metrics = metrics
    if not metrics:
        print(
            f"error: no comparison metrics configured in {args.defaults_file} and none given "
            "via --metric",
            file=sys.stderr,
        )
        return 2
    higher_better = mode != "lower-better"
    print(f"gating metrics {metrics} ({mode}) on branch {args.branch}")

    try:
        conn = connect(args)
    except (redis.exceptions.RedisError, OSError) as exc:
        print(f"error: cannot reach RedisTimeSeries at {args.host}:{args.port}: {exc}",
              file=sys.stderr)
        return 2

    if args.discover:
        return run_discovery(conn, args)

    try:
        approved_count = len(load_approved_baselines(args.baselines_file))
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"error: cannot read {args.baselines_file}: {exc}", file=sys.stderr)
        return 2
    if approved_count:
        print(f"applying {approved_count} approved baseline override(s) from {args.baselines_file}")

    try:
        results, series_seen = analyse(conn, args, metrics, higher_better)
    except (redis.exceptions.RedisError, TypeError, ValueError) as exc:
        print(f"error: querying RedisTimeSeries failed: {exc}", file=sys.stderr)
        return 2

    if series_seen == 0:
        print(
            "error: no benchmark series matched. The gate cannot pass on an empty result set -- "
            "check --triggering-env / --branch / --github-org / --github-repo, or run with "
            "--discover to see what exists.",
            file=sys.stderr,
        )
        return 2

    summary = render_summary(results, args, args.enforce)
    write_summary(summary)

    if args.json_out:
        with open(args.json_out, "w") as fd:
            json.dump(
                {
                    "branch": args.branch,
                    "generated_ms": int(time.time() * 1000),
                    "enforced": args.enforce,
                    "threshold_pct": args.threshold_pct,
                    "window": args.window,
                    "days": args.days,
                    "series_seen": series_seen,
                    "results": results,
                },
                fd,
                indent=2,
                sort_keys=True,
            )
        print(f"wrote {args.json_out}")

    regressions = [r for r in results if r["verdict"] == VERDICT_REGRESSED]
    if regressions:
        print(f"{len(regressions)} regression(s) beyond the gate:")
        for r in regressions:
            print(
                f"  {r['test_name']} / {r['metric']}: {fmt(r.get('current'))} vs baseline "
                f"{fmt(r.get('baseline'))} ({r.get('change_pct', 0.0):+.1f}%)"
            )
        if args.enforce:
            return 1
        print("warn-only mode: not failing the job.")
    else:
        print("no regressions beyond the gate.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
