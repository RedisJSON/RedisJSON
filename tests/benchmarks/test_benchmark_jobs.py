import copy
import json
from pathlib import Path
import tempfile
import unittest

from benchmark_jobs import make_plan, merge_results


class BenchmarkJobsTest(unittest.TestCase):
    def test_full_suite_is_assigned_once_and_deterministically(self):
        source = Path(__file__).parent
        plan = make_plan(source, 5)
        names = [name for shard in plan['shards'] for name in shard]
        expected = {p.name for p in source.glob('*.yml') if p.name != 'defaults.yml'}
        self.assertEqual(set(names), expected)
        self.assertEqual(len(names), len(expected))
        self.assertTrue(all(plan['shards']))
        self.assertEqual(plan, make_plan(source, 5))

    def test_measured_durations_balance_heavy_tests_across_jobs(self):
        source = Path(__file__).parent
        specs = sorted(p for p in source.glob('*.yml') if p.name != 'defaults.yml')
        timings = {'benchmarks': {
            spec.stem: {label: {'run_seconds': 1000 if index < 5 else 1}
                        for label in ('master', 'pr')}
            for index, spec in enumerate(specs)
        }}
        plan = make_plan(source, 5, timings)
        heavy = {spec.name for spec in specs[:5]}
        self.assertTrue(all(len(set(shard) & heavy) == 1 for shard in plan['shards']))

    def test_merge_detects_missing_partial_failed_duplicate_and_mismatched_results(self):
        plan = {'shards': [[f'test-{index}.yml'] for index in range(5)],
                'modules': {'master': 'aaa', 'pr': 'bbb'}, 'redis': 'same-server'}
        measurement = dict(ops_per_sec=100, used_memory=10, used_memory_dataset=5,
                           used_memory_peak=20, used_memory_rss=30, keys=1)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = []
            for index in range(5):
                path = root / f'shard-{index}' / 'comparison.json'
                path.parent.mkdir()
                data = {key: plan[key] for key in ('modules', 'redis')}
                data['benchmarks'] = {f'test-{index}': {'master': measurement, 'pr': measurement}}
                path.write_text(json.dumps(data))
                paths.append(path)
            merged, failed = merge_results(plan, root)
            self.assertFalse(failed)
            self.assertEqual(len(merged['benchmarks']), 5)
            original = paths[0].read_text()
            for corruption in ('missing', 'partial', 'failed', 'wrong-module', 'extra', 'invalid-metric', 'broken-json', 'invalid-object'):
                with self.subTest(corruption=corruption):
                    data = json.loads(original)
                    if corruption == 'missing':
                        paths[0].unlink()
                    elif corruption == 'broken-json':
                        paths[0].write_text('{')
                    else:
                        if corruption == 'partial':
                            del data['benchmarks']['test-0']['pr']
                        elif corruption == 'failed':
                            data['benchmarks']['test-0']['pr'] = {'error': 'client failed'}
                        elif corruption == 'wrong-module':
                            data['modules']['pr'] = 'different-build'
                        elif corruption == 'invalid-object':
                            data['benchmarks'] = ['test-0']
                        elif corruption == 'extra':
                            data['benchmarks']['unexpected'] = {}
                        else:
                            data['benchmarks']['test-0']['pr']['ops_per_sec'] = 0
                        paths[0].write_text(json.dumps(data))
                    merged, failed = merge_results(plan, root)
                    self.assertTrue(failed)
                    self.assertEqual(len(merged['benchmarks']), 5)
                    self.assertIn('error', merged['benchmarks']['test-0']['pr'])
                    paths[0].write_text(original)
            duplicate = copy.deepcopy(plan)
            duplicate['shards'][1] = duplicate['shards'][0]
            with self.assertRaisesRegex(ValueError, 'Duplicate assignment'):
                merge_results(duplicate, root)


if __name__ == '__main__':
    unittest.main()
