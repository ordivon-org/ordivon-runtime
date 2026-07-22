# Ordivon M7 Preparation Audit

- Phase: `ORDIVON-MIGRATION-M7-PREFLIGHT-2026-07-22`
- Measured commit: `bd0ef3f23d5296bcda41e47bd61d2c5190f9f1aa`
- Scope: design and host preflight only
- Runtime changes applied: none

## Host readiness

The host provides:

- Arch Linux under WSL2;
- systemd 261 with systemd enabled in `/etc/wsl.conf`;
- cgroup v2 with CPU, IO, memory and PID controllers;
- `systemd-run` UID/GID execution;
- `systemd-sysusers` and `systemd-tmpfiles` dry-run and alternate-root support;
- Windows-side `wsl.exe` access;
- externally visible distro name `archlinux`.

This is sufficient to design a Windows-orchestrated real reboot test. No reboot was executed during preflight.

## Packaging validation

The proposed static user and filesystem templates were applied only inside an isolated temporary root.

The generated account has:

```text
name: ordivon-worker
home: /var/lib/ordivon/worker
shell: /usr/bin/nologin
stable system UID/GID allocated by systemd-sysusers
```
The isolated tmpfiles layout produced the frozen modes:

```text
/var/lib/ordivon/control             0700 root:root
/var/lib/ordivon/control/registry    0700 root:root
/var/lib/ordivon/control/bundles     0700 root:root
/var/lib/ordivon/control/results     0700 root:root
/var/lib/ordivon/worker              0710 root:ordivon-worker
/var/cache/ordivon-worker            0750 ordivon-worker
/run/ordivon                         0750 root:ordivon-worker
```

The real account and directories were not applied to the host.

## Surrogate worker isolation

A real transient systemd service ran as UID/GID `nobody` with the intended sandbox properties.

Observed results:

- root-owned bundle was readable;
- bundle mutation was denied;
- root-only control data was inaccessible;
- assigned workspace was writable;
- assigned output path was writable;
- temporary service and filesystem state were removed after the probe.

This proves local UID isolation mechanics, not the final M7 Runner implementation.

## Supervisor and payload split

A second transient unit kept a trusted supervisor at UID 0 with only `CAP_SETUID` and `CAP_SETGID` in the capability bounding set. The child cleared supplementary groups and dropped to UID/GID 65534.
Observed results:

- child exited successfully;
- payload workspace write succeeded;
- payload could not write the root-owned result path;
- supervisor wrote the result after receiving child evidence through a pre-created pipe;
- supervisor remained UID 0 while the payload ran as the surrogate worker UID.

An initial version of this probe also showed that a root UID with only `SETUID/SETGID` capabilities cannot bypass DAC to read a worker-owned mode-0700 workspace. This is desirable: the supervisor must use pipes and wait status rather than reading payload-private files after execution.

## Current blockers

1. The actual `ordivon-worker` account is absent.
2. M6 Runner and payload still execute under one runtime identity.
3. M6 bundle, output and result paths are not yet split by trust level.
4. The repository is root-owned mode 0700.
5. Runtime and test code contain 24 root-specific path references at the measured commit.
6. Rust, Cargo and MCP test flows still rely on root-home toolchain/cache paths.
7. A real WSL terminate/restart/reconcile scenario has not been executed.
8. Retention, quota, backup, restore and orphan remediation remain designs only.

## Entry decision

M7.0 design and preparation gates pass. M7.1 may begin on a feature-gated branch with filesystem and identity migration.

The following remain prohibited until their dedicated gates pass:

- applying production routing;
- remote MCP or Cloudflare changes;
- credentials or network access;
- Git push, merge or deployment;
- weakening M6 ambiguity, sandbox or evidence contracts.

Canonical machine evidence is stored in `ordivon-m7-preflight-evidence-2026-07-22.json` and conforms to Evidence Envelope v1.
