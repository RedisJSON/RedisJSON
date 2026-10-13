import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import yaml

from compare_aws import RemoteResetError, compare, destroy, provision, run_one


class AWSComparisonTest(unittest.TestCase):
    def test_partial_provision_retains_state_for_cleanup_and_cleanup_errors_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'defaults.yml').write_text('remote: []\n')
            state = root / 'state.json'
            environment = dict(GITHUB_REPOSITORY='RedisJSON/RedisJSON', GITHUB_SHA='abc',
                               GITHUB_ACTOR='tester', GITHUB_RUN_ID='1', GITHUB_RUN_ATTEMPT='1')

            def failed_apply(*args):
                self.assertEqual(json.loads(state.read_text())['terraform_dir'], str(root))
                raise RuntimeError('partial apply')

            with patch.dict('os.environ', environment), patch(
                'redisbench_admin.utils.remote.fetch_remote_setup_from_config',
                return_value=(str(root), 'oss-standalone', 'test')
            ), patch('redisbench_admin.utils.remote.setup_remote_environment', side_effect=failed_apply):
                with self.assertRaisesRegex(RuntimeError, 'partial apply'):
                    provision(state, root)
            with patch('python_terraform.Terraform') as terraform:
                terraform.return_value.destroy.return_value = (0, '', '')
                destroy(state)
                terraform.assert_called_once_with(working_dir=str(root))
                terraform.return_value.destroy.return_value = (1, '', 'failed')
                with self.assertRaisesRegex(RuntimeError, 'AWS teardown failed'):
                    destroy(state)

    def test_remote_run_preserves_workload_collects_throughput_and_resets_after_failure(self):
        spec = Path(__file__).parent / 'json_set_fulldoc_api_replies_q3_gmaps_passiveassist.yml'
        config = yaml.safe_load(spec.read_text())
        inventory = dict(server_private_ip='10.0.0.1', server_public_ip='192.0.2.1',
                         client_public_ip='192.0.2.2', user='ubuntu', port=6379)
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
                (kwargs['cwd'] / 'result.json').write_text(json.dumps({'Tests': {'Overall': {'rps': 100}}}))
                return subprocess.CompletedProcess(command, 0)

            with patch('compare_aws.subprocess.run', side_effect=runner), patch(
                'compare_aws.remote_commands', side_effect=[['redis_version:8.2.0\r\n'], ['']]
            ) as remote:
                value = run_one(spec, root / 'module.so', root / 'success', datasets,
                                inventory, root / 'key.pem', [{'type': 'oss-standalone'}])
                self.assertEqual(value['ops_per_sec'], 100)
                self.assertEqual(value['redis_version'], '8.2.0')
                self.assertIn('shutdown nosave', remote.call_args.args[2][0])

            with patch('compare_aws.subprocess.run', return_value=subprocess.CompletedProcess([], 1)), patch(
                'compare_aws.remote_commands', return_value=['']
            ) as remote:
                with self.assertRaisesRegex(RuntimeError, 'Benchmark exited 1'):
                    run_one(spec, root / 'module.so', root / 'failure', datasets,
                            inventory, root / 'key.pem', [])
                self.assertIn('shutdown nosave', remote.call_args.args[2][0])

    def test_all_pairs_run_sequentially_and_reset_failure_marks_remaining_workloads(self):
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
                                      state=state, private_key=root / 'key', output=root / 'results')
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
