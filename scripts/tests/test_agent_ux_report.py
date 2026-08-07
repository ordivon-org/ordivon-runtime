from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/agent_ux_report.py"
SPEC = importlib.util.spec_from_file_location("agent_ux_report", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def write_jsonl(path: Path, records: list[dict[str, object]]) -> None:
    path.write_text(
        "".join(json.dumps(record, sort_keys=True) + "\n" for record in records),
        encoding="utf-8",
    )


class AgentUxReportTests(unittest.TestCase):
    def test_boundary_outcomes_expose_failure_classification_and_coverage(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            trace = Path(temporary) / "runtime-trace.jsonl"
            write_jsonl(
                trace,
                [
                    {
                        "kind": "mcp_protocol_observation",
                        "method": "tools/call",
                        "tool": "workspace.exec",
                        "observedUnixMs": 100,
                        "client": {"name": "openai-mcp", "version": "1.0.0"},
                    },
                    {
                        "kind": "mcp_tool_call_outcome",
                        "tool": "workspace.exec",
                        "ok": True,
                        "outcome": "success",
                        "totalMs": 20,
                        "observedUnixMs": 101,
                    },
                    {
                        "kind": "mcp_protocol_observation",
                        "method": "tools/call",
                        "tool": "workspace.mutate",
                        "observedUnixMs": 102,
                        "client": {"name": "openai-mcp", "version": "1.0.0"},
                    },
                    {
                        "kind": "mcp_tool_call_outcome",
                        "tool": "workspace.mutate",
                        "ok": False,
                        "outcome": "tool_error",
                        "errorCode": "INVALID_REQUEST",
                        "errorOrigin": "mcp_adapter",
                        "retryClass": "never",
                        "commitState": "not_started",
                        "field": "expectedDigest",
                        "totalMs": 5,
                        "observedUnixMs": 103,
                    },
                    {
                        "kind": "mcp_protocol_observation",
                        "method": "tools/call",
                        "tool": "workspace.read",
                        "observedUnixMs": 104,
                        "client": {"name": "openai-mcp", "version": "1.0.0"},
                    },
                ],
            )
            report = MODULE.build_report(trace, 0)
            self.assertEqual(report["sourceMode"], "mcp_tool_call_outcome")
            self.assertTrue(report["classificationAvailable"])
            self.assertEqual(report["protocolCallCount"], 3)
            self.assertEqual(report["outcomeCount"], 2)
            self.assertEqual(report["missingOutcomeCount"], 1)
            self.assertEqual(report["failedOutcomeCount"], 1)
            self.assertEqual(report["signals"]["invalidRequestCount"], 1)
            self.assertEqual(report["tools"]["workspace.mutate"]["errorCodes"], {"INVALID_REQUEST": 1})
            self.assertEqual(report["tools"]["workspace.mutate"]["fields"], {"expectedDigest": 1})
            self.assertEqual(report["clients"], {"openai-mcp@1.0.0": 3})

    def test_legacy_core_fallback_keeps_pre_rt0_trace_useful(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            trace = Path(temporary) / "runtime-trace.jsonl"
            write_jsonl(
                trace,
                [
                    {
                        "kind": "mcp_protocol_observation",
                        "method": "tools/call",
                        "tool": "workspace.execPlan",
                        "observedUnixMs": 200,
                    },
                    {
                        "tool": "workspace.execPlan",
                        "ok": False,
                        "coreMs": 2,
                        "totalMs": 3,
                        "observedUnixMs": 201,
                    },
                ],
            )
            report = MODULE.build_report(trace, 0)
            self.assertEqual(report["sourceMode"], "legacy_core")
            self.assertFalse(report["classificationAvailable"])
            self.assertEqual(report["protocolCallCount"], 1)
            self.assertEqual(report["outcomeCount"], 1)
            self.assertEqual(report["failedOutcomeCount"], 1)
            self.assertEqual(report["tools"]["workspace.execPlan"]["failureRateBasisPoints"], 10_000)

    def test_boundary_cutover_does_not_count_old_protocol_calls_as_missing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            trace = Path(temporary) / "runtime-trace.jsonl"
            write_jsonl(
                trace,
                [
                    {
                        "kind": "mcp_protocol_observation",
                        "method": "tools/call",
                        "tool": "workspace.read",
                        "observedUnixMs": 10,
                    },
                    {
                        "kind": "mcp_protocol_observation",
                        "method": "tools/call",
                        "tool": "workspace.read",
                        "observedUnixMs": 99,
                    },
                    {
                        "kind": "mcp_tool_call_outcome",
                        "tool": "workspace.read",
                        "ok": True,
                        "outcome": "success",
                        "totalMs": 1,
                        "observedUnixMs": 100,
                    },
                    {
                        "kind": "mcp_protocol_observation",
                        "method": "tools/call",
                        "tool": "workspace.get",
                        "observedUnixMs": 101,
                    },
                    {
                        "kind": "mcp_tool_call_outcome",
                        "tool": "workspace.get",
                        "ok": True,
                        "outcome": "success",
                        "totalMs": 2,
                        "observedUnixMs": 102,
                    },
                ],
            )
            report = MODULE.build_report(trace, 0)
            self.assertEqual(report["effectiveSinceMs"], 99)
            self.assertEqual(report["protocolCallCount"], 2)
            self.assertEqual(report["outcomeCount"], 2)
            self.assertEqual(report["missingOutcomeCount"], 0)


if __name__ == "__main__":
    unittest.main()
