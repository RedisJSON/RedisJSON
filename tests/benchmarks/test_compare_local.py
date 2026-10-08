"""Validation failures must not become apparent memory/performance improvements."""

import unittest
from pathlib import Path
from unittest.mock import Mock

from compare_local import change, memory_snapshot, owns_server, summary, throughput


class ComparisonTest(unittest.TestCase):
    def test_memory_growth_and_peak_are_visible_in_report(self):
        baseline = dict(used_memory=1000, used_memory_dataset=500,
                        used_memory_peak=1200, used_memory_rss=4096,
                        keys=10, ops_per_sec=100)
        candidate = dict(baseline, used_memory_dataset=600, used_memory_peak=1800)
        report = summary({"modules": {"master": "aaa", "pr": "bbb"},
                          "benchmarks": {"JSON.SET": {"master": baseline, "pr": candidate}}})
        self.assertIn("used_memory_dataset | 500.00 | 600.00 | +20.00%", report)
        self.assertIn("used_memory_peak | 1,200.00 | 1,800.00 | +50.00%", report)

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
        with self.assertRaises(KeyError):
            throughput({"Tests": {}})
        with self.assertRaises(ValueError):
            throughput({"ALL STATS": {"Totals": {"Ops/sec": 100, "Connection Errors": 1}}})

    def test_error_cannot_look_like_a_successful_comparison(self):
        report = summary({"modules": {"master": "a", "pr": "b"},
                          "benchmarks": {"test": {"master": {"error": "missing result"},
                                                   "pr": {"error": "failed"}}}})
        self.assertIn("**ERROR**", report)
        self.assertNotIn("+0.00%", report)

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
