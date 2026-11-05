import datetime
import json
import os
from pathlib import Path
import sys
import time

import redis


def read_process(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    cpu_seconds = (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")
    rss_bytes = int(fields[21]) * os.sysconf("SC_PAGE_SIZE")
    return cpu_seconds, rss_bytes


def counter_rate(previous, key, value, now):
    baseline = previous.get(key)
    previous[key] = (now, value)
    if baseline is None or now <= baseline[0] or value < baseline[1]:
        return None
    return (value - baseline[1]) / (now - baseline[0])


def sample(client, pids, previous, started):
    record = {"redis_commands_per_second": None}
    try:
        info = client.info()
    except redis.RedisError as error:
        info = None
        record["redis_error"] = f"{type(error).__name__}: {error}"

    now = time.monotonic()
    record["timestamp"] = datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")
    record["elapsed_seconds"] = round(now - started, 3)

    if info is not None:
        commands = info["total_commands_processed"]
        rate = counter_rate(previous, "commands", commands, now)
        record["redis_commands_total"] = commands
        record["redis_commands_per_second"] = round(rate, 3) if rate is not None else None
        record["redis_used_memory_bytes"] = info["used_memory"]
        record["redis_keys"] = sum(
            value["keys"] for key, value in info.items()
            if key.startswith("db") and key[2:].isdigit()
        )

    for name, pid in pids.items():
        record[f"{name}_cpu_percent"] = None
        record[f"{name}_rss_bytes"] = None
        try:
            cpu_seconds, rss_bytes = read_process(pid)
            rate = counter_rate(previous, name, cpu_seconds, now)
            record[f"{name}_cpu_percent"] = round(100 * rate, 3) if rate is not None else None
            record[f"{name}_rss_bytes"] = rss_bytes
        except (OSError, ValueError, IndexError) as error:
            record[f"{name}_process_error"] = f"{type(error).__name__}: {error}"

    return record


def main():
    redis_pid, generator_pid = (int(value) for value in sys.argv[1:])
    client = redis.Redis(
        host="localhost", port=6379,
        socket_connect_timeout=2, socket_timeout=2, retry_on_timeout=False,
    )
    pids = {"redis": redis_pid, "generator": generator_pid}
    previous = {}
    started = time.monotonic()
    with open("diagnostics.log", "w", buffering=1) as log:
        while True:
            line = json.dumps(sample(client, pids, previous, started), sort_keys=True)
            print(line, flush=True)
            print(line, file=log, flush=True)
            time.sleep(15)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"ERROR: Random traffic monitor stopped: {type(error).__name__}: {error}", flush=True)
