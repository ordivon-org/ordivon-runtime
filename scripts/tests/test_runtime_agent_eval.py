from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/runtime_agent_eval.py"
SPEC = importlib.util.spec_from_file_location("ordivon_runtime_agent_eval", SCRIPT)
assert SPEC and SPEC.loader
agent_eval = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = agent_eval
SPEC.loader.exec_module(agent_eval)


class RuntimeAgentEvalTests(unittest.TestCase):
    def test_suite_contract_is_valid_and_versioned(self) -> None:
        suite = agent_eval.load_suite(REPO / "evals/runtime-agent/suite.json")
        self.assertEqual(suite["suiteId"], "ordivon-runtime-agent-regression-v1")
        self.assertEqual(len(suite["tasks"]), 8)
        agent_eval.validate_against_tree(REPO, suite)

    def test_exact_mutation_requires_one_anchor(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "source.txt"
            path.write_text("before\n", encoding="utf-8")
            agent_eval.replace_exact(path, "before", "after")
            self.assertEqual(path.read_text(encoding="utf-8"), "after\n")
            with self.assertRaisesRegex(ValueError, "found 0"):
                agent_eval.replace_exact(path, "before", "after")

    def test_suite_json_declares_exact_task_count(self) -> None:
        data = json.loads((REPO / "evals/runtime-agent/suite.json").read_text(encoding="utf-8"))
        self.assertEqual(data["taskCount"], len(data["tasks"]))


if __name__ == "__main__":
    unittest.main()
