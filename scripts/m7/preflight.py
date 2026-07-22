#!/usr/bin/env python3
"""M7 host and identity preflight without applying the real Worker account."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pwd
import shutil
import stat
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
SYSUSERS = REPO_ROOT / "packaging/systemd/sysusers.d/ordivon.conf"
TMPFILES = REPO_ROOT / "packaging/systemd/tmpfiles.d/ordivon.conf"


def run(args: list[str], *, cwd: Path = REPO_ROOT, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(args, cwd=cwd, text=True, capture_output=True, check=False)
    if check and result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"{' '.join(args)} failed ({result.returncode}): {detail}")
    return result


def sha256_bytes(value: bytes) -> str:
    return "sha256:" + hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def canonical_digest(value: Any) -> str:
    payload = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return sha256_bytes(payload)


def git_head() -> str:
    return run(["git", "rev-parse", "HEAD"]).stdout.strip()


def mode(path: Path) -> str:
    return oct(stat.S_IMODE(path.stat().st_mode))


def os_release() -> dict[str, str]:
    values: dict[str, str] = {}
    for line in Path("/etc/os-release").read_text().splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key] = value.strip().strip('"')
    return values


def validate_packaging() -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="ordivon-m7-root-") as temporary:
        root = Path(temporary)
        (root / "etc").mkdir(parents=True)
        run(["systemd-sysusers", f"--root={root}", str(SYSUSERS)])
        run(["systemd-tmpfiles", "--create", f"--root={root}", str(TMPFILES)])
        passwd_text = (root / "etc/passwd").read_text()
        worker_line = next(
            line for line in passwd_text.splitlines() if line.startswith("ordivon-worker:")
        )
        fields = worker_line.split(":")
        worker_uid = int(fields[2])
        worker_gid = int(fields[3])
        expected = {
            "/var/lib/ordivon/control": "0o700",
            "/var/lib/ordivon/control/registry": "0o700",
            "/var/lib/ordivon/control/bundles": "0o700",
            "/var/lib/ordivon/control/results": "0o700",
            "/var/lib/ordivon/worker": "0o710",
            "/var/cache/ordivon-worker": "0o750",
            "/run/ordivon": "0o750",
        }
        actual = {path: mode(root / path.removeprefix("/")) for path in expected}
        if actual != expected:
            raise RuntimeError(f"tmpfiles mode mismatch: expected={expected} actual={actual}")
        return {
            "workerUid": worker_uid,
            "workerGid": worker_gid,
            "workerHome": fields[5],
            "workerShell": fields[6],
            "directoryModes": actual,
            "passed": True,
        }


def privilege_probe() -> dict[str, Any]:
    nobody = pwd.getpwnam("nobody")
    root = Path(f"/var/lib/ordivon-m7-preflight-{os.getpid()}")
    bundle = root / "bundle"
    control = root / "control"
    workspace = root / "workspace"
    output = root / "output"
    try:
        for path in (bundle, control, workspace, output):
            path.mkdir(parents=True, exist_ok=True)
        root.chmod(0o755)
        bundle.chmod(0o755)
        control.chmod(0o700)
        workspace.chmod(0o700)
        output.chmod(0o700)
        os.chown(workspace, nobody.pw_uid, nobody.pw_gid)
        os.chown(output, nobody.pw_uid, nobody.pw_gid)
        request = bundle / "request.json"
        request.write_text('{"probe":"M7_WORKER_READ"}\n')
        request.chmod(0o444)
        secret = control / "registry-secret"
        secret.write_text("CONTROL_ONLY\n")
        secret.chmod(0o600)
        script = workspace / "probe.py"
        script.write_text(_probe_script(bundle, control, workspace, output))
        os.chown(script, nobody.pw_uid, nobody.pw_gid)
        script.chmod(0o500)
        command = [
            "systemd-run",
            "--quiet",
            "--wait",
            "--pipe",
            "--collect",
            "--uid=nobody",
            "--gid=nobody",
            "--property=NoNewPrivileges=yes",
            "--property=CapabilityBoundingSet=",
            "--property=AmbientCapabilities=",
            "--property=ProtectSystem=strict",
            "--property=ProtectHome=yes",
            "--property=PrivateTmp=yes",
            "--property=PrivateNetwork=yes",
            "--property=PrivateDevices=yes",
            "--property=PrivateIPC=yes",
            "--property=PrivatePIDs=yes",
            "--property=ProtectProc=invisible",
            "--property=ProcSubset=pid",
            "--property=RestrictAddressFamilies=AF_UNIX",
            f"--property=ReadOnlyPaths={bundle}",
            f"--property=ReadWritePaths={workspace} {output}",
            "/usr/bin/python3.14",
            str(script),
        ]
        run(command)
        result = json.loads((output / "result.json").read_text())
        if result != {
            "bundleReadable": True,
            "bundleImmutable": True,
            "controlHidden": True,
            "workspaceWritable": True,
            "outputWritable": True,
        }:
            raise RuntimeError(f"unexpected privilege probe result: {result}")
        return result | {"surrogateUid": nobody.pw_uid, "passed": True}
    finally:
        shutil.rmtree(root, ignore_errors=True)


def _probe_script(bundle: Path, control: Path, workspace: Path, output: Path) -> str:
    return f'''import json
from pathlib import Path
bundle = Path({str(bundle)!r})
control = Path({str(control)!r})
workspace = Path({str(workspace)!r})
output = Path({str(output)!r})
result = {{}}
result["bundleReadable"] = "M7_WORKER_READ" in (bundle / "request.json").read_text()
try:
    (bundle / "request.json").write_text("tamper")
    result["bundleImmutable"] = False
except OSError:
    result["bundleImmutable"] = True
try:
    (control / "registry-secret").read_text()
    result["controlHidden"] = False
except OSError:
    result["controlHidden"] = True
(workspace / "worker-write.txt").write_text("workspace")
result["workspaceWritable"] = (workspace / "worker-write.txt").read_text() == "workspace"
(output / "worker-output.txt").write_text("output")
result["outputWritable"] = (output / "worker-output.txt").read_text() == "output"
(output / "result.json").write_text(json.dumps(result, sort_keys=True))
'''


def root_path_references() -> dict[str, Any]:
    roots = [REPO_ROOT / "crates/ordivon-exec", REPO_ROOT / "crates/ordivon-mcp", REPO_ROOT / "scripts/mcp"]
    matches: list[dict[str, Any]] = []
    for root in roots:
        for path in root.rglob("*"):
            if not path.is_file() or path.suffix not in {".rs", ".py", ".mjs"}:
                continue
            try:
                text = path.read_text()
            except UnicodeDecodeError:
                continue
            for line_number, line in enumerate(text.splitlines(), 1):
                if "/root" in line:
                    matches.append(
                        {
                            "path": str(path.relative_to(REPO_ROOT)),
                            "line": line_number,
                            "textDigest": sha256_bytes(line.strip().encode()),
                        }
                    )
    return {"count": len(matches), "matches": matches}


def wsl_observation() -> dict[str, Any]:
    wsl_exe = Path("/mnt/c/Windows/System32/wsl.exe")
    config = Path("/etc/wsl.conf").read_text() if Path("/etc/wsl.conf").is_file() else ""
    distros: list[str] = []
    if wsl_exe.is_file():
        result = subprocess.run([str(wsl_exe), "-l", "-q"], capture_output=True, check=False)
        decoded = result.stdout.decode("utf-16le", errors="ignore").replace("\x00", "")
        distros = [line.strip() for line in decoded.splitlines() if line.strip()]
    return {
        "systemdEnabled": "systemd=true" in config.replace(" ", "").lower(),
        "hostWslExecutable": str(wsl_exe) if wsl_exe.is_file() else None,
        "distros": distros,
        "externalRebootOrchestratorPossible": wsl_exe.is_file() and bool(distros),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise RuntimeError("M7 preflight requires root for transient UID isolation probe")

    release = os_release()
    systemd_version = run(["systemctl", "--version"]).stdout.splitlines()[0]
    cgroup_type = run(["stat", "-fc", "%T", "/sys/fs/cgroup"]).stdout.strip()
    worker_exists = subprocess.run(
        ["getent", "passwd", "ordivon-worker"], capture_output=True, check=False
    ).returncode == 0
    repo_stat = REPO_ROOT.stat()
    observations = {
        "host": {
            "osId": release.get("ID"),
            "osVersion": release.get("VERSION_ID"),
            "kernel": run(["uname", "-r"]).stdout.strip(),
            "architecture": run(["uname", "-m"]).stdout.strip(),
            "systemdVersion": systemd_version,
            "cgroupFilesystem": cgroup_type,
            "bootId": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
            "virtualization": "wsl",
        },
        "wsl": wsl_observation(),
        "packaging": validate_packaging(),
        "surrogateWorkerProbe": privilege_probe(),
        "supervisorPayloadProbe": supervisor_payload_probe(),
        "currentWorkerExists": worker_exists,
        "repository": {
            "path": str(REPO_ROOT),
            "mode": oct(stat.S_IMODE(repo_stat.st_mode)),
            "ownerUid": repo_stat.st_uid,
            "ownerGid": repo_stat.st_gid,
        },
        "hardCodedRootPaths": root_path_references(),
        "blockers": [
            "The real ordivon-worker account has not been applied.",
            "The trusted Runner and untrusted payload still share one runtime identity.",
            "M6 control bundle and payload-writable output roots are not yet split.",
            "Existing test and harness paths still contain root-specific locations.",
            "A real WSL reboot recovery run has not been executed.",
        ],
        "readiness": {
            "designReady": True,
            "packagingTemplatesValid": True,
            "surrogateUidIsolationPassed": True,
            "supervisorPayloadSplitProbePassed": True,
            "hostRebootOrchestrationAvailable": wsl_observation()[
                "externalRebootOrchestratorPossible"
            ],
            "runtimeChangeApplied": False,
        },
    }

    revision = git_head()
    evidence = {
        "schemaVersion": 1,
        "phase": "ORDIVON-MIGRATION-M7-PREFLIGHT-2026-07-22",
        "evidenceType": "runtime-hardening-preflight",
        "generatedAt": datetime.now(timezone.utc).isoformat(),
        "sourceRevision": revision,
        "implementationCommit": revision,
        "harnesses": [
            {
                "path": str(Path(__file__).resolve().relative_to(REPO_ROOT)),
                "sha256": sha256_file(Path(__file__).resolve()),
            },
            {
                "path": str(SYSUSERS.relative_to(REPO_ROOT)),
                "sha256": sha256_file(SYSUSERS),
            },
            {
                "path": str(TMPFILES.relative_to(REPO_ROOT)),
                "sha256": sha256_file(TMPFILES),
            },
        ],
        "environment": observations["host"] | {"wsl": observations["wsl"]},
        "observationsDigest": canonical_digest(observations),
        "claimsNotMade": [
            "The real ordivon-worker account was not created or enabled.",
            "No M6 Runner or payload execution path was changed.",
            "The surrogate nobody probe does not prove the final supervisor/payload identity split.",
            "No real WSL reboot was executed.",
            "The preflight does not authorize production routing or broader capabilities.",
        ],
        "observations": observations,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps({"output": str(args.output), "readiness": observations["readiness"]}, indent=2))
    return 0


def supervisor_payload_probe() -> dict[str, Any]:
    nobody = pwd.getpwnam("nobody")
    root = Path(f"/var/lib/ordivon-m7-supervisor-{os.getpid()}")
    control = root / "control"
    workspace = root / "workspace"
    try:
        control.mkdir(parents=True)
        workspace.mkdir()
        root.chmod(0o755)
        control.chmod(0o700)
        workspace.chmod(0o700)
        os.chown(workspace, nobody.pw_uid, nobody.pw_gid)
        script = control / "probe.py"
        script.write_text(_supervisor_probe_script(control, workspace, nobody.pw_uid, nobody.pw_gid))
        script.chmod(0o500)
        command = [
            "systemd-run", "--quiet", "--wait", "--pipe", "--collect",
            f"--setenv=PROBE_CONTROL={control}",
            f"--setenv=PROBE_WORKSPACE={workspace}",
            "--property=NoNewPrivileges=yes",
            "--property=CapabilityBoundingSet=CAP_SETUID CAP_SETGID",
            "--property=AmbientCapabilities=",
            "--property=ProtectSystem=strict",
            "--property=ProtectHome=yes",
            "--property=PrivateTmp=yes",
            "--property=PrivateNetwork=yes",
            "--property=PrivateDevices=yes",
            "--property=PrivateIPC=yes",
            "--property=PrivatePIDs=yes",
            "--property=ProtectProc=invisible",
            "--property=ProcSubset=pid",
            "--property=RestrictAddressFamilies=AF_UNIX",
            f"--property=ReadWritePaths={control} {workspace}",
            "/usr/bin/python3.14", str(script),
        ]
        run(command)
        result = json.loads((control / "result.json").read_text())
        expected = {
            "childExitedZero": True,
            "payloadUid": nobody.pw_uid,
            "resultProtected": True,
            "supervisorUid": 0,
            "workspaceWritable": True,
        }
        if result != expected:
            raise RuntimeError(f"unexpected supervisor/payload result: {result}")
        return result | {"passed": True}
    finally:
        shutil.rmtree(root, ignore_errors=True)


def _supervisor_probe_script(control: Path, workspace: Path, uid: int, gid: int) -> str:
    return f'''import json
import os
from pathlib import Path
control = Path({str(control)!r})
workspace = Path({str(workspace)!r})
result = control / "result.json"
read_fd, write_fd = os.pipe()
pid = os.fork()
if pid == 0:
    os.close(read_fd)
    os.setgroups([])
    os.setgid({gid})
    os.setuid({uid})
    payload = {{}}
    try:
        result.write_text("forged")
        payload["resultProtected"] = False
    except OSError:
        payload["resultProtected"] = True
    (workspace / "payload-write.txt").write_text("payload")
    payload["workspaceWritable"] = True
    os.write(write_fd, json.dumps(payload).encode())
    os.close(write_fd)
    os._exit(0)
os.close(write_fd)
payload = json.loads(os.read(read_fd, 4096).decode())
os.close(read_fd)
_, status = os.waitpid(pid, 0)
payload["payloadUid"] = {uid}
payload["supervisorUid"] = os.getuid()
payload["childExitedZero"] = os.waitstatus_to_exitcode(status) == 0
result.write_text(json.dumps(payload, sort_keys=True))
'''


if __name__ == "__main__":
    raise SystemExit(main())
