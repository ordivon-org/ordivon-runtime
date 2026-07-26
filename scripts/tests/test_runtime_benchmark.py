from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/runtime_benchmark.py"
SPEC = importlib.util.spec_from_file_location("ordivon_runtime_benchmark", SCRIPT)
assert SPEC and SPEC.loader
benchmark = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = benchmark
SPEC.loader.exec_module(benchmark)


class RuntimeBenchmarkTests(unittest.TestCase):
    def test_summary_is_stable_and_reports_tail_latency(self) -> None:
        summary = benchmark.summarize([1.0, 2.0, 3.0, 4.0, 10.0])
        self.assertEqual(summary["n"], 5)
        self.assertEqual(summary["p50Ms"], 3.0)
        self.assertEqual(summary["p95Ms"], 10.0)

    def test_comparison_reports_only_material_regressions(self) -> None:
        baseline = {"read": {"p50Ms": 10.0, "p95Ms": 20.0}}
        current = {"read": {"p50Ms": 11.0, "p95Ms": 26.0}}
        regressions = benchmark.compare_results(current, baseline, 0.20)
        self.assertEqual(len(regressions), 1)
        self.assertEqual(regressions[0]["metric"], "p95Ms")
        self.assertEqual(regressions[0]["regressionRatio"], 0.3)


if __name__ == "__main__":
    unittest.main()
