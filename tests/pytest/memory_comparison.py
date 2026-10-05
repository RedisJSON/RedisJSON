"""Non-gating memory/SET comparison of two built modules on identical fixtures."""
import argparse
import contextlib
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

import redis

from test_memory_nightly_report import get_test_documents


@contextlib.contextmanager
def server(binary, module, log_path):
    with tempfile.TemporaryDirectory(prefix='json-memory-') as directory:
        socket = str(Path(directory) / 'redis.sock')
        with log_path.open('w') as log:
            process = subprocess.Popen([
                str(binary), '--port', '0', '--unixsocket', socket,
                '--save', '', '--appendonly', 'no', '--dir', directory,
                '--loadmodule', str(module),
            ], stdout=log, stderr=subprocess.STDOUT)
            client = redis.Redis(unix_socket_path=socket, decode_responses=False,
                                 socket_timeout=300)
            try:
                for _ in range(500):
                    try:
                        client.ping()
                        break
                    except redis.ConnectionError:
                        if process.poll() is not None:
                            raise RuntimeError(f'Redis exited; see {log_path.name}')
                        time.sleep(.02)
                else:
                    raise RuntimeError('Redis startup timed out')
                yield client, socket
            finally:
                client.close()
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


def measure(args, module, payload, keys, requests, log):
    with server(args.redis_server, module, log) as (control, socket):
        # Warm command bookkeeping before measuring the empty-process baseline.
        control.execute_command('JSON.SET', 'warmup', '$', 'null')
        control.delete('warmup')
        control.info('memory')
        baseline = control.info('memory')['used_memory']
        loader = redis.Redis(unix_socket_path=socket, decode_responses=False,
                             socket_timeout=300)
        try:
            for i in range(keys):
                if loader.execute_command('JSON.SET', f'doc:{i}', '$', payload) != b'OK':
                    raise RuntimeError('JSON.SET failed')
        finally:
            loader.close()
        control.ping()  # Let the server process the loading client's disconnect.
        process_bytes = control.info('memory')['used_memory'] - baseline
        key_bytes = sum(control.memory_usage(f'doc:{i}') for i in range(keys))
        # Replacement SET, as in the existing local comparison; no latency gate.
        for i in range(min(requests, 1000)):
            control.execute_command('JSON.SET', f'doc:{i % keys}', '$', payload)
        control.config_resetstat()
        start = time.perf_counter()
        for offset in range(0, requests, min(keys, 100)):
            with control.pipeline(transaction=False) as pipe:
                for i in range(offset, min(offset + min(keys, 100), requests)):
                    pipe.execute_command('JSON.SET', f'doc:{i % keys}', '$', payload)
                if any(reply != b'OK' for reply in pipe.execute()):
                    raise RuntimeError('SET returned an unexpected reply')
        elapsed = time.perf_counter() - start
        stat = control.info('commandstats')['cmdstat_json.set']
        if stat['calls'] != requests or stat.get('failed_calls', 0) or stat.get('rejected_calls', 0):
            raise RuntimeError(f'Invalid command statistics: {stat}')
        digest = None
        for i in range(keys):
            value = control.execute_command('JSON.GET', f'doc:{i}')
            current = hashlib.sha256(value).hexdigest()
            if digest is not None and current != digest:
                raise RuntimeError('Keys have different contents')
            digest = current
        return dict(process_bytes=process_bytes, key_bytes=key_bytes,
                    set_us=stat['usec'] / requests, ops_s=requests / elapsed,
                    readback_sha256=digest, calls=stat['calls'])


def write_report(output, report):
    (output / 'results.json').write_text(json.dumps(report, indent=2))
    lines = [
        '# Memory and SET comparison against master', '',
        'Informational only: no performance or memory thresholds. Negative change is better.',
        'Medians of alternating runs. SET times are server commandstats microseconds; '
        'throughput in JSON includes client/transport time. These are replacement SETs.',
        'Process bytes = INFO MEMORY used_memory minus empty-process baseline after loading; '
        'not RSS. Key bytes = sum of MEMORY USAGE, using each revision\'s accounting policy; '
        'with proportional accounting, shared strings are divided among live references. '
        'Both include unused capacity; no debug discounts.',
        'Fixtures: existing nightly small/medium/large; large_repetition adds the historical '
        '19-byte string 400 times. City uses one full citylots document. '
        'Raw readback hashes must agree across both versions and all trials.', '',
        '| Scenario | Keys / SETs per run | Master process B | Current process B | Change | '
        'Master key B | Current key B | Change | Master SET µs | Current SET µs | Change | Status |',
        '|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|',
    ]
    for name, case in report['cases'].items():
        groups = {v: [r for r in case['runs'] if r['variant'] == v and 'error' not in r]
                  for v in ('master', 'current')}
        valid = all(len(rows) == report['repetitions'] for rows in groups.values())
        valid = valid and len({r['readback_sha256'] for rows in groups.values() for r in rows}) == 1
        columns = []
        for metric in ('process_bytes', 'key_bytes', 'set_us'):
            if valid:
                a, b = (statistics.median(r[metric] for r in groups[v]) for v in groups)
                change = f'{(b / a - 1) * 100:+.2f}%' if a > 0 else 'N/A'
                columns.extend([f'{a:,.3f}' if metric == 'set_us' else f'{a:,.0f}',
                                f'{b:,.3f}' if metric == 'set_us' else f'{b:,.0f}', change])
            else:
                columns.extend(['N/A'] * 3)
        lines.append(f"| {name} | {case['keys']} / {case['requests']} | " +
                     ' | '.join(columns) + (' | OK |' if valid else ' | INCOMPLETE / ERROR |'))
    lines += ['', '## Provenance', '', '```json', json.dumps(report['provenance'], indent=2), '```',
              '', '## Errors', '']
    errors = report['errors'] + [f"{name}: {r['variant']}: {r['error']}"
                              for name, case in report['cases'].items()
                              for r in case['runs'] if 'error' in r]
    lines += errors or ['None. Incomplete rows can also indicate readback mismatch; see results.json.']
    (output / 'report.md').write_text('\n'.join(lines) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('redis-server', 'master-module', 'current-module', 'city', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--repetitions', type=int, default=3)
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error('repetitions must be positive')
    args.output.mkdir(parents=True, exist_ok=True)
    report = dict(repetitions=args.repetitions, cases={}, errors=[], provenance={
        'redis_server': str(args.redis_server.resolve()),
        'city': str(args.city.resolve()),
    })
    modules = {'master': args.master_module.resolve(), 'current': args.current_module.resolve()}
    for variant, module in modules.items():
        report['provenance'][variant] = dict(module=str(module))
        if module.is_file():
            report['provenance'][variant]['sha256'] = hashlib.sha256(module.read_bytes()).hexdigest()
    write_report(args.output, report)
    docs = get_test_documents()
    # Preserve the historical large-repetition fixture verbatim (19 UTF-8 bytes x 400).
    docs['large_repetition'] = {'doc': {**docs['large']['doc'],
                                      'repeated_strings': ['Redis is very fast. '] * 400}}
    for name in ('small', 'medium', 'large', 'large_repetition', 'city'):
        keys, requests = (1, 3) if name == 'city' else (100, 10000)
        case = dict(keys=keys, requests=requests, runs=[])
        report['cases'][name] = case
        try:
            payload = args.city.read_bytes() if name == 'city' else json.dumps(docs[name]['doc']).encode()
            case.update(input_bytes=len(payload), sha256=hashlib.sha256(payload).hexdigest())
            for trial in range(args.repetitions):
                for variant in (list(modules) if trial % 2 == 0 else list(reversed(modules))):
                    row = dict(variant=variant, trial=trial)
                    try:
                        row.update(measure(args, modules[variant], payload, keys, requests,
                                           args.output / f'{name}-{variant}-{trial}.log'))
                    except Exception as error:
                        row['error'] = str(error)
                    case['runs'].append(row)
                    write_report(args.output, report)
                    print(name, variant, trial, row, flush=True)
        except Exception as error:
            report['errors'].append(f'{name}: {error}')
        write_report(args.output, report)


if __name__ == '__main__':
    main()
