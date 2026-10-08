"""Validation failures must not become apparent memory/performance improvements."""

import unittest
from xml.etree import ElementTree
import json
from pathlib import Path
from unittest.mock import Mock

from compare_local import (change, escape_dataset_unicode, memory_snapshot,
                           owns_server, summary, throughput)


class ComparisonTest(unittest.TestCase):
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

    def test_memory_growth_and_peak_are_visible_in_report(self):
        baseline = dict(used_memory=1000, used_memory_dataset=500,
                        used_memory_peak=1200, used_memory_rss=4096,
                        keys=10, ops_per_sec=100)
        candidate = dict(baseline, used_memory_dataset=600, used_memory_peak=1800)
        report = summary({"modules": {"master": "aaa", "pr": "bbb"},
                          "benchmarks": {"JSON.SET": {"master": baseline, "pr": candidate}}})
        table = ElementTree.fromstring(report[report.index('<table>'):])
        cells = [cell.text for cell in table.find('tbody/tr')]
        self.assertEqual(cells[7:10], ['500', '600', '🔴 +20.00%'])
        self.assertEqual(cells[10:13], ['1,200', '1,800', '🔴 +50.00%'])

    def test_red_markers_follow_metric_direction_and_ignore_key_counts(self):
        baseline = dict(used_memory=100, used_memory_dataset=0, used_memory_peak=100,
                        used_memory_rss=100, keys=10, ops_per_sec=100)
        candidate = dict(baseline, used_memory=90, used_memory_dataset=10,
                         used_memory_peak=110, keys=20, ops_per_sec=90)
        report = summary({"modules": {"master": "a", "pr": "b"},
                          "benchmarks": {"test": {"master": baseline, "pr": candidate}}})
        table = ElementTree.fromstring(report[report.index('<table>'):])
        rows = table.findall('tbody/tr')
        self.assertEqual(len(rows), 1)
        cells = [cell.text for cell in rows[0]]
        self.assertEqual(len(cells), 19)
        self.assertEqual([index for index, cell in enumerate(cells) if '🔴' in cell], [3, 9, 12])
        self.assertEqual(cells[4:7], ['100', '90', '🟢 -10.00%'])
        self.assertEqual(cells[16:19], ['10', '20', '+100.00%'])
        headers = table.findall('thead/tr')
        self.assertEqual([cell.get('colspan') for cell in headers[0]][1:], ['3'] * 6)
        self.assertEqual([cell.text for cell in headers[1]], ['Master', 'PR', 'Change %'] * 6)

    def test_percentage_marker_thresholds_for_throughput_and_memory(self):
        for delta, expected in ((-10, '🟢'), (0, ''), (0.1, '🟡'), (4.99, '🟡'),
                                (5, '🔴'), (5.01, '🔴'), (10, '🔴')):
            with self.subTest(degradation=delta):
                baseline = dict(used_memory=10000, used_memory_dataset=10000,
                                used_memory_peak=10000, used_memory_rss=10000,
                                keys=10, ops_per_sec=10000)
                candidate = {key: value + delta * 100 for key, value in baseline.items()}
                candidate['ops_per_sec'] = 10000 - delta * 100
                report = summary({'modules': {'master': 'a', 'pr': 'b'},
                                  'benchmarks': {'test': {'master': baseline, 'pr': candidate}}})
                table = ElementTree.fromstring(report[report.index('<table>'):])
                cells = [cell.text for cell in table.find('tbody/tr')]
                for index, cell in enumerate(cells):
                    markers = ''.join(c for c in cell if c in '🟢🟡🔴')
                    self.assertEqual(markers, expected if index in (3, 6, 9, 12, 15) else '')

    def test_missing_memory_is_not_reported_as_zero(self):
        connection = Mock()
        connection.info.return_value = {"used_memory": 100}
        with self.assertRaises(KeyError):
            memory_snapshot(connection)

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
        self.assertEqual(row[1].get("colspan"), "18")
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
