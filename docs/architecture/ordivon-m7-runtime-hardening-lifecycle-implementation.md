# Ordivon M7 Runtime Hardening and Lifecycle Implementation

- Phase: `ORDIVON-MIGRATION-M7-2026-07-22`
- Branch: `agent/m7-runtime-hardening-lifecycle`
- Design/preflight base: `7b7e037252ae2f5293d64c6c49eb47a9ee856ad0`
- Measured implementation: `c62f4f0a90a0c1866cc336fc819f2e860d457d41`
- Status: local hardened Dogfood gates pass; no production cutover

## 1. Result

M7 hardens the M6 transactional Job/Attempt control plane against an untrusted
payload identity, full WSL kernel restart, retained-state growth, and held
orphan remediation. It preserves M6 idempotency, at-most-once dispatch,
transactional terminal state, Native MCP Task projection, and bounded local
Agent interface.

M7 does not add Agent tools, network access, credentials, deployment, Git push
or merge, Cloudflare routing, production port changes, or external side-effect
authority.

## 2. Trusted supervisor and worker payload

A static non-login account is installed:

```text
user/group: ordivon-worker
uid/gid: host-assigned static system identity
home: /var/lib/ordivon/worker
shell: /usr/bin/nologin
supplementary groups: empty for payload execution
```

The Attempt unit starts a root-owned trusted Runner supervisor with a capability
bounding set limited to `CAP_SETUID` and `CAP_SETGID`. The supervisor creates
pipes and control evidence, forks, permanently drops the child to the worker
UID/GID, applies no-new-privileges, closes control descriptors, and then execs
the untrusted payload.

The payload cannot write Registry state, immutable bundles, trusted results, or
other workspaces. Runner start and terminal evidence remain root-owned.

## 3. Filesystem and mount-view isolation

M7 separates trust roots:

```text
/var/lib/ordivon/control       root-owned Registry, bundles, results
/var/lib/ordivon/worker        worker workspace and attempt payload data
/var/cache/ordivon-worker      bounded worker cache
/run/ordivon/<attempt>         per-Attempt mount view
```

A single static worker UID is not treated as sufficient cross-workspace
isolation. Each Attempt hides the worker parent tree and bind-mounts only its
assigned workspace, runtime directory, and cache into a per-Attempt view under
`/run/ordivon`. Source-repository Git control metadata and sibling workspaces
remain unavailable to the payload.

## 4. Lifecycle governance

SQLite migration v2 adds lifecycle policy, investigation hold, GC plan and
tombstone evidence, backup metadata, remediation evidence, and lifecycle
events. An M6 binary that does not understand the migration rejects the newer
Registry rather than silently ignoring lifecycle state.

Quota admission is evaluated inside the same `BEGIN IMMEDIATE` transaction as
idempotency and capacity admission. A quota rejection creates no Job, Attempt,
or reservation and leaves an auditable lifecycle event.

Garbage collection is two-phase:

```text
select eligible terminal state
→ persist digest-bound plan
→ revalidate in transaction
→ mark gc_pending/tombstone metadata
→ no-follow contained deletion
→ append lifecycle evidence
```

Nonterminal work, active reservations, held-orphaned Attempts, investigation
holds, and active backup state are excluded.

Online SQLite backup is blocked while capacity is active. Restore targets an
empty staging root, validates schema and content, and never overwrites a live
Registry in place.

## 5. Held-orphan remediation

Operator remediation is not exposed as a model MCP tool. Inspection returns an
evidence digest. Release requires the expected digest and re-observation of
unit InvocationID, cgroup, PID, and process start identity.

A reused unit name with a different InvocationID is not terminated. Once the
original process identity is proven absent, the original reservation may be
released with append-only operator evidence. A live original tree continues to
hold capacity.

## 6. Full WSL kernel-restart recovery

A Windows-side process executed `wsl.exe --shutdown`, restarted the Arch Linux
distribution, waited for systemd, and ran the recovery collector against five
persisted crash windows. Kernel boot ID changed from
`cd045495-b89b-49a6-8d00-d9a77eaa550a` to
`8c3d039b-9699-4c6a-b259-500b88204d11`.

Observed recovery:

| Scenario | Final Attempt | Reservation |
|---|---|---|
| running at reboot | lost | released |
| cancel intent pending | lost | released |
| result written, terminal transaction pending | succeeded | released |
| dispatch intent unbound | lost | released |
| held orphaned | orphaned | held_orphaned |

Attempt count remained `5 → 5`, no automatic redispatch occurred, no
nonterminal Attempt remained, and no Attempt unit remained active.

An earlier distro-only `wsl.exe --terminate` run revealed that WSL userspace can
restart without changing the shared kernel boot ID. The final harness therefore
requires full `wsl.exe --shutdown` for the kernel-restart claim.

## 7. Verification

The final local matrix contains ten successful commands:

- default workspace tests;
- M6 feature tests;
- M7 feature tests;
- M7 MCP tests;
- M7 Runner build;
- eight real M6 systemd recovery journeys;
- four real M7 hardening/remediation journeys;
- default, M7 executor, and M7 MCP strict Clippy.

The M7 feature suite also includes four lifecycle contracts: atomic quota
rejection, investigation hold and Artifact tombstoning, online backup and
empty-root restore, and active-capacity backup blocking.

M7 Wire, transport, and Dogfood evidence passed. The Dogfood matrix executed
eight journeys with forty model-visible calls, one repair round, zero fallback,
disconnect recovery, cgroup cancellation, idempotent replay, and a package
validation task executed under the worker identity.

Twenty M6/M7 Shadow pairs produced identical semantic digests. Overall medians:

| Metric | M6 | M7 |
|---|---:|---:|
| elapsed | 635 ms | 689 ms |
| tool calls | 5 | 5 |
| Context | 1812 B | 1808 B |
| HTTP requests | 5 | 5 |
| repair rounds | 0 | 0 |
| fallback | 0 | 0 |

The measured hardening overhead was approximately 8.5 percent in this bounded
local matrix, below the frozen 20 percent threshold. This local benchmark does
not establish production throughput or broad workload performance.

The independent evidence checker returned `M7_EVIDENCE_PASS`. Modifying the M7
Shadow summary caused digest verification to fail with exit 1.

## 8. Decision

M7 is eligible for continued bounded local hardened Dogfood. The M-series
execution migration has now proven durable execution, compact Agent projection,
real MCP transport, bounded Dogfood, transactional Job/Attempt truth, non-root
payload isolation, lifecycle governance, orphan remediation, and one full WSL
kernel-restart recovery matrix.

This result does not authorize production routing, remote exposure, credentials,
network access, deployment, push, merge, external side effects, multi-host
scheduling, or unattended high-risk authority.

## 9. Remaining debt

- Long-duration soak, resource-pressure, and repeated-reboot evidence is absent.
- The static worker remains one local identity; isolation depends on the mount
  namespace and filesystem contract rather than per-Job UIDs.
- Toolchain supply-chain projection and cache provenance remain narrow.
- Backup/restore is local and single-node; disaster recovery is not proven.
- Remote principal authentication and organization policy distribution remain
  outside M7.
- Repository document governance retains the same historical violations that
  predate the M-series closeout.
