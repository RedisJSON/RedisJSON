"""Run the complete nightly comparison on one dedicated AWS server/client pair."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

from redis.client import parse_info
import yaml

from compare_local import escape_dataset_unicode, summary, throughput


class RemoteResetError(RuntimeError):
    """Stop the suite if its dedicated remote environment cannot be reset."""


def provision(state, source, shard=0):
    from python_terraform import Terraform
    from redisbench_admin.utils.remote import fetch_remote_setup_from_config, setup_remote_environment

    config = yaml.safe_load((source / 'defaults.yml').read_text())
    directory, _, _ = fetch_remote_setup_from_config(config['remote'])
    # Persist before apply so the always() step can clean up a partial deployment.
    data = {'terraform_dir': directory}
    state.write_text(json.dumps(data))
    organization, repository = os.environ['GITHUB_REPOSITORY'].split('/')
    result = setup_remote_environment(
        Terraform(working_dir=directory), os.environ['GITHUB_SHA'],
        os.environ['GITHUB_ACTOR'],
        f"redisjson-nightly-{os.environ['GITHUB_RUN_ID']}-{os.environ['GITHUB_RUN_ATTEMPT']}-{shard}",
        organization, repository, 'github-actions.nightly-comparison', 21600,
    )
    code, user, server_private, server_public, port, _, client_public = result
    if code:
        raise RuntimeError(f'Terraform apply failed: {code}')
    data.update(user=user, server_private_ip=server_private,
                server_public_ip=server_public, client_public_ip=client_public, port=port)
    state.write_text(json.dumps(data))


def destroy(state):
    if not state.exists():
        return
    from python_terraform import Terraform, IsNotFlagged

    data = json.loads(state.read_text())
    code, stdout, stderr = Terraform(working_dir=data['terraform_dir']).destroy(
        capture_output=True, no_color=IsNotFlagged, force=IsNotFlagged, auto_approve=True,
    )
    if code:
        raise RuntimeError(f'AWS teardown failed: {stdout}\n{stderr}')


def remote_commands(inventory, key, commands):
    from redisbench_admin.utils.remote import execute_remote_commands

    results = execute_remote_commands(
        inventory['server_public_ip'], inventory['user'], str(key), commands, 22, timeout=30,
    )
    for code, stdout, stderr in results:
        if code:
            raise RuntimeError(f'Remote command failed: {stderr}')
    return [''.join(stdout) for _, stdout, _ in results]


def run_one(spec, module, directory, datasets, inventory, key, remote_config):
    directory.mkdir(parents=True)
    (directory / 'datasets').symlink_to(datasets, target_is_directory=True)
    config = yaml.safe_load(spec.read_text())
    config.pop('kpis', None)  # Relative comparison, not the historical AWS floors.
    config['remote'] = remote_config
    (directory / 'test.yml').write_text(yaml.safe_dump(config, sort_keys=False))
    hosts = ','.join(f'{name}={inventory[name]}' for name in
                     ('server_private_ip', 'server_public_ip', 'client_public_ip'))
    command = [
        'redisbench-admin', 'run-remote', '--test', 'test.yml',
        '--module_path', str(module), '--required-module', 'ReJSON',
        '--inventory', hosts, '--user', inventory['user'], '--private_key', str(key),
        '--db_port', str(inventory['port']), '--keep_env_and_topo',
        '--allowed-envs', 'oss-standalone', '--github_org', 'RedisJSON',
        '--github_repo', 'RedisJSON', '--github_branch', directory.name,
    ]
    env = dict(os.environ, BENCHMARK_REPETITIONS='1', BENCHMARK_RUNNER_GROUP_M_ID='1',
               BENCHMARK_RUNNER_GROUP_TOTAL='1', PUSH_RTS='0', PUSH_S3='',
               PROFILE='0', SKIP_DB_SETUP='0', SKIP_REDIS_SPIN='0')
    cli = f"redis-cli -p {int(inventory['port'])}"
    try:
        with (directory / 'runner.log').open('w') as log:
            try:
                completed = subprocess.run(command, cwd=directory, env=env, stdout=log,
                                           stderr=subprocess.STDOUT, timeout=1800)
            except subprocess.TimeoutExpired as error:
                raise RemoteResetError('Remote client timed out; stopping before another workload') from error
        if completed.returncode:
            raise RuntimeError(f'Benchmark exited {completed.returncode}; see {directory / "runner.log"}')
        server, = remote_commands(inventory, key, [f'{cli} --raw INFO server'])
        measurements = {}
        raw = list(directory.glob('*.json'))
        if len(raw) != 1:
            raise ValueError(f'Expected one benchmark result, found {len(raw)}')
        measurements['ops_per_sec'] = throughput(json.loads(raw[0].read_text()))
        measurements['redis_version'] = parse_info(server)['redis_version']
        return measurements
    finally:
        # Dedicated topology owned by this job; failure to reset must stop the run.
        # The shell tolerates an already-stopped server, but never a live server
        # that failed to shut down. No other workload may inherit its dataset.
        try:
            remote_commands(inventory, key, [
                f'if {cli} ping >/dev/null 2>&1; then {cli} shutdown nosave; fi'
            ])
        except Exception as error:
            raise RemoteResetError(f'Cannot reset AWS Redis: {error}') from error


def compare(args):
    source = args.master_dir.resolve() / 'tests/benchmarks'
    modules = {label: root.resolve() / 'target/release/librejson.so'
               for label, root in [('baseline', args.baseline_dir), ('master', args.master_dir)]}
    plan = json.loads(args.plan.read_text()) if args.plan else None
    specs = sorted(path for path in source.glob('*.yml') if path.name != 'defaults.yml')
    if plan:
        if not 0 <= args.shard < len(plan['shards']):
            raise ValueError('Invalid shard index')
        names = plan['shards'][args.shard]
        if not names or any(Path(name).name != name or not (source / name).is_file() for name in names):
            raise ValueError('Invalid or empty shard')
        specs = [source / name for name in names]
    inventory = json.loads(args.state.read_text())
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    datasets = output / 'datasets'
    shutil.copytree(source / 'datasets', datasets)
    for dataset in datasets.rglob('*.json'):
        dataset.write_text(escape_dataset_unicode(dataset.read_text(encoding='utf-8')), encoding='utf-8')
    remote_config = yaml.safe_load((source / 'defaults.yml').read_text())['remote']
    results = {
        'modules': {label: hashlib.sha256(module.read_bytes()).hexdigest() for label, module in modules.items()},
        'revisions': plan['revisions'] if plan else {label: subprocess.check_output(
            ['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
            for label, root in [('baseline', args.baseline_dir), ('master', args.master_dir)]},
        'benchmarks': {},
    }
    if plan and results['modules'] != plan['modules']:
        raise ValueError('Binaries differ from the shared build')
    failed = False
    aborted = False
    for spec in specs:
        pair = results['benchmarks'][spec.stem] = {}
        for label, module in modules.items():
            print(f'Running {spec.name}: {label}', flush=True)
            started = time.monotonic()
            try:
                if aborted:
                    raise RemoteResetError('Not run: the AWS environment could not be reset')
                value = run_one(spec, module, output / spec.stem / label, datasets,
                                inventory, args.private_key.resolve(), remote_config)
                version = value['redis_version']
                if results.setdefault('redis', version) != version:
                    raise ValueError('Redis version changed during comparison')
                pair[label] = value
            except Exception as error:
                failed = True
                aborted = aborted or isinstance(error, RemoteResetError)
                pair[label] = {'error': str(error)}
                print(f'ERROR: {error}', flush=True)
            pair[label]['run_seconds'] = round(time.monotonic() - started, 3)
            (output / 'comparison.json').write_text(json.dumps(results, indent=2) + '\n')
        revisions = '\n'.join(f'- {label}: `{sha}`' for label, sha in results['revisions'].items())
        (output / 'summary.md').write_text(revisions + '\n\n' + summary(results, 'baseline', 'master'))
    if not results['benchmarks']:
        raise ValueError('No benchmark workloads found')
    return int(failed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('provision', 'run', 'destroy'))
    parser.add_argument('--state', type=Path, required=True)
    parser.add_argument('--benchmarks-dir', type=Path)
    parser.add_argument('--private-key', type=Path)
    parser.add_argument('--baseline-dir', type=Path)
    parser.add_argument('--master-dir', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--plan', type=Path)
    parser.add_argument('--shard', type=int, default=0)
    args = parser.parse_args()
    required = {'provision': ['benchmarks_dir'], 'run': ['private_key', 'baseline_dir', 'master_dir', 'output'],
                'destroy': []}[args.command]
    if any(getattr(args, name) is None for name in required):
        parser.error(f'{args.command} requires: {", ".join(required)}')
    if args.command == 'provision':
        provision(args.state, args.benchmarks_dir, args.shard)
    elif args.command == 'destroy':
        destroy(args.state)
    else:
        return compare(args)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
