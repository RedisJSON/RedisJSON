"""Validation failures must not become apparent performance improvements."""

import unittest
import tempfile
import contextlib
import io
from xml.etree import ElementTree
import json
from pathlib import Path
from unittest.mock import Mock, patch

from compare_local import (main, change, escape_dataset_unicode, owns_server, summary, throughput)


class ComparisonTest(unittest.TestCase):
    def test_ci_retries_both_revisions_and_marks_last_pair_or_error(self):
        for rates, attempts in [([100, 110, 100, 101], 2), ([100, 90, 100, 99], 2),
                                ([100, 105], 1), ([100, 95], 1), ([100, 110] * 4, 4),
                                ([100, 110, 100, None], 2)]:
            with self.subTest(rates=rates), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / 'datasets').mkdir()
                (root / 'test.yml').write_text('name: test\n')
                module = root / 'module.so'
                module.write_bytes(b'module')
                output = root / 'results'
                remaining = iter(rates)
                def run(spec, module, directory, *args):
                    directory.mkdir(parents=True)
                    rate = next(remaining)
                    (directory / 'runner.log').write_text(str(rate))
                    if rate is None:
                        raise RuntimeError('client failed')
                    return {'ops_per_sec': rate}
                argv = ['compare_local.py', '--baseline-module', str(module), '--candidate-module', str(module),
                        '--benchmarks-dir', str(root), '--output', str(output)]
                with patch('sys.argv', argv), patch('compare_local.shutil.which', side_effect=lambda x: x), patch(
                    'compare_local.subprocess.check_output', return_value='Redis 8.6'
                ), patch('compare_local.run_one', side_effect=run) as runner, contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(main(), int(None in rates))
                self.assertEqual([c.args[2].name for c in runner.call_args_list], ['master', 'pr'] * attempts)
                data = json.loads((output / 'comparison.json').read_text())
                self.assertEqual(data['benchmarks']['test']['pr']['attempt_count'], attempts)
                history = output / 'test/attempts'
                self.assertEqual(len(list(history.glob('*.json'))), attempts)
                report = (output / 'summary.md').read_text()
                self.assertEqual('🟠 test' in report, attempts > 1)
                if attempts > 1:
                    self.assertEqual((history / 'attempt-1/pr/runner.log').read_text(), str(rates[1]))
                if None in rates:
                    self.assertIn('ERROR', report)
                else:
                    self.assertEqual(data['benchmarks']['test']['pr']['ops_per_sec'], rates[-1])

    def test_unicode_payloads_keep_exact_json_values(self):
        text = '{ "temperature": "10°", "name": "שלום😀", "number": 1.2300e-2 }'
        escaped = escape_dataset_unicode(text)
        self.assertTrue(escaped.isascii())
        self.assertEqual(json.loads(escaped), json.loads(text))
        self.assertIn('1.2300e-2', escaped)
        self.assertEqual(escape_dataset_unicode(escaped), escaped)
        for name in ('api_replies_q3_gmaps_passiveassist.json', 'api_replies_q5_gmaps_place.json'):
            original = (Path(__file__).parent / 'datasets' / name).read_text(encoding='utf-8')
            escaped = escape_dataset_unicode(original)
            self.assertTrue(escaped.isascii())
            self.assertEqual(json.loads(original), json.loads(escaped))

    def test_percentage_marker_thresholds_for_throughput(self):
        for delta, expected in ((-10, '🟢'), (0, ''), (0.1, '🟡'), (4.99, '🟡'),
                                (5, '🔴'), (5.01, '🔴'), (10, '🔴')):
            with self.subTest(degradation=delta):
                baseline = dict(ops_per_sec=10000)
                candidate = dict(ops_per_sec=10000 - delta * 100)
                report = summary({'modules': {'master': 'a', 'pr': 'b'},
                                  'benchmarks': {'test': {'master': baseline, 'pr': candidate}}})
                table = ElementTree.fromstring(report[report.index('<table>'):])
                cells = [cell.text for cell in table.find('tbody/tr')]
                self.assertEqual(len(cells), 4)
                self.assertNotIn('memory', report.lower())
                for index, cell in enumerate(cells):
                    markers = ''.join(c for c in cell if c in '🟢🟡🔴')
                    self.assertEqual(markers, expected if index == 3 else '')

    def test_both_client_formats_and_invalid_results(self):
        self.assertEqual(throughput({"Tests": {"Overall": {"rps": "123.5"}}}), 123.5)
        self.assertEqual(throughput({"ALL STATS": {"Totals": {"Ops/sec": 456}}}), 456)
        for value in (0, -1, "nan", "inf"):
            with self.assertRaises(ValueError):
                throughput({"Tests": {"Overall": {"rps": value}}})
        for result in ({"Tests": {}}, {"Tests": {"Overall": {}}}):
            with self.assertRaisesRegex(ValueError, "check runner.log"):
                throughput(result)
        with self.assertRaises(ValueError):
            throughput({"ALL STATS": {"Totals": {"Ops/sec": 100, "Connection Errors": 1}}})

    def test_error_cannot_look_like_a_successful_comparison(self):
        report = summary({"modules": {"master": "a", "pr": "b"},
                          "benchmarks": {"test <unsafe>": {"master": {"error": "missing <result>"},
                                                   "pr": {"error": "failed"}}}})
        self.assertIn("<strong>ERROR</strong>", report)
        self.assertNotIn("+0.00%", report)
        table = ElementTree.fromstring(report[report.index("<table>"):])
        row = table.find("tbody/tr")
        self.assertEqual(row[0].text, "🔴 test <unsafe>")
        self.assertEqual(row[1].get("colspan"), "3")
        self.assertIn("missing &lt;result&gt;", report)

    def test_zero_baseline(self):
        self.assertEqual(change(0, 0), "0.00%")
        self.assertEqual(change(0, 10), "n/a (zero baseline)")

    def test_cleanup_only_owns_its_own_database_directory(self):
        connection = Mock()
        connection.config_get.return_value = {"dir": "/tmp/other-redis"}
        self.assertFalse(owns_server(connection, Path("/tmp/our-redis")))
        connection.config_get.return_value = {"dir": "/tmp/our-redis/run"}
        self.assertTrue(owns_server(connection, Path("/tmp/our-redis")))


if __name__ == "__main__":
    unittest.main()
