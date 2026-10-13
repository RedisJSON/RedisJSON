"""Compare two RedisJSON modules using the same command benchmarks and datasets."""

import argparse
import hashlib
from html import escape
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import socket
from statistics import median
import subprocess
import time

import redis
import yaml


def escape_dataset_unicode(text):
    # redisbench-admin 0.12.39 decodes redis-benchmark's echoed command as ASCII.
    # Escape only non-ASCII JSON characters; retain formatting, numbers and values.
    return re.sub(r"[^\x00-\x7f]", lambda match: json.dumps(match[0])[1:-1], text)


def throughput(result):
    if "Tests" in result:
        if "rps" not in result["Tests"].get("Overall", {}):
            raise ValueError("Missing throughput result; check runner.log for benchmark errors")
        value = float(result["Tests"]["Overall"]["rps"])
    else:
        totals = result["ALL STATS"]["Totals"]
        if totals.get("Connection Errors", 0):
            raise ValueError("memtier reported connection errors")
        value = float(totals["Ops/sec"])
    if not math.isfinite(value) or value <= 0:
        raise ValueError("Missing or invalid throughput measurement")
    return value


def owns_server(connection, db_root):
    """Never stop a Redis instance belonging to another local process."""
    try:
        directory = connection.config_get("dir")["dir"]
        return Path(directory).resolve().is_relative_to(db_root.resolve())
    except redis.ConnectionError:
        return False


def run_one(spec, module, directory, datasets, redis_binary, runner, timeout):
    directory.mkdir(parents=True)
    (directory / "datasets").symlink_to(datasets, target_is_directory=True)
    # redisbench-admin matches module options against the module path ("rejson").
    # Bundle labels such as master.so/pr.so must not disable those options.
    module_alias = directory / ("rejson" + module.suffix)
    module_alias.symlink_to(module)
    config = yaml.safe_load(spec.read_text())
    # AWS's absolute throughput floors do not apply to this runner.
    config.pop("kpis", None)
    # Each invocation is one fresh standalone instance, with no remote exporters.
    config.pop("remote", None)
    (directory / "test.yml").write_text(yaml.safe_dump(config, sort_keys=False))
    db_root = directory / "db"
    db_root.mkdir()
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    connection = redis.Redis(
        host="127.0.0.1", port=port, decode_responses=True,
        socket_connect_timeout=2, socket_timeout=5,
    )
    command = [
        runner, "run-local", "--test", "test.yml",
        "--module_path", str(module_alias), "--required-module", "ReJSON",
        "--redis-binary", redis_binary, "--port", str(port),
        "--host", "127.0.0.1", "--db-dirname", str(db_root),
        "--keep_env_and_topo", "--allowed-envs", "oss-standalone",
        "--allowed-setups", "oss-standalone",
        "--github_org", "RedisJSON", "--github_repo", "RedisJSON",
        "--github_branch", directory.name,
    ]
    env = dict(os.environ, BENCHMARK_REPETITIONS="1", BENCHMARK_RUNNER_GROUP_M_ID="1",
               BENCHMARK_RUNNER_GROUP_TOTAL="1", PUSH_RTS="0", PUSH_S3="",
               PROFILE="0", SKIP_DB_SETUP="0", SKIP_REDIS_SPIN="0")
    try:
        with (directory / "runner.log").open("w") as log:
            with subprocess.Popen(command, cwd=directory, env=env, stdout=log,
                                  stderr=subprocess.STDOUT, start_new_session=True) as process:
                try:
                    code = process.wait(timeout=timeout)
                except BaseException:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                    raise
        if code:
            raise RuntimeError(f"Benchmark exited {code}; see {directory / 'runner.log'}")
        if not owns_server(connection, db_root):
            raise RuntimeError("Benchmark did not leave its own Redis instance running")
        measurements = {}
        raw_results = list(directory.glob("*.json"))
        if len(raw_results) != 1:
            raise ValueError(f"Expected one benchmark result, found {len(raw_results)}")
        measurements["ops_per_sec"] = throughput(json.loads(raw_results[0].read_text()))
        return measurements
    finally:
        try:
            if owns_server(connection, db_root):
                connection.shutdown(nosave=True)
        finally:
            connection.close()
            module_alias.unlink(missing_ok=True)


def summary(results, baseline="master", candidate="pr"):
    before_label = "PR" if baseline == "pr" else baseline.title()
    after_label = "PR" if candidate == "pr" else candidate.title()
    lines = [
        "# RedisJSON command benchmarks: throughput", "",
        "Same workloads, sequential runs, fresh Redis per revision and test.",
        "🟠 Benchmark name: retried because a previous pair differed by more than 5%; showing medians across all attempts.",
        "Change %: 🟢 improvement; 🟡 degradation below 5%; 🔴 degradation of 5% or more. Unchanged values are unmarked.",
        f"Each metric has {before_label}, {after_label}, and Change % columns. Higher throughput is better.",
        "For retried tests, throughput columns show each revision’s median; Change % is the median of paired changes, not the change between those columns.",
        f"{before_label} module SHA256: `{results['modules'][baseline]}`",
        f"{after_label} module SHA256: `{results['modules'][candidate]}`", "",
        '<table>',
        '<thead><tr><th rowspan="2">Benchmark</th>',
        '<th colspan="3">Throughput (ops/s) ↑</th></tr>',
        '<tr>' + f'<th>{escape(before_label)}</th><th>{escape(after_label)}</th><th>Change %</th>' + '</tr></thead>',
        '<tbody>',
    ]
    for name, pair in results["benchmarks"].items():
        label = escape(name)
        attempts = max(value.get("attempt_count", 1) for value in pair.values())
        if attempts > 1:
            aggregation = " (median)" if "change_percent" in pair[candidate] else ""
            label = "🟠 " + label + f"<br><small>{attempts} attempts{aggregation}</small>"
        errors = [f"{revision}: {value['error']}" for revision, value in pair.items()
                  if "error" in value]
        if errors:
            message = escape("; ".join(errors))
            lines.append(f'<tr><td>🔴 {label}</td><td colspan="3"><strong>ERROR</strong>: {message}</td></tr>')
            continue
        before, after = pair[baseline]["ops_per_sec"], pair[candidate]["ops_per_sec"]
        percent = pair[candidate].get("change_percent", 100 * (after - before) / before)
        degradation = -percent
        marker = ""
        if degradation < 0:
            marker = "🟢 "
        elif degradation > 0:
            marker = "🔴 " if degradation >= 5 else "🟡 "
        cells = (f"{before:,.2f}", f"{after:,.2f}", f"{marker}{percent:+.2f}%")
        lines.append(f"<tr><td>{label}</td>" +
                     "".join(f'<td align="right">{escape(cell)}</td>' for cell in cells) + "</tr>")
    lines.extend(["</tbody>", "</table>"])
    return "\n".join(lines) + "\n"


def record_attempt(pair, directory, attempt, max_retries=3):
    history = directory / "attempts"
    history.mkdir(parents=True, exist_ok=True)
    (history / f"attempt-{attempt + 1}.json").write_text(json.dumps(pair, indent=2) + "\n")
    if any("error" in value for value in pair.values()):
        return False
    before, after = (value["ops_per_sec"] for value in pair.values())
    if attempt == max_retries or (attempt == 0 and abs(after - before) * 100 <= before * 5):
        return False
    print(f"Retrying both runs for {directory.name}: change={(after / before - 1) * 100:+.2f}%, "
          f"retry {attempt + 1}/{max_retries}", flush=True)
    archive = history / f"attempt-{attempt + 1}"
    archive.mkdir()
    for label in pair:
        source = directory / label
        if source.exists():
            source.rename(archive / label)
    return True


def run_pairs(specs, modules, output, run_fn, results, max_retries=3, abort_on=()):
    failed = False
    aborted = False
    baseline, candidate = modules
    for spec in specs:
        attempts = []
        for attempt in range(max_retries + 1):
            # Keep result keys in baseline/candidate order even when execution is reversed.
            pair = results["benchmarks"][spec.stem] = {label: {} for label in modules}
            order = list(modules) if attempt % 2 == 0 else list(reversed(modules))
            for label in order:
                module = modules[label]
                print(f"Running {spec.name}: {label}", flush=True)
                started = time.monotonic()
                try:
                    if aborted:
                        raise RuntimeError("Not run: the benchmark environment could not be reset")
                    pair[label] = dict(run_fn(spec, module, output / spec.stem / label))
                except Exception as error:
                    failed = True
                    aborted = aborted or isinstance(error, abort_on)
                    pair[label] = {"error": str(error)}
                    print(f"ERROR: {error}", flush=True)
                finally:
                    if label in pair:
                        pair[label]["attempt_count"] = attempt + 1
                        pair[label]["run_seconds"] = round(time.monotonic() - started, 3)
                    (output / "comparison.json").write_text(json.dumps(results, indent=2) + "\n")
            attempts.append(pair)
            if not record_attempt(pair, output / spec.stem, attempt, max_retries):
                break
        if len(attempts) > 1 and not any("error" in value for value in pair.values()):
            percent = median(100 * (item[candidate]["ops_per_sec"] - item[baseline]["ops_per_sec"])
                             / item[baseline]["ops_per_sec"] for item in attempts)
            pair = {label: dict(value, ops_per_sec=median(item[label]["ops_per_sec"] for item in attempts))
                    for label, value in pair.items()}
            pair[candidate]["change_percent"] = percent
            results["benchmarks"][spec.stem] = pair
            (output / "comparison.json").write_text(json.dumps(results, indent=2) + "\n")
        revisions = "\n".join(f"- {label}: `{sha}`" for label, sha in results.get("revisions", {}).items())
        report = (revisions + "\n\n" if revisions else "") + summary(results, *modules)
        (output / "summary.md").write_text(report)
    if not results["benchmarks"]:
        raise ValueError("No benchmark workloads found")
    return int(failed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-module", type=Path, required=True)
    parser.add_argument("--candidate-module", type=Path, required=True)
    parser.add_argument("--benchmarks-dir", type=Path, default=Path(__file__).resolve().parent)
    parser.add_argument("--benchmark", action="append", help="YAML filename; repeat to select tests")
    parser.add_argument("--output", type=Path, required=True, help="New output directory")
    parser.add_argument("--redis-binary", default="redis-server")
    parser.add_argument("--timeout", type=int, default=1800, help="Seconds per revision per test")
    args = parser.parse_args()
    source = args.benchmarks_dir.resolve()
    specs = [source / name for name in args.benchmark] if args.benchmark else sorted(source.glob("*.yml"))
    specs = [spec for spec in specs if spec.name != "defaults.yml"]
    if not specs or any(not spec.is_file() for spec in specs):
        parser.error("Expected at least one existing benchmark YAML")
    modules = {"master": args.baseline_module.resolve(), "pr": args.candidate_module.resolve()}
    if any(not module.is_file() for module in modules.values()):
        parser.error("Build both modules before running this comparison")
    runner = shutil.which("redisbench-admin")
    redis_binary = shutil.which(args.redis_binary)
    if not runner or not redis_binary:
        parser.error("redisbench-admin and redis-server must be available")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    datasets = output / "datasets"
    shutil.copytree(source / "datasets", datasets)
    for dataset in datasets.rglob("*.json"):
        original = dataset.read_text(encoding="utf-8")
        escaped = escape_dataset_unicode(original)
        if escaped != original:
            dataset.write_text(escaped, encoding="utf-8")
    results = {
        "modules": {label: hashlib.sha256(module.read_bytes()).hexdigest()
                    for label, module in modules.items()},
        "redis": subprocess.check_output([redis_binary, "--version"], text=True).strip(),
        "benchmarks": {},
    }
    def run(spec, module, directory):
        return run_one(spec, module, directory, datasets, redis_binary, runner, args.timeout)

    status = run_pairs(specs, modules, output, run, results)
    print(summary(results))
    return status


if __name__ == "__main__":
    raise SystemExit(main())
