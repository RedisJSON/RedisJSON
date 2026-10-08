import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from jsonpath_benchmark_report import render_summary


class BenchmarkReportTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.results = Path(directory.name) / "results.jsonl"
        self.paths = Path(directory.name) / "paths.jsonl"
        self.baseline = Path(directory.name) / "master.jsonl"
        self.baseline_paths = Path(directory.name) / "master-paths.jsonl"
        self.row = {
            "version": "7",
            "kind": "LibraryBenchmark",
            "group": "jsonpath",
            "function_name": "eval_path",
            "id": "out_of_range_index",
            "profiles": [{
                "tool": "Callgrind",
                "data": {"total": {"regressions": [], "metrics": {
                    key: {"values": {"new": value, "old": value}}
                    for key, value in {
                        "Ir": 1234, "Dr": 456, "Dw": 0, "EstimatedCycles": 2345,
                    }.items()
                }}},
            }],
        }
        self.write_results(self.row)
        self.write_paths({"name": "eval/out-of-range-index", "path": "$.numbers[128]"})
        self.save_baseline()

    def write_results(self, *rows):
        self.results.write_text("".join(json.dumps(row) + "\n" for row in rows))

    def write_paths(self, *rows):
        self.paths.write_text("".join(json.dumps(row) + "\n" for row in rows))

    def save_baseline(self):
        rows = [json.loads(line) for line in self.results.read_text().splitlines()]
        for row in rows:
            total = row["profiles"][0]["data"]["total"]
            total["regressions"] = []
            for metric in total["metrics"].values():
                metric["values"] = {"new": metric["values"]["old"]}
        self.baseline.write_text("".join(json.dumps(row) + "\n" for row in rows))
        self.baseline_paths.write_text(self.paths.read_text())

    def summary(self):
        return render_summary(self.results, self.paths, self.baseline, self.baseline_paths)

    def test_comparison_reports_all_workloads_and_native_counts(self):
        compile_row = {**self.row, "function_name": "compile_path"}
        self.write_results(self.row, compile_row)
        self.write_paths(
            {"name": "eval/out-of-range-index", "path": "$.numbers[128]"},
            {"name": "compile/out-of-range-index", "path": "$.numbers[128]"},
        )
        self.save_baseline()
        summary = self.summary()
        self.assertIn("**2 workloads**", summary)
        self.assertIn("1 compilation, 1 evaluation", summary)
        self.assertIn("1,234 | 1,234 | +0.00%", summary)
        self.assertIn("456 → 456 | 0 → 0 | 2,345 → 2,345", summary)
        self.assertIn("<code>$.numbers[128]</code>", summary)
        self.assertIn("Gungraun", summary)
        self.assertIn("Regressions: 0", summary)
        self.assertIn("simulated", summary)
        self.assertLess(summary.index("compile/out"), summary.index("eval/out"))
        self.assertNotIn("ns/iter", summary)

    def test_summary_shows_native_regressions_and_improvements(self):
        slow = copy.deepcopy(self.row)
        slow["id"] = "slow"
        total = slow["profiles"][0]["data"]["total"]
        total["metrics"]["Ir"]["values"] = {"old": 100, "new": 110}
        total["regressions"] = [{"Soft": {
            "metric": {"Callgrind": "Ir"}, "new": 110, "old": 100,
            "diff_pct": "10", "limit": "5",
        }}]
        fast = copy.deepcopy(self.row)
        fast["id"] = "fast"
        fast["profiles"][0]["data"]["total"]["metrics"]["Ir"]["values"] = {
            "old": 100, "new": 80,
        }
        self.write_results(slow, fast)
        self.write_paths({"name": "eval/slow", "path": "$.slow"},
                         {"name": "eval/fast", "path": "$.fast"})
        self.save_baseline()
        summary = self.summary()
        self.assertIn("Regressions: 1", summary)
        self.assertIn("100 | 110 | +10.00% | 🔴 Regression", summary)
        self.assertIn("100 | 80 | -20.00% | 🟢 Fewer instructions", summary)
        self.assertIn("🟥", summary)
        self.assertIn("🟩", summary)
        self.assertIn("All 2 workloads", summary)

    def test_missing_or_wrong_native_baseline_cannot_claim_a_comparison(self):
        for previous in [None, 1, 0, -1, True]:
            row = copy.deepcopy(self.row)
            values = row["profiles"][0]["data"]["total"]["metrics"]["Ir"]["values"]
            if previous is None:
                del values["old"]
            else:
                values["old"] = previous
            self.write_results(row)
            with self.subTest(previous=previous), self.assertRaises(ValueError):
                self.summary()

    def test_mismatched_workloads_or_paths_cannot_claim_a_comparison(self):
        row = copy.deepcopy(self.row)
        row["id"] = "different"
        self.baseline.write_text(json.dumps(row) + "\n")
        with self.assertRaises(ValueError):
            self.summary()
        self.save_baseline()
        self.baseline_paths.write_text(json.dumps({
            "name": "eval/out-of-range-index", "path": "$.different",
        }) + "\n")
        with self.assertRaises(ValueError):
            self.summary()

    def test_exactly_five_percent_uses_gungrauns_passing_verdict(self):
        values = self.row["profiles"][0]["data"]["total"]["metrics"]["Ir"]["values"]
        values.update(old=100, new=105)
        self.write_results(self.row)
        self.save_baseline()
        summary = self.summary()
        self.assertIn("100 | 105 | +5.00% | Within limit", summary)
        self.assertIn("Regressions: 0", summary)

    def test_rejects_empty_duplicate_or_malformed_results(self):
        for content in ["", "\n", "not json", self.results.read_text() * 2]:
            self.results.write_text(content)
            with self.subTest(content=content), self.assertRaises(ValueError):
                self.summary()

    def test_rejects_missing_invalid_or_zero_instruction_counts(self):
        for value in [None, -1, 0, 1.5, True, float("nan"), "1234"]:
            row = copy.deepcopy(self.row)
            metric = row["profiles"][0]["data"]["total"]["metrics"]["Ir"]
            metric["values"] = {} if value is None else {"new": value}
            self.write_results(row)
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.summary()
        row = copy.deepcopy(self.row)
        del row["profiles"][0]["data"]["total"]["metrics"]["Dr"]
        self.write_results(row)
        with self.assertRaises(ValueError):
            self.summary()

    def test_rejects_wrong_schema_suite_or_tool(self):
        for patch in [
            {"version": "unknown"}, {"kind": "BinaryBenchmark"},
            {"group": "other"}, {"function_name": "unknown"}, {"id": None},
            {"profiles": []}, {"profiles": [{"tool": "Dhat"}]},
            {"profiles": self.row["profiles"] * 2},
        ]:
            self.write_results({**self.row, **patch})
            with self.subTest(patch=patch), self.assertRaises(ValueError):
                self.summary()

    def test_metadata_must_cover_exactly_the_measured_workloads(self):
        valid = {"name": "eval/out-of-range-index", "path": "$.numbers[128]"}
        for rows in [[], [valid, valid], [{**valid, "name": "eval/other"}],
                     [valid, {**valid, "name": "eval/extra"}],
                     [{**valid, "path": None}], [{**valid, "path": ""}]]:
            self.write_paths(*rows)
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                self.summary()

    def test_paths_are_escaped_for_github_markdown(self):
        self.write_paths({
            "name": "eval/out-of-range-index",
            "path": '$.rows[?@.name == "a|<b>`"].uid\n',
        })
        self.baseline_paths.write_text(self.paths.read_text())
        self.assertIn(
            '<code>$.rows[?@.name == &quot;a&#124;&lt;b&gt;&#96;&quot;].uid </code>',
            self.summary(),
        )

    def test_cli_appends_job_summary_and_rejects_incomplete_data(self):
        summary = self.results.with_name("summary.md")
        summary.write_text("Existing summary\n")
        command = [
            sys.executable, str(Path(__file__).with_name("jsonpath_benchmark_report.py")),
            str(self.results), "--paths-file", str(self.paths),
            "--baseline-file", str(self.baseline),
            "--baseline-paths-file", str(self.baseline_paths),
        ]
        env = {**os.environ, "GITHUB_STEP_SUMMARY": str(summary)}
        result = subprocess.run(command, env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(summary.read_text(), "Existing summary\n" + result.stdout)
        total = self.row["profiles"][0]["data"]["total"]
        total["metrics"]["Ir"]["values"].update(old=100, new=110)
        total["regressions"] = [{"Soft": {
            "metric": {"Callgrind": "Ir"}, "new": 110, "old": 100,
            "diff_pct": "10", "limit": "5",
        }}]
        self.write_results(self.row)
        self.save_baseline()
        result = subprocess.run(command, env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("🔴 Regression", result.stdout)
        self.assertTrue(summary.read_text().endswith(result.stdout))
        self.results.write_text("")
        result = subprocess.run(command, env=env, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
