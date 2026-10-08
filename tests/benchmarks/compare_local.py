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


def change(before, after):
    if before == 0:
        return "0.00%" if after == 0 else "n/a (zero baseline)"
    return f"{100 * (after / before - 1):+.2f}%"


def summary(results, baseline="master", candidate="pr"):
    before_label = "PR" if baseline == "pr" else baseline.title()
    after_label = "PR" if candidate == "pr" else candidate.title()
    lines = [
        "# RedisJSON command benchmarks: throughput", "",
        "Same workloads, sequential runs, fresh Redis per revision and test.",
        "Change %: 🟢 improvement; 🟡 degradation below 5%; 🔴 degradation of 5% or more. Unchanged values are unmarked.",
        f"Each metric has {before_label}, {after_label}, and Change % columns. Higher throughput is better.",
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
        errors = [f"{revision}: {value['error']}" for revision, value in pair.items()
                  if "error" in value]
        if errors:
            message = escape("; ".join(errors))
            lines.append(f'<tr><td>🔴 {label}</td><td colspan="3"><strong>ERROR</strong>: {message}</td></tr>')
            continue
        before, after = pair[baseline]["ops_per_sec"], pair[candidate]["ops_per_sec"]
        degradation = before - after
        marker = ""
        if degradation < 0:
            marker = "🟢 "
        elif degradation > 0:
            marker = "🔴 " if degradation * 100 >= before * 5 else "🟡 "
        cells = (f"{before:,.2f}", f"{after:,.2f}", f"{marker}{change(before, after)}")
        lines.append(f"<tr><td>{label}</td>" +
                     "".join(f'<td align="right">{escape(cell)}</td>' for cell in cells) + "</tr>")
    lines.extend(["</tbody>", "</table>"])
    return "\n".join(lines) + "\n"


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
    failed = False
    for spec in specs:
        pair = results["benchmarks"][spec.stem] = {}
        for label, module in modules.items():
            print(f"Running {spec.name}: {label}", flush=True)
            started = time.monotonic()
            try:
                pair[label] = run_one(spec, module, output / spec.stem / label,
                                      datasets, redis_binary, runner, args.timeout)
            except Exception as error:
                failed = True
                pair[label] = {"error": str(error)}
                print(f"ERROR: {error}", flush=True)
            finally:
                if label in pair:
                    pair[label]["run_seconds"] = round(time.monotonic() - started, 3)
                (output / "comparison.json").write_text(json.dumps(results, indent=2) + "\n")
        (output / "summary.md").write_text(summary(results))
    print(summary(results))
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
