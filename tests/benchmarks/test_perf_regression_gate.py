#!/usr/bin/env python3
"""Unit tests for the performance-regression gate's decision logic.

These cover the parts that decide whether CI goes red, without needing a RedisTimeSeries
instance: metric-name normalisation, the TS.MRANGE reply shapes, and the ratchet itself.

Run with:  python3 -m unittest discover -s tests/benchmarks -p 'test_perf_*.py'
"""

import argparse
import json
import os
import tempfile
import unittest

import perf_regression_gate as gate


def make_args(**overrides):
    args = argparse.Namespace(
        window=3,
        threshold_pct=5.0,
        max_threshold_pct=25.0,
        noise_samples=10,
        baselines_file=None,
        branch="master",
        days=180,
        deployment_name="oss-standalone",
        triggering_env="circleci",
        github_org="RedisJSON",
        github_repo="RedisJSON",
        arch="x86_64",
        running_platform=None,
    )
    for key, value in overrides.items():
        setattr(args, key, value)
    return args


def points(values):
    """Turn a list of values into ascending-timestamp datapoints."""
    return [(1_700_000_000_000 + i * gate.DAY_MS, float(v)) for i, v in enumerate(values)]


class TestLeafMetricName(unittest.TestCase):
    def test_jsonpath_reduces_to_leaf(self):
        # The exporter labels series with the jsonpath leaf, so a full jsonpath must normalise.
        self.assertEqual(gate.leaf_metric_name("$.Tests.Overall.rps"), "rps")
        self.assertEqual(gate.leaf_metric_name("$.OverallRates.overallOpsRate"), "overallOpsRate")

    def test_bare_name_is_left_alone(self):
        self.assertEqual(gate.leaf_metric_name("Ops/sec"), "Ops/sec")

    def test_quoted_leaf_is_unwrapped(self):
        self.assertEqual(gate.leaf_metric_name('$."ALL STATS".*."Ops/sec"'), "Ops/sec")


class TestNormaliseMrange(unittest.TestCase):
    expected = [("k1", {"test_name": "t1"}, [(1, 10.0)])]

    def test_list_of_dicts(self):
        reply = [{"k1": [{"test_name": "t1"}, [(1, 10.0)]]}]
        self.assertEqual(gate.normalise_mrange(reply), self.expected)

    def test_flat_dict(self):
        reply = {"k1": [{"test_name": "t1"}, [(1, 10.0)]]}
        self.assertEqual(gate.normalise_mrange(reply), self.expected)

    def test_raw_resp2_label_pairs(self):
        reply = [["k1", [["test_name", "t1"]], [[1, 10.0]]]]
        self.assertEqual(gate.normalise_mrange(reply), self.expected)

    def test_unrecognised_shape_raises(self):
        # Must fail loudly: a silently-empty parse would look like a clean run.
        with self.assertRaises(TypeError):
            gate.normalise_mrange("nonsense")
        with self.assertRaises(TypeError):
            gate.normalise_mrange([("k1", "only-two")])


class TestBuildFilters(unittest.TestCase):
    def test_carries_the_labels_compare_uses(self):
        filters = gate.build_filters(make_args(), "Ops/sec")
        self.assertIn("branch=master", filters)
        self.assertIn("metric=Ops/sec", filters)
        self.assertIn("deployment_name=oss-standalone", filters)
        self.assertIn("triggering_env=circleci", filters)
        self.assertIn("github_org=RedisJSON", filters)
        self.assertIn("github_repo=RedisJSON", filters)

    def test_default_arch_is_not_filtered_on(self):
        # The arch label postdates the oldest series, so constraining it to the default would
        # silently drop the history the ratchet is built from.
        filters = gate.build_filters(make_args(arch=gate.ARCH_DEFAULT), "Ops/sec")
        self.assertFalse([f for f in filters if f.startswith("arch=")])

    def test_non_default_arch_is_filtered_on(self):
        filters = gate.build_filters(make_args(arch="aarch64"), "Ops/sec")
        self.assertIn("arch=aarch64", filters)

    def test_running_platform_is_optional(self):
        self.assertFalse(
            [f for f in gate.build_filters(make_args(), "Ops/sec") if "running_platform" in f]
        )
        self.assertIn(
            "running_platform=intel",
            gate.build_filters(make_args(running_platform="intel"), "Ops/sec"),
        )


class TestRollingStats(unittest.TestCase):
    def test_rolling_medians(self):
        self.assertEqual(gate.rolling_medians([1, 2, 3, 4], 3), [2, 3])

    def test_rolling_medians_too_short(self):
        self.assertEqual(gate.rolling_medians([1, 2], 3), [])

    def test_spread_pct_of_constant_series_is_zero(self):
        self.assertEqual(gate.spread_pct([100.0] * 5), 0.0)

    def test_spread_pct_detects_noise(self):
        self.assertGreater(gate.spread_pct([100.0, 130.0, 70.0, 120.0]), 10.0)


class TestEvaluateGroup(unittest.TestCase):
    def evaluate(self, values, **overrides):
        higher_better = overrides.pop("higher_better", True)
        approved = overrides.pop("approved", {})
        return gate.evaluate_group(
            "t1", "Ops/sec", points(values), make_args(**overrides), higher_better, approved
        )

    def test_stable_series_is_ok(self):
        res = self.evaluate([100, 101, 99, 100, 101, 99, 100])
        self.assertEqual(res["verdict"], gate.VERDICT_OK)

    def test_clear_drop_regresses(self):
        res = self.evaluate([100, 100, 100, 100, 80, 80, 80])
        self.assertEqual(res["verdict"], gate.VERDICT_REGRESSED)
        self.assertAlmostEqual(res["change_pct"], -20.0)

    def test_clear_rise_improves(self):
        res = self.evaluate([100, 100, 100, 100, 130, 130, 130])
        self.assertEqual(res["verdict"], gate.VERDICT_IMPROVED)

    def test_ratchet_does_not_follow_a_slow_run_down(self):
        # Performance was 100, dropped to 80 and stayed there. The baseline must remain the
        # earlier fast level rather than settling at the new normal, so this keeps failing.
        res = self.evaluate([100, 100, 100, 80, 80, 80, 80, 80, 80, 80, 80, 80])
        self.assertEqual(res["verdict"], gate.VERDICT_REGRESSED)
        self.assertEqual(res["baseline"], 100.0)

    def test_baseline_excludes_the_current_window(self):
        # The three newest runs form `current`; they must not raise their own baseline.
        res = self.evaluate([100, 100, 100, 100, 200, 200, 200])
        self.assertEqual(res["baseline"], 100.0)

    def test_insufficient_data_is_skipped_not_passed(self):
        res = self.evaluate([100, 100, 100, 100, 100])
        self.assertEqual(res["verdict"], gate.VERDICT_SKIPPED_NO_DATA)
        self.assertNotIn("change_pct", res)

    def test_single_outlier_does_not_set_the_baseline(self):
        # A lucky spike inside the history is absorbed by the median, so it cannot make every
        # later run look like a regression. This is what keeps the ratchet usable on noisy EC2.
        res = self.evaluate([100, 100, 160, 100, 100, 100, 100, 100, 100])
        self.assertEqual(res["verdict"], gate.VERDICT_OK)
        self.assertLess(res["baseline"], 160.0)

    def test_noisy_series_is_reported_as_not_gateable(self):
        res = self.evaluate(
            [100, 40, 160, 50, 150, 45, 155, 60, 140, 50, 150, 40], max_threshold_pct=25.0
        )
        self.assertEqual(res["verdict"], gate.VERDICT_SKIPPED_UNSTABLE)

    def test_noise_floor_widens_the_threshold(self):
        # History sits at 100 but carries a +-12% excursion, so its measured spread is ~6%.
        # A 5.5% drop is past the nominal 5% threshold yet still inside that noise floor, so it
        # must not fire -- otherwise every noisy test reports a regression every run.
        history = [100, 100, 100, 100, 100, 100, 112, 88, 100]
        res = self.evaluate(history + [94.5, 94.5, 94.5])
        self.assertEqual(res["baseline"], 100.0)
        self.assertAlmostEqual(res["change_pct"], -5.5)
        self.assertGreater(res["effective_threshold_pct"], res["threshold_pct"])
        self.assertEqual(res["verdict"], gate.VERDICT_OK)

    def test_lower_better_metric_inverts(self):
        # Latency: rising is bad, and the ratchet keeps the lowest sustained value.
        res = self.evaluate([10, 10, 10, 10, 20, 20, 20], higher_better=False)
        self.assertEqual(res["verdict"], gate.VERDICT_REGRESSED)
        self.assertEqual(res["baseline"], 10.0)

    def test_approved_baseline_lowers_the_bar(self):
        approved = {"t1|Ops/sec": {"baseline": 80.0, "approved_on": "2026-07-30"}}
        res = self.evaluate([100, 100, 100, 100, 80, 80, 80], approved=approved)
        self.assertEqual(res["verdict"], gate.VERDICT_OK)
        self.assertEqual(res["baseline"], 80.0)
        self.assertTrue(res["baseline_source"].startswith("approved:"))
        # The derived value is still reported, so the accepted cost stays visible.
        self.assertEqual(res["derived_baseline"], 100.0)

    def test_approved_threshold_override(self):
        approved = {"t1|Ops/sec": {"threshold_pct": 30.0}}
        res = self.evaluate([100, 100, 100, 100, 80, 80, 80], approved=approved)
        self.assertEqual(res["verdict"], gate.VERDICT_OK)
        self.assertEqual(res["baseline"], 100.0)


class TestLoadApprovedBaselines(unittest.TestCase):
    def load(self, doc):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fd:
            json.dump(doc, fd)
            path = fd.name
        self.addCleanup(os.unlink, path)
        return gate.load_approved_baselines(path)

    def test_missing_file_is_empty(self):
        self.assertEqual(gate.load_approved_baselines("/nonexistent/perf-baselines.json"), {})

    def test_entries_are_keyed_by_test_and_metric(self):
        approved = self.load(
            {"baselines": [{"test_name": "t1", "metric": "Ops/sec", "baseline": 5}]}
        )
        self.assertIn("t1|Ops/sec", approved)

    def test_entry_missing_identity_is_rejected(self):
        with self.assertRaises(ValueError):
            self.load({"baselines": [{"metric": "Ops/sec", "baseline": 5}]})

    def test_entry_with_no_effect_is_rejected(self):
        with self.assertRaises(ValueError):
            self.load({"baselines": [{"test_name": "t1", "metric": "Ops/sec"}]})


class TestLoadDefaults(unittest.TestCase):
    def test_reads_and_normalises_the_repo_defaults(self):
        here = os.path.dirname(os.path.abspath(__file__))
        metrics, mode = gate.load_defaults(os.path.join(here, "defaults.yml"))
        self.assertEqual(mode, "higher-better")
        self.assertIn("Ops/sec", metrics)
        # Nothing may reach the query as a raw jsonpath, or it would match no series.
        self.assertFalse([m for m in metrics if m.startswith("$")])


class TestRenderSummary(unittest.TestCase):
    def test_regression_table_names_the_approval_file(self):
        results = [
            {
                "test_name": "t1",
                "metric": "Ops/sec",
                "verdict": gate.VERDICT_REGRESSED,
                "baseline": 100.0,
                "current": 80.0,
                "change_pct": -20.0,
                "effective_threshold_pct": 5.0,
                "baseline_source": "derived:best-trailing-median",
                "samples": 9,
            }
        ]
        args = make_args(baselines_file="tests/benchmarks/perf-baselines.json")
        text = gate.render_summary(results, args, enforced=True)
        self.assertIn("Regressions", text)
        self.assertIn("-20.0%", text)
        self.assertIn("perf-baselines.json", text)

    def test_warn_only_mode_is_stated(self):
        args = make_args(baselines_file="x.json")
        text = gate.render_summary([], args, enforced=False)
        self.assertIn("warn-only", text)


if __name__ == "__main__":
    unittest.main()
