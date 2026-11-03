"""Resolve nightly revisions and conservatively evaluate baseline promotion."""

import argparse
import json
import math
import os
from pathlib import Path
import subprocess
from urllib.parse import quote


def emit(name, value):
    with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
        output.write(f'{name}={value}\n')


def note(message):
    print(message)
    with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as output:
        output.write(message + '\n\n')


def resolve():
    event = os.environ['GITHUB_EVENT_NAME']
    scheduled_master = event == 'schedule' and os.environ['GITHUB_REF'] == 'refs/heads/master'
    if not scheduled_master and event != 'workflow_dispatch':
        emit('skip', 'true')
        note('Skipped: benchmark comparisons require a schedule on master or a manual run.')
        return
    branch = 'benchmark-baseline' if scheduled_master else os.environ.get('BASELINE_BRANCH', '').strip()
    branch = branch or 'benchmark-baseline'
    if subprocess.run(['git', 'check-ref-format', f'refs/heads/{branch}'],
                      capture_output=True).returncode:
        raise ValueError(f'Invalid baseline branch name: {branch!r}')
    repository = os.environ['GITHUB_REPOSITORY']
    def branch_sha(branch):
        data = json.loads(subprocess.check_output(
            ['gh', 'api', f'repos/{repository}/git/ref/heads/{quote(branch, safe="/")}'], text=True))
        return data['object']['sha']
    baseline = branch_sha(branch)
    candidate = branch_sha('master') if scheduled_master else os.environ['GITHUB_SHA']
    emit('baseline', baseline)
    emit('candidate', candidate)
    emit('skip', str(baseline == candidate).lower())
    note(f'Baseline branch: `{branch}` (`{baseline}`); candidate: `{candidate}`. ' +
         ('Skipped: identical commits, no builds or AWS resources required.' if baseline == candidate else
          'Both commits are pinned for this comparison. Manual runs never promote the baseline.'))


def evaluate(results):
    if results.get('merge_errors') or not results.get('benchmarks'):
        return False, 'Incomplete comparison; baseline retained.'
    ratios = []
    for name, pair in results['benchmarks'].items():
        values = [pair.get(label, {}) for label in ('baseline', 'master')]
        rates = [value.get('ops_per_sec') for value in values]
        if any('error' in value for value in values) or any(
            type(rate) not in (int, float) or not math.isfinite(rate) or rate <= 0 for rate in rates
        ):
            return False, f'Invalid or failed measurement: {name}; baseline retained.'
        before, after = rates
        percent = values[1].get('change_percent', 100 * (after - before) / before)
        if type(percent) not in (int, float) or not math.isfinite(percent) or percent <= -100:
            return False, f'Invalid paired change: {name}; baseline retained.'
        if percent <= -5:
            return False, f'{name} degraded by at least 5%; baseline retained.'
        ratios.append(math.log1p(percent / 100))
    gain = math.expm1(sum(ratios) / len(ratios)) * 100
    eligible = gain > 1e-9  # Ignore floating-point noise around zero improvement.
    return eligible, (f'Geometric mean throughput change: {gain:+.2f}%. ' +
                      ('Promotion criteria met (requires scheduled run and confirmation).' if eligible else
                       'Overall improvement must be positive; baseline retained.'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['resolve', 'evaluate'])
    parser.add_argument('results', type=Path, nargs='?')
    parser.add_argument('--previous', type=Path)
    args = parser.parse_args()
    if args.command == 'resolve':
        resolve()
        return
    results = json.loads(args.results.read_text())
    if args.previous:
        previous = json.loads(args.previous.read_text())
        for key in ('modules', 'revisions', 'redis'):
            if results.get(key) != previous.get(key):
                raise ValueError(f'Confirmation differs from initial round: {key}')
        if set(results['benchmarks']) != set(previous['benchmarks']):
            raise ValueError('Confirmation workload coverage differs from initial round')
    eligible, reason = evaluate(results)
    emit('eligible', str(eligible).lower())
    note(reason)


if __name__ == '__main__':
    main()
