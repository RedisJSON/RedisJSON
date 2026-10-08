import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import yaml

from configure_affinity import configure, cpus, select_cores, wait_for_irq_affinity, wrap_client

from compare_aws import RemoteResetError, collect_diagnostics, compare, destroy, provision, run_one


class AWSComparisonTest(unittest.TestCase):
    def test_affinity_reserves_smt_siblings_and_preserves_client_arguments(self):
        siblings = {cpu: {cpu % 4, cpu % 4 + 4} for cpu in range(8)}
        self.assertEqual(cpus('0-2,5,7-8'), {0, 1, 2, 5, 7, 8})
        self.assertEqual(select_cores(set(range(8)), siblings, 2), ([1, 2], {1, 2, 5, 6}))
        with self.assertRaisesRegex(RuntimeError, 'Not enough physical cores'):
            select_cores({0, 4}, siblings, 1)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            # A stand-in taskset checks its arguments and runs the real client.
            taskset = root / 'taskset'
            taskset.write_text('#!/bin/sh\n[ "$1" = "-c" ] && [ "$2" = "1,2" ] || exit 1\nshift 2\nexec "$@"\n')
            taskset.chmod(0o755)
            client = root / 'client with spaces'
            client.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n')
            client.chmod(0o755)
            with patch('configure_affinity.shutil.which', return_value=str(taskset)):
                wrap_client(client, [1, 2])
            self.assertEqual(subprocess.check_output([str(client), 'JSON.GET', 'a b', '$'], text=True),
                             'JSON.GET\na b\n$\n')
            with self.assertRaisesRegex(RuntimeError, 'already wrapped'):
                wrap_client(client, [1, 2])

    def test_nic_isolation_rejects_irqs_still_on_reserved_core(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def fake_path(path):
                return root / str(path).lstrip('/')
            for cpu in range(8):
                path = fake_path(f'/sys/devices/system/cpu/cpu{cpu}/topology/thread_siblings_list')
                path.parent.mkdir(parents=True)
                path.write_text(f'{cpu % 4},{cpu % 4 + 4}')
            fake_path('/sys/class/net/eth0/device/msi_irqs/40').mkdir(parents=True)
            irq = fake_path('/proc/irq/40')
            irq.mkdir(parents=True)
            (irq / 'effective_affinity_list').write_text('0')
            rps = fake_path('/sys/class/net/eth0/queues/rx-0/rps_cpus')
            rps.parent.mkdir(parents=True)
            rps.write_text('ff')
            fake_path('/tmp').mkdir()
            with patch('configure_affinity.Path', side_effect=fake_path), patch(
                'configure_affinity.os.sched_getaffinity', return_value=set(range(8)), create=True
            ), patch('configure_affinity.subprocess.run', return_value=subprocess.CompletedProcess([], 0)) as run:
                data = configure('server', [])
                self.assertEqual(data['cpulist'], '1')
                self.assertEqual(data['reserved_cpus'], [1, 5])
                self.assertEqual(rps.read_text(), '0')
                self.assertEqual((irq / 'smp_affinity_list').read_text(), '0')
                self.assertEqual(run.call_args.args[0], ['systemctl', 'stop', 'irqbalance'])
                (irq / 'effective_affinity_list').write_text('5')
                with patch('configure_affinity.time.monotonic', side_effect=[0, 11]), self.assertRaisesRegex(
                    RuntimeError, 'IRQ affinity did not settle within 10s'
                ):
                    configure('server', [])

    def test_irq_affinity_waits_for_migration_without_reapplying_settings(self):
        with patch('configure_affinity.Path') as path, patch(
            'configure_affinity.time.sleep'
        ) as sleep, patch('configure_affinity.time.monotonic', return_value=0):
            effective = path.return_value.__truediv__.return_value.__truediv__.return_value
            effective.read_text.side_effect = ['1', '0']
            self.assertEqual(wait_for_irq_affinity({'31': 0}), {'31': '0'})
            sleep.assert_called_once_with(0.1)
            effective.write_text.assert_not_called()

    def test_partial_provision_retains_state_for_cleanup_and_cleanup_errors_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'defaults.yml').write_text('remote: []\n')
            state = root / 'state.json'
            environment = dict(GITHUB_REPOSITORY='RedisJSON/RedisJSON', GITHUB_SHA='abc',
                               GITHUB_ACTOR='tester', GITHUB_RUN_ID='1', GITHUB_RUN_ATTEMPT='1')

            def failed_apply(*args):
                self.assertEqual(json.loads(state.read_text())['terraform_dir'], str(root))
                self.assertEqual(args[3], 'redisjson-nightly-1-1-3')
                raise RuntimeError('partial apply')

            with patch.dict('os.environ', environment), patch(
                'redisbench_admin.utils.remote.fetch_remote_setup_from_config',
                return_value=(str(root), 'oss-standalone', 'test')
            ), patch('redisbench_admin.utils.remote.setup_remote_environment', side_effect=failed_apply):
                with self.assertRaisesRegex(RuntimeError, 'partial apply'):
                    provision(state, root, shard=3)
            with patch('python_terraform.Terraform') as terraform:
                terraform.return_value.destroy.return_value = (0, '', '')
                destroy(state)
                terraform.assert_called_once_with(working_dir=str(root))
                terraform.return_value.destroy.return_value = (1, '', 'failed')
                with self.assertRaisesRegex(RuntimeError, 'AWS teardown failed'):
                    destroy(state)

    @patch('compare_aws.collect_diagnostics')
    def test_remote_run_preserves_workload_collects_throughput_and_resets_after_failure(self, diagnostics):
        spec = Path(__file__).parent / 'json_set_fulldoc_api_replies_q3_gmaps_passiveassist.yml'
        config = yaml.safe_load(spec.read_text())
        inventory = dict(server_private_ip='10.0.0.1', server_public_ip='192.0.2.1',
                         client_public_ip='192.0.2.2', user='ubuntu', port=6379, server_cpulist='2')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            datasets = root / 'datasets'
            datasets.mkdir()

            def runner(command, **kwargs):
                self.assertIn('--inventory', command)
                self.assertIn('--keep_env_and_topo', command)
                self.assertEqual(kwargs['env']['BENCHMARK_RUNNER_GROUP_TOTAL'], '1')
                copied = yaml.safe_load((kwargs['cwd'] / 'test.yml').read_text())
                self.assertEqual(copied['clientconfig'], config['clientconfig'])
                self.assertNotIn('kpis', copied)
                self.assertEqual(copied['dbconfig'][:-1], config.get('dbconfig', []))
                self.assertEqual(copied['dbconfig'][-1], {'configuration-parameters': [{'server-cpulist': '2'}]})
                (kwargs['cwd'] / 'result.json').write_text(json.dumps({'Tests': {'Overall': {'rps': 100}}}))
                return subprocess.CompletedProcess(command, 0)

            with patch('compare_aws.subprocess.run', side_effect=runner), patch(
                'compare_aws.remote_commands', side_effect=[['redis_version:8.2.0\r\n'], ['']]
            ) as remote:
                value = run_one(spec, root / 'module.so', root / 'success', datasets,
                                inventory, root / 'key.pem', [{'type': 'oss-standalone'}])
                self.assertEqual(value['ops_per_sec'], 100)
                self.assertEqual(value['redis_version'], '8.2.0')
                self.assertEqual([call.args[-1] for call in diagnostics.call_args_list], ['before', 'after'])
                self.assertIn('shutdown nosave', remote.call_args.args[2][0])

            with patch('compare_aws.subprocess.run', return_value=subprocess.CompletedProcess([], 1)), patch(
                'compare_aws.remote_commands', return_value=['']
            ) as remote:
                with self.assertRaisesRegex(RuntimeError, 'Benchmark exited 1'):
                    run_one(spec, root / 'module.so', root / 'failure', datasets,
                            inventory, root / 'key.pem', [])
                self.assertIn('shutdown nosave', remote.call_args.args[2][0])

    def test_diagnostics_preserve_missing_telemetry_without_hiding_other_host(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch('compare_aws.remote_commands', side_effect=[
                RuntimeError('server snapshot unavailable'), ['{"timestamp": 123, "host": {}}']
            ]) as remote:
                collect_diagnostics(root, {'port': 6379}, root / 'key.pem', 'after')
            self.assertEqual(json.loads((root / 'diagnostics/server-after.json').read_text()),
                             {'error': 'server snapshot unavailable'})
            self.assertEqual(json.loads((root / 'diagnostics/client-after.json').read_text())['timestamp'], 123)
            self.assertEqual([call.args[-1] for call in remote.call_args_list],
                             ['server_public_ip', 'client_public_ip'])
            self.assertTrue(remote.call_args_list[0].args[2][0].endswith(' 6379'))
            self.assertFalse(remote.call_args_list[1].args[2][0].endswith(' 6379'))

    @patch('compare_aws.configure_affinity')
    def test_all_pairs_run_sequentially_and_reset_failure_marks_remaining_workloads(self, affinity):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'master/tests/benchmarks'
            (source / 'datasets').mkdir(parents=True)
            (source / 'defaults.yml').write_text('remote: []\n')
            for name in ('first', 'second'):
                (source / f'{name}.yml').write_text('name: test\n')
            for label in ('baseline', 'master'):
                module = root / label / 'target/release/librejson.so'
                module.parent.mkdir(parents=True)
                module.write_bytes(b'same-module')
            state = root / 'state.json'
            state.write_text('{}')
            args = argparse.Namespace(master_dir=root / 'master', baseline_dir=root / 'baseline',
                                      state=state, private_key=root / 'key', output=root / 'results', plan=None, shard=0)
            value = dict(ops_per_sec=100, redis_version='8.2.0')
            with patch('compare_aws.subprocess.check_output', return_value='same-sha\n'), patch(
                'compare_aws.run_one', return_value=value
            ) as run:
                self.assertEqual(compare(args), 0)
                self.assertEqual([(c.args[0].stem, c.args[2].name) for c in run.call_args_list],
                                 [('first', 'baseline'), ('first', 'master'),
                                  ('second', 'baseline'), ('second', 'master')])
                results = json.loads((args.output / 'comparison.json').read_text())
                self.assertEqual(results['revisions'], dict(baseline='same-sha', master='same-sha'))
                report = (args.output / 'summary.md').read_text()
                self.assertEqual(report.count('<th>Baseline</th><th>Master</th><th>Change %</th>'), 1)
                self.assertIn('Baseline module SHA256:', report)
                self.assertNotIn('<th>PR</th>', report)

            args.plan = root / 'plan.json'
            args.plan.write_text(json.dumps(dict(shards=[['second.yml']], modules=results['modules'],
                                                  revisions=results['revisions'])))
            args.output = root / 'shard-results'
            with patch('compare_aws.subprocess.check_output', side_effect=AssertionError('No git checkout needed')), patch(
                'compare_aws.run_one', return_value=value
            ) as run:
                self.assertEqual(compare(args), 0)
                self.assertEqual([(c.args[0].stem, c.args[2].name) for c in run.call_args_list],
                                 [('second', 'baseline'), ('second', 'master')])
            args.plan = None

            args.output = root / 'reset-failure'
            with patch('compare_aws.subprocess.check_output', return_value='same-sha\n'), patch(
                'compare_aws.run_one', side_effect=RemoteResetError('reset failed')
            ) as run:
                self.assertEqual(compare(args), 1)
                self.assertEqual(run.call_count, 1)
                results = json.loads((args.output / 'comparison.json').read_text())
                self.assertEqual(len(results['benchmarks']), 2)
                self.assertTrue(all('error' in value for pair in results['benchmarks'].values()
                                    for value in pair.values()))


if __name__ == '__main__':
    unittest.main()
