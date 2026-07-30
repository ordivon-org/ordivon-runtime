from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/check_host_h2_r2_receipt.py"
RECEIPT = REPO / "evidence/harness-h2-runtime-r2-live-d50f609-20260731.json"

spec = importlib.util.spec_from_file_location("ordivon_host_h2_r2_receipt", SCRIPT)
assert spec and spec.loader
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class HostH2RuntimeR2ReceiptTests(unittest.TestCase):
    def test_committed_live_receipt_satisfies_runtime_boundary(self) -> None:
        result = checker.validate_receipt(RECEIPT)
        self.assertEqual(result["referenceTypes"], checker.EXPECTED_REFERENCE_TYPES)
        self.assertEqual(result["assignmentGeneration"], 1)
        self.assertFalse(result["productionRuntimeExpansionRequired"])
        self.assertTrue(str(result["jobId"]).startswith("job-"))
        self.assertTrue(str(result["attemptId"]).startswith("attempt-"))

    def test_reference_drift_is_rejected_even_with_recomputed_integrity(self) -> None:
        receipt = json.loads(RECEIPT.read_text(encoding="utf-8"))
        tampered = copy.deepcopy(receipt)
        references = tampered["request"]["execution"]["foreignReferences"]
        references[0]["generation"] = "2"
        payload = copy.deepcopy(tampered)
        payload.pop("integrity")
        tampered["integrity"]["payloadDigest"] = checker.canonical_digest(payload)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "tampered.json"
            path.write_text(json.dumps(tampered), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Assignment reference generation differs"):
                checker.validate_receipt(path)

    def test_failed_live_check_is_rejected_even_with_recomputed_integrity(self) -> None:
        receipt = json.loads(RECEIPT.read_text(encoding="utf-8"))
        tampered = copy.deepcopy(receipt)
        tampered["checks"]["freshClientRecoveredOriginalJob"] = False
        payload = copy.deepcopy(tampered)
        payload.pop("integrity")
        tampered["integrity"]["payloadDigest"] = checker.canonical_digest(payload)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "tampered.json"
            path.write_text(json.dumps(tampered), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "live checks failed"):
                checker.validate_receipt(path)


if __name__ == "__main__":
    unittest.main()
