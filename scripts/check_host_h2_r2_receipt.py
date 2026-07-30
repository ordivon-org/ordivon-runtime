#!/usr/bin/env python3
from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parents[1]
DEFAULT_RECEIPT = REPO / "evidence/harness-h2-runtime-r2-live-d50f609-20260731.json"
EXPECTED_REFERENCE_TYPES = ["assignment", "harness_run", "task", "task_attempt"]
EXPECTED_CHECKS = {
    "assignmentDigestConflict",
    "assignmentGenerationConflict",
    "exactReplayReturnedOriginalJob",
    "freshClientRecoveredOriginalJob",
    "handoffProjectsHarnessRun",
    "hostRecordedRuntimeJob",
    "noSemanticCompletionClaim",
    "oneRuntimeJob",
    "terminalEvidenceBoundAttempt",
    "terminalEvidenceBoundJob",
    "terminalEvidenceBoundSource",
    "terminalEvidenceBoundWorkspace",
    "terminalEvidenceRetainedExactReferences",
}


def canonical_digest(value: Any) -> str:
    encoded = json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return "sha256:" + hashlib.sha256(encoded).hexdigest()


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be an object")
    return value


def _array(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        raise ValueError(f"{label} must be an array")
    return value


def validate_receipt(path: str | Path = DEFAULT_RECEIPT) -> dict[str, Any]:
    receipt_path = Path(path)
    receipt = _object(json.loads(receipt_path.read_text(encoding="utf-8")), "receipt")
    if receipt.get("schemaVersion") != 1:
        raise ValueError("receipt schemaVersion differs")
    if receipt.get("kind") != "ordivon.host-h2-runtime-r2-live-receipt":
        raise ValueError("receipt kind differs")

    integrity = _object(receipt.get("integrity"), "integrity")
    if integrity.get("algorithm") != "sha256":
        raise ValueError("receipt integrity algorithm differs")
    if integrity.get("canonicalization") != "ordivon-canonical-json-v1":
        raise ValueError("receipt canonicalization differs")
    payload = copy.deepcopy(receipt)
    payload.pop("integrity")
    if integrity.get("payloadDigest") != canonical_digest(payload):
        raise ValueError("receipt integrity digest differs")

    checks = _object(receipt.get("checks"), "checks")
    if set(checks) != EXPECTED_CHECKS:
        raise ValueError("receipt check set differs")
    failed = sorted(name for name, result in checks.items() if result is not True)
    if failed:
        raise ValueError(f"receipt live checks failed: {failed}")

    request = _object(receipt.get("request"), "request")
    client_request_id = request.get("clientRequestId")
    if not isinstance(client_request_id, str) or not client_request_id.startswith("request:harness:g"):
        raise ValueError("Host clientRequestId is invalid")
    execution = _object(request.get("execution"), "request execution")
    references = _array(execution.get("foreignReferences"), "request foreignReferences")
    reference_types = [
        _object(reference, "Host reference").get("type") for reference in references
    ]
    if reference_types != EXPECTED_REFERENCE_TYPES:
        raise ValueError("Host reference order or types differ")
    if any(reference.get("namespace") != "ordivon.host" for reference in references):
        raise ValueError("Host reference namespace differs")
    for reference in references:
        if not isinstance(reference.get("id"), str) or not isinstance(reference.get("digest"), str):
            raise ValueError("Host reference identity or digest is invalid")

    assignment_generation = receipt.get("assignmentGeneration")
    assignment_id = receipt.get("assignmentId")
    assignment_reference = references[0]
    if assignment_reference.get("id") != assignment_id:
        raise ValueError("Assignment reference identity differs")
    if assignment_reference.get("generation") != str(assignment_generation):
        raise ValueError("Assignment reference generation differs")
    if references[1].get("id") != receipt.get("harnessRunId"):
        raise ValueError("Harness Run reference identity differs")
    if references[2].get("id") != receipt.get("taskId"):
        raise ValueError("Task reference identity differs")
    if references[3].get("id") != receipt.get("taskAttemptId"):
        raise ValueError("Task Attempt reference identity differs")

    runtime = _object(receipt.get("runtime"), "runtime")
    terminal = _object(runtime.get("terminalEvidence"), "terminal evidence")
    if terminal.get("foreignReferences") != references:
        raise ValueError("terminal evidence references differ from the Host request")
    job_id = runtime.get("jobId")
    attempt_id = runtime.get("attemptId")
    if terminal.get("jobId") != job_id or terminal.get("attemptId") != attempt_id:
        raise ValueError("terminal evidence Job or Runtime Attempt differs")
    workspace = _object(receipt.get("workspace"), "workspace")
    if terminal.get("workspaceId") != workspace.get("workspaceId"):
        raise ValueError("terminal evidence Workspace differs")
    if terminal.get("sourceRevision") != workspace.get("sourceRevision"):
        raise ValueError("terminal evidence source revision differs")
    if terminal.get("executionDisposition") != "succeeded":
        raise ValueError("Runtime execution did not succeed")
    if "semanticCompletion" in terminal or "taskOutcome" in terminal:
        raise ValueError("Runtime terminal evidence claims semantic completion")
    if runtime.get("workspaceClosed") is not True:
        raise ValueError("Runtime Workspace was not closed")

    jobs = _array(runtime.get("matchingJobs"), "matching Runtime Jobs")
    if len(jobs) != 1:
        raise ValueError("receipt does not retain exactly one Runtime Job")
    job = _object(jobs[0], "matching Runtime Job")
    if job.get("jobId") != job_id or job.get("attemptId") != attempt_id:
        raise ValueError("recovered Runtime Job identity differs")
    if job.get("clientRequestId") != client_request_id:
        raise ValueError("recovered Runtime Job request identity differs")

    host = _object(receipt.get("host"), "host")
    handoff = _object(host.get("handoff"), "Host handoff")
    if handoff.get("assignmentId") != assignment_id:
        raise ValueError("Host handoff Assignment differs")
    if handoff.get("assignmentGeneration") != assignment_generation:
        raise ValueError("Host handoff Assignment generation differs")
    if handoff.get("harnessRunId") != receipt.get("harnessRunId"):
        raise ValueError("Host handoff Harness Run differs")

    return {
        "receipt": str(receipt_path),
        "payloadDigest": integrity["payloadDigest"],
        "clientRequestId": client_request_id,
        "jobId": job_id,
        "attemptId": attempt_id,
        "assignmentGeneration": assignment_generation,
        "referenceTypes": reference_types,
        "productionRuntimeExpansionRequired": False,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Validate the committed Host H2 / Runtime R2 live receipt.")
    parser.add_argument("receipt", nargs="?", default=str(DEFAULT_RECEIPT))
    args = parser.parse_args()
    print(json.dumps(validate_receipt(args.receipt), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
