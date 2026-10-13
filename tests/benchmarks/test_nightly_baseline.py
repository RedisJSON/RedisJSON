import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from nightly_baseline import evaluate, resolve, main


class NightlyBaselineTest(unittest.TestCase):
    def result(self, *rates):
        return {'benchmarks': {str(i): {'baseline': {'ops_per_sec': 100},
                                      'master': {'ops_per_sec': rate}}
                               for i, rate in enumerate(rates)}, 'merge_errors': []}

    def test_promotion_requires_improvement_without_regressions_and_complete_results(self):
        for rates, eligible in [([106, 110], True), ([105], False), ([100], False),
                                ([200, 95], False), ([120, 96], True)]:
            with self.subTest(rates=rates):
                self.assertEqual(evaluate(self.result(*rates))[0], eligible)
        for rate in [0, float('nan'), float('inf'), None]:
            self.assertFalse(evaluate(self.result(rate))[0])
        self.assertFalse(evaluate(self.result())[0])
        broken = self.result(110)
        broken['merge_errors'] = ['Missing shard']
        self.assertFalse(evaluate(broken)[0])
        broken = self.result(110)
        broken['benchmarks']['0']['baseline'] = {'error': 'failure'}
        self.assertFalse(evaluate(broken)[0])

    def test_geometric_mean_gives_each_workload_equal_weight(self):
        value = self.result(120, 100)
        scaled = copy.deepcopy(value)
        for result in scaled['benchmarks']['0'].values():
            result['ops_per_sec'] *= 100000
        self.assertEqual(evaluate(value)[0], evaluate(scaled)[0])

    def test_confirmation_rejects_changed_binaries_revisions_server_or_coverage(self):
        with tempfile.TemporaryDirectory() as tmp:
            previous = Path(tmp) / 'initial.json'
            current = Path(tmp) / 'confirmation.json'
            value = self.result(110)
            value.update(modules={'baseline': 'a', 'master': 'b'},
                         revisions={'baseline': 'c', 'master': 'd'}, redis='8.6.0')
            previous.write_text(json.dumps(value))
            for key in ('modules', 'revisions', 'redis', 'benchmarks'):
                changed = copy.deepcopy(value)
                changed[key] = {} if key != 'redis' else 'different'
                current.write_text(json.dumps(changed))
                with patch('sys.argv', ['baseline', 'evaluate', str(current), '--previous', str(previous)]):
                    with self.assertRaises(ValueError):
                        main()

    def test_manual_pins_dispatch_sha_and_schedule_resolves_master_and_skips_equal(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / 'output'
            summary = Path(tmp) / 'summary'
            env = {'GITHUB_REPOSITORY': 'RedisJSON/RedisJSON', 'GITHUB_EVENT_NAME': 'workflow_dispatch',
                   'GITHUB_SHA': 'manual-sha', 'GITHUB_OUTPUT': str(output), 'GITHUB_STEP_SUMMARY': str(summary)}
            with patch.dict(os.environ, env), patch('nightly_baseline.subprocess.check_output',
                                                   return_value=json.dumps({'object': {'sha': 'base-sha'}})) as api:
                resolve()
                self.assertEqual(api.call_count, 1)
                self.assertIn('candidate=manual-sha', output.read_text())
                self.assertIn('skip=false', output.read_text())
            output.write_text('')
            env['GITHUB_EVENT_NAME'] = 'schedule'
            with patch.dict(os.environ, env), patch('nightly_baseline.subprocess.check_output',
                                                   return_value=json.dumps({'object': {'sha': 'same-sha'}})) as api:
                resolve()
                self.assertEqual(api.call_count, 2)
                self.assertIn('skip=true', output.read_text())
