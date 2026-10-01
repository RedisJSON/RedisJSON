import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

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
            self.comparison({"small": 105}, {"other": 100}),
            self.comparison({"small": 105}, {"small": 0}),
            self.comparison({"small": float("nan")}),
            self.comparison({"small": 105}, commits=("same", "same")),
        ]:
            with self.subTest(path=path), self.assertRaises(ValueError):
                read_comparison(path)

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
        self.assertIn("No compatible baseline", summary)
        self.assertIn("Baseline only", summary)
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
