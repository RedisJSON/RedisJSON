import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import jsonpath_benchmark_gate
from jsonpath_benchmark_gate import (
    confirmed_regressions,
    read_comparison,
    regression_filter,
    render_summary,
)


class BenchmarkGateTests(unittest.TestCase):
    def comparison(self, candidate, baseline=None, commits=("base", "head")):
        baseline = baseline or {name: 100 for name in candidate}
        entries = [
            {
                "commit": {"id": commit},
                "benches": [
                    {"name": name, "value": value, "unit": "ns/iter"}
                    for name, value in values.items()
                ],
            }
            for commit, values in zip(commits, [baseline, candidate])
        ]
        path = Path(self.directory.name) / f"comparison-{self.counter}.json"
        self.counter += 1
        path.write_text(json.dumps({"entries": {"JSONPath": entries}}))
        return path

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.counter = 0

    def test_exactly_five_percent_must_repeat_on_same_workload(self):
        first = self.comparison({"small": 105, "other": 100})
        for repeated, expected in [
            ({"small": 105}, ["small"]),
            ({"small": 104.99}, []),
            ({"small": 100, "other": 110}, []),
        ]:
            with self.subTest(repeated=repeated):
                self.assertEqual(
                    confirmed_regressions(first, self.comparison(repeated)), expected
                )

    def test_decimal_boundary_and_filter_escape(self):
        first = self.comparison(
            {"eval/a+b": 0.21, "fast": 104.99}, {"eval/a+b": 0.2, "fast": 100}
        )
        self.assertEqual(regression_filter(read_comparison(first)[1]), r"^(eval/a\+b)$")
        below = self.comparison({"fast": 104.99})
        self.assertEqual(regression_filter(read_comparison(below)[1]), "")

    def test_missing_rerun_workload_and_wrong_commits_cannot_pass(self):
        first = self.comparison({"small": 105})
        for second in [
            self.comparison({"other": 105}),
            self.comparison({"small": 105}, commits=("different", "head")),
        ]:
            with self.subTest(second=second), self.assertRaises(ValueError):
                confirmed_regressions(first, second)

    def test_incomplete_or_invalid_measurements_cannot_pass(self):
        for path in [
            self.comparison({}, {"small": 100}),
            self.comparison({"small": 105}, {"small": 0}),
            self.comparison({"small": float("nan")}),
            self.comparison({"small": 100, "new": float("nan")}, {"small": 100}),
            self.comparison({"small": 105}, commits=("same", "same")),
        ]:
            with self.subTest(path=path), self.assertRaises(ValueError):
                read_comparison(path)

    def test_added_and_removed_workloads_preserve_existing_comparisons(self):
        first = self.comparison(
            {"existing": 110, "new": 500}, {"existing": 100, "removed": 100}
        )
        self.assertEqual(read_comparison(first)[1], {"existing": 10})
        self.assertEqual(
            confirmed_regressions(first, self.comparison({"existing": 105})),
            ["existing"],
        )
        summary = render_summary(first)
        self.assertIn("Compared **1**", summary)
        self.assertIn("1** without a master baseline", summary)
        self.assertIn("+10.00%", summary)
        self.assertIn("No master baseline", summary)
        self.assertNotIn("`removed`", summary)

    def test_disjoint_workloads_do_not_claim_a_comparison(self):
        path = self.comparison({"new": 100}, {"old": 100})
        self.assertEqual(read_comparison(path)[1], {})
        self.assertIn("No matching master baseline", render_summary(path))

    def test_master_cache_selection_ignores_prs_and_suite_hash_changes(self):
        prefix = "jsonpath-baseline-v1-ubuntu-24.04-X64-"
        master = prefix + "old-suite-hash-0.8.2-100-1"
        caches = [
            {"ref": "refs/pull/1661/merge", "key": prefix + "new-suite-hash-0.8.2-300-1"},
            {"ref": "refs/heads/master", "key": prefix + "master-0.9.0-200-1"},
            {"ref": "refs/heads/master", "key": master},
            {"ref": "refs/heads/master", "key": prefix + "master-0.8.2-90-1"},
        ]
        with patch("subprocess.check_output", return_value=json.dumps(caches)) as request:
            self.assertEqual(
                jsonpath_benchmark_gate.master_baseline_key("RedisJSON/RedisJSON", prefix, "0.8.2"),
                master,
            )
            command = request.call_args.args[0]
            self.assertEqual(command[command.index("--ref") + 1], "refs/heads/master")
            self.assertEqual(command[command.index("--sort") + 1], "created_at")
            self.assertEqual(command[command.index("--order") + 1], "desc")
        with patch("subprocess.check_output", return_value=json.dumps(caches[:2])):
            self.assertEqual(
                jsonpath_benchmark_gate.master_baseline_key("RedisJSON/RedisJSON", prefix, "0.8.2"),
                "",
            )

    def test_incompatible_units_and_duplicate_workloads_cannot_pass(self):
        path = self.comparison({"small": 105})
        data = json.loads(path.read_text())
        benches = data["entries"]["JSONPath"][1]["benches"]
        benches[0]["unit"] = "us/iter"
        path.write_text(json.dumps(data))
        with self.assertRaises(ValueError):
            read_comparison(path)
        benches[0]["unit"] = "ns/iter"
        benches.append(benches[0].copy())
        path.write_text(json.dumps(data))
        with self.assertRaises(ValueError):
            read_comparison(path)

    def test_cli_selects_then_fails_only_confirmed_regressions(self):
        script = Path(__file__).with_name("jsonpath_benchmark_gate.py")
        first = self.comparison({"small": 105})
        output = Path(self.directory.name) / "output"
        env = {
            **os.environ,
            "GITHUB_OUTPUT": str(output),
            "GITHUB_STEP_SUMMARY": str(output.with_name("summary")),
        }
        select = subprocess.run(
            [sys.executable, str(script), str(first)], env=env, capture_output=True
        )
        self.assertEqual(select.returncode, 0, select.stderr)
        self.assertEqual(output.read_text(), "filter=^(small)$\n")
        for value, expected in [(105, 1), (104.99, 0)]:
            with self.subTest(value=value):
                result = subprocess.run(
                    [
                        sys.executable, str(script), str(first),
                        "--confirmation", str(self.comparison({"small": value})),
                    ],
                    env=env,
                    capture_output=True,
                )
                self.assertEqual(result.returncode, expected, result.stderr)

    def test_summary_distinguishes_confirmed_unconfirmed_and_faster_results(self):
        first = self.comparison({"slow": 110, "noise": 106, "fast": 80, "stable": 101})
        pending = render_summary(first)
        self.assertIn("Awaiting confirmation", pending)
        summary = render_summary(first, self.comparison({"slow": 105, "noise": 100}))
        self.assertIn("Confirmed ≥5%", summary)
        self.assertIn("Not reproduced", summary)
        self.assertIn("-20.00%", summary)
        self.assertIn("🟩🟩🟩🟩", summary)
        self.assertIn("Within 5%", summary)
        self.assertIn("All 4 workloads", summary)

    def test_summary_without_baseline_does_not_claim_a_comparison(self):
        path = self.comparison({"eval/a|b": 123})
        data = json.loads(path.read_text())
        data["entries"]["JSONPath"] = data["entries"]["JSONPath"][-1:]
        path.write_text(json.dumps(data))
        summary = render_summary(path)
        self.assertIn("No matching master baseline", summary)
        self.assertIn("No master baseline", summary)
        self.assertIn("123.00 ns/iter", summary)
        self.assertIn("eval/a&#124;b", summary)
        self.assertNotIn("Confirmed", summary)
        with self.assertRaises(ValueError):
            render_summary(path, self.comparison({"eval/a|b": 125}))

    def test_summary_cli_writes_job_summary_without_changing_gate_result(self):
        script = Path(__file__).with_name("jsonpath_benchmark_gate.py")
        summary = Path(self.directory.name) / "summary"
        result = subprocess.run(
            [sys.executable, str(script), str(self.comparison({"slow": 110})),
             "--summary", "--confirmation", str(self.comparison({"slow": 111}))],
            env={**os.environ, "GITHUB_STEP_SUMMARY": str(summary)},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Confirmed ≥5%", summary.read_text())
        self.assertEqual(result.stdout.strip(), summary.read_text().strip())


if __name__ == "__main__":
    unittest.main()
