"""Read-only Linux snapshots, collected outside the measured benchmark interval."""

import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time


def read(path):
    try:
        return Path(path).read_text()
    except OSError as error:
        return {'error': str(error)}


def snapshot(port=None):
    data = {'timestamp': time.time(), 'host': {}, 'processes': {}}
    # Before/after deltas expose CPU steal, interrupts, and network retransmits.
    for name in ('stat', 'uptime', 'loadavg', 'pressure/cpu', 'softirqs',
                 'net/snmp', 'net/netstat', 'cpuinfo'):
        data['host'][name] = read('/proc/' + name)
    for process in Path('/proc').glob('[0-9]*'):
        if read(process / 'comm') not in ('redis-server\n', 'redis-benchmark\n'):
            continue
        details = {name: read(process / name) for name in ('stat', 'status', 'sched', 'schedstat')}
        # Thread counters distinguish CPU execution, migrations, and run-queue wait.
        details['threads'] = {
            thread.name: {name: read(thread / name) for name in ('stat', 'sched', 'schedstat')}
            for thread in (process / 'task').glob('[0-9]*')
        }
        maps = read(process / 'maps')
        details['module_mappings'] = [line for line in maps.splitlines() if 'rejson' in line] if isinstance(maps, str) else maps
        data['processes'][process.name] = details
    if port is not None:
        data['redis'] = {}
        for section in ('server', 'cpu', 'stats', 'commandstats', 'latencystats', 'keyspace'):
            result = subprocess.run(
                ['redis-cli', '-p', str(port), '--raw', 'INFO', section],
                capture_output=True, text=True, timeout=5,
            )
            data['redis'][section] = result.stdout if result.returncode == 0 else {'error': result.stderr}
        try:
            data['module_sha256'] = hashlib.sha256(Path('/tmp/librejson.so').read_bytes()).hexdigest()
        except OSError as error:
            data['module_sha256'] = {'error': str(error)}
    return data


if __name__ == '__main__':
    print(json.dumps(snapshot(int(sys.argv[1]) if len(sys.argv) > 1 else None)))
