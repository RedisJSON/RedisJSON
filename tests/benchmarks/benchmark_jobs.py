"""Plan independent benchmark jobs and merge their complete, validated results."""

import argparse
import hashlib
import json
import math
from pathlib import Path
import shlex
import subprocess
import sys

import yaml

from compare_local import MEMORY_METRICS, summary


def estimated_seconds(spec):
    config = yaml.safe_load(spec.read_text())
    client = config['clientconfig']
    if isinstance(client, list):
        client = {key: value for entry in client for key, value in entry.items()}
    if client['tool'] == 'memtier_benchmark':
        arguments = shlex.split(client['arguments'])
        return 2 * float(arguments[arguments.index('--test-time') + 1])
    parameters = client['parameters']
    if isinstance(parameters, list):
        parameters = {key: value for entry in parameters for key, value in entry.items()}
    # Floors are only initial scheduling estimates, not thresholds for this host.
    rates = [rule['ge']['$.Tests.Overall.rps'] for rule in config.get('kpis', [])
             if '$.Tests.Overall.rps' in rule.get('ge', {})]
    rate = float(rates[0]) if rates else 100_000
    return 2 * float(parameters['requests']) / rate


def make_plan(source, count, timings=None):
    if count < 1:
        raise ValueError('Shard count must be positive')
    specs = sorted(path for path in source.glob('*.yml') if path.name != 'defaults.yml')
    if len(specs) < count:
        raise ValueError('Each shard must have at least one test')
    weights = {}
    for spec in specs:
        pair = (timings or {}).get('benchmarks', {}).get(spec.stem, {})
        measured = [pair.get(label, {}).get('run_seconds', 0) for label in ('master', 'pr')]
        valid = all(isinstance(value, (int, float)) and math.isfinite(value) and value > 0
                    for value in measured)
        weights[spec.name] = (sum(measured) if valid and not any(
            'error' in pair.get(label, {}) for label in ('master', 'pr')
        ) else estimated_seconds(spec))
    shards = [[] for _ in range(count)]
    totals = [0.0] * count
    for name in sorted(weights, key=lambda name: (-weights[name], name)):
        index = min(range(count), key=lambda index: totals[index])
        shards[index].append(name)
        totals[index] += weights[name]
    return {'shards': shards, 'estimated_seconds': totals}


def merge_results(plan, root):
    combined = {'modules': plan['modules'], 'redis': plan['redis'], 'benchmarks': {}}
    issues = []
    for index, names in enumerate(plan['shards']):
        path = root / f'shard-{index}' / 'comparison.json'
        try:
            data = json.loads(path.read_text())
            if data['modules'] != plan['modules'] or data['redis'] != plan['redis']:
                raise ValueError('Binaries or Redis version differ from the build job')
            if not isinstance(data['benchmarks'], dict):
                raise ValueError('Invalid benchmarks object')
            expected = {Path(name).stem for name in names}
            unexpected = set(data['benchmarks']) - expected
            if unexpected:
                raise ValueError(f'Unexpected benchmarks: {sorted(unexpected)}')
        except (OSError, ValueError, KeyError, TypeError) as error:
            issues.append(f'Shard {index}: {error}')
            data = {'benchmarks': {}}
        for name in names:
            name = Path(name).stem
            if name in combined['benchmarks']:
                raise ValueError(f'Duplicate assignment: {name}')
            pair = data['benchmarks'].get(name, {})
            if not isinstance(pair, dict):
                pair = {}
            checked = {}
            for label in ('master', 'pr'):
                value = pair.get(label)
                if not isinstance(value, dict):
                    value = {'error': f'Missing {label} result in shard {index}'}
                elif 'error' not in value:
                    metrics = ('ops_per_sec', *MEMORY_METRICS, 'keys')
                    if any(type(value.get(metric)) not in (int, float)
                           or not math.isfinite(value[metric]) or value[metric] < 0
                           or (metric == 'ops_per_sec' and value[metric] == 0)
                           for metric in metrics):
                        value = {'error': f'Invalid measurements in shard {index}'}
                checked[label] = value
            combined['benchmarks'][name] = checked
    combined['benchmarks'] = dict(sorted(combined['benchmarks'].items()))
    combined['merge_errors'] = issues
    failed = bool(issues) or any('error' in value for pair in combined['benchmarks'].values()
                                 for value in pair.values())
    return combined, failed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    plan_parser = commands.add_parser('plan')
    plan_parser.add_argument('--bundle', type=Path, required=True)
    plan_parser.add_argument('--shards', type=int, default=5)
    plan_parser.add_argument('--timings', type=Path, help='Previous combined comparison.json')
    run_parser = commands.add_parser('run')
    run_parser.add_argument('--bundle', type=Path, required=True)
    run_parser.add_argument('--shard', type=int, required=True)
    run_parser.add_argument('--output', type=Path, required=True)
    merge_parser = commands.add_parser('merge')
    merge_parser.add_argument('--plan', type=Path, required=True)
    merge_parser.add_argument('--results', type=Path, required=True)
    merge_parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'plan':
        bundle = args.bundle.resolve()
        timings = json.loads(args.timings.read_text()) if args.timings else None
        plan = make_plan(bundle / 'suite', args.shards, timings)
        plan['modules'] = {label: hashlib.sha256((bundle / f'{label}.so').read_bytes()).hexdigest()
                           for label in ('master', 'pr')}
        plan['redis'] = subprocess.check_output([str(bundle / 'bin/redis-server'), '--version'], text=True).strip()
        (bundle / 'plan.json').write_text(json.dumps(plan, indent=2) + '\n')
        print(json.dumps(plan, indent=2))
    elif args.command == 'run':
        bundle = args.bundle.resolve()
        plan = json.loads((bundle / 'plan.json').read_text())
        if not 0 <= args.shard < len(plan['shards']):
            parser.error('Invalid shard index')
        names = plan['shards'][args.shard]
        if not names:
            parser.error('Empty shard')
        command = [sys.executable, str(Path(__file__).with_name('compare_local.py')),
                   '--baseline-module', str(bundle / 'master.so'),
                   '--candidate-module', str(bundle / 'pr.so'),
                   '--redis-binary', str(bundle / 'bin/redis-server'),
                   '--benchmarks-dir', str(bundle / 'suite'), '--output', str(args.output)]
        for name in names:
            command.extend(['--benchmark', name])
        return subprocess.call(command)
    else:
        plan = json.loads(args.plan.read_text())
        results, failed = merge_results(plan, args.results)
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / 'comparison.json').write_text(json.dumps(results, indent=2) + '\n')
        report = summary(results)
        if results['merge_errors']:
            report += '\nMerge errors:\n\n' + '\n'.join(results['merge_errors']) + '\n'
        (args.output / 'summary.md').write_text(report)
        print(report)
        return int(failed)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
