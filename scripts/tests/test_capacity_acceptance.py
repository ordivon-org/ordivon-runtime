from __future__ import annotations

import json
from pathlib import Path
import runpy
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/ordivon-runtime-capacity-acceptance"


class CapacityAcceptanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.module = runpy.run_path(str(SCRIPT))

    def test_capacity_checks_require_exact_holder_sets_and_cleanup(self) -> None:
        checks = self.module["capacity_checks"](
            limit=2,
            job_ids=["job-1", "job-2"],
            workspace_ids=["workspace-1", "workspace-2", "workspace-3"],
            rejection={
                "code": "CONCURRENCY_LIMIT",
                "capacity": {
                    "scope": "global",
                    "active": 2,
                    "limit": 2,
                    "holderJobIds": ["job-2", "job-1"],
                    "holderWorkspaceIds": ["workspace-2", "workspace-1"],
                    "holdersTruncated": False,
                },
            },
            terminal=[
                {"jobId": "job-1", "status": "succeeded"},
                {"jobId": "job-2", "status": "succeeded"},
            ],
            close_results=[
                {"workspaceId": "workspace-3", "closed": True, "removed": True},
                {"workspaceId": "workspace-2", "closed": True, "removed": True},
                {"workspaceId": "workspace-1", "closed": True, "removed": True},
            ],
        )
        self.assertTrue(all(checks.values()))
        bad = dict(checks)
        bad["holderProjectionConsistent"] = False
        self.assertFalse(all(bad.values()))

    def test_capacity_checks_accept_honest_bounded_holder_projection(self) -> None:
        jobs = [f"job-{index}" for index in range(17)]
        workspaces = [f"workspace-{index}" for index in range(18)]
        checks = self.module["capacity_checks"](
            limit=17,
            job_ids=jobs,
            workspace_ids=workspaces,
            rejection={
                "code": "CONCURRENCY_LIMIT",
                "capacity": {
                    "scope": "global",
                    "active": 17,
                    "limit": 17,
                    "holderJobIds": jobs[:16],
                    "holderWorkspaceIds": workspaces[:16],
                    "holdersTruncated": True,
                },
            },
            terminal=[{"jobId": job, "status": "succeeded"} for job in jobs],
            close_results=[
                {"workspaceId": workspace, "closed": True, "removed": True}
                for workspace in workspaces
            ],
        )
        self.assertTrue(all(checks.values()))

    def test_capacity_cli_bounds_follow_runtime_types_not_legacy_sixteen(self) -> None:
        self.assertEqual(self.module["positive_limit"]("17"), 17)
        self.assertEqual(self.module["positive_limit"]("4294967295"), 4_294_967_295)
        with self.assertRaises(Exception):
            self.module["positive_limit"]("4294967296")
        self.assertEqual(self.module["positive_seconds"]("61"), 61.0)

    def test_atomic_receipt_is_private_and_integrity_is_stable(self) -> None:
        report = {"schemaVersion": 1, "checks": {"ok": True}}
        digest = self.module["canonical_digest"](report)
        self.assertEqual(digest, self.module["canonical_digest"](json.loads(json.dumps(report))))
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "receipt.json"
            self.module["write_json_atomic"](path, report, pretty=True)
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(json.loads(path.read_text(encoding="utf-8")), report)


if __name__ == "__main__":
    unittest.main()
