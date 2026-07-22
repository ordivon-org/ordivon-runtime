# Ordivon M7 Runtime Hardening and Lifecycle

- Phase: `ORDIVON-MIGRATION-M7-DESIGN-2026-07-22`
- Base: `c5c908280fdfdfb7699c7f1f2bbd17711be39cfd`
- Status: design and preflight only; no real Worker account or runtime route changed

## 1. Central question

M6 established transactional Job/Attempt truth. M7 asks:

> Can the same control plane survive a compromised payload process, a real WSL restart, long-running state growth, and orphan remediation without weakening M6's at-most-once and evidence contracts?

M7 is a hardening and lifecycle stage. It does not add new Agent tools, network access, credentials, deployment, push, merge, or production routing.

## 2. Primary goals

1. Run untrusted payloads under a stable non-login `ordivon-worker` identity.
2. Keep trusted Runner evidence outside payload write authority.
3. Prove real WSL/systemd reboot reconciliation.
4. Add bounded retention, quota, garbage collection, WAL checkpoint, backup, and restore.
5. Add evidence-backed remediation for held-orphaned Attempts.
6. Standardize evidence envelopes and stage closeout snapshots.

## 3. Threat model

The payload is model-authored or model-selected code and must be treated as untrusted. It may:

- read and modify any path writable by its UID;
- attempt to forge Runner result files;
- attempt to read Registry, immutable bundle, credentials, host sockets, or other workspaces;
- fork descendants, ignore termination, exhaust bounded resources, or race cleanup;
- exploit language caches or build scripts;
- survive long enough to overlap Core or host restart.

The trusted computing base remains:

- Ordivon Core and Registry;
- the small Runner supervisor;
- root-owned policy, binaries, bundles, results, migrations, and evidence code;
- systemd, kernel, cgroup v2, and the local filesystem.

M7 does not claim protection against a compromised kernel, systemd, root Core, or root-owned Runner binary.

## 4. Identity decision

A static system identity is selected over `DynamicUser` because workspace ownership, cache identity, Attempt recovery, and post-reboot evidence must remain stable.

The account contract is:

```text
user/group: ordivon-worker
login shell: /usr/bin/nologin
home/state root: /var/lib/ordivon/worker
interactive login: forbidden
supplementary groups: none by default
```

## 5. Trusted supervisor and payload identity split

Simply setting the entire Attempt unit to `User=ordivon-worker` is rejected. The Runner and target process would share one UID, allowing the target to attack Runner-owned result paths or same-UID process state.

M7 instead separates identities inside one heavily sandboxed Attempt unit:

```text
trusted Runner supervisor
  uid: 0
  capabilities: CAP_SETUID + CAP_SETGID only
  writable: root-owned control result path

untrusted payload child
  uid/gid: ordivon-worker
  supplementary groups: empty
  writable: assigned workspace, payload output, bounded cache
```

The Runner creates pipes, forks, clears supplementary groups, sets GID/UID, closes all control descriptors in the child, and only then executes the payload.

The supervisor must not regain broad root capability. Its bounding set remains limited to the downward identity transition. `NoNewPrivileges`, network/device/IPC/PID isolation and the existing resource ceilings remain enabled.

A preflight probe already proves that this split is locally feasible: the payload can write its workspace, cannot forge the root-owned result, and reports completion to the supervisor through a pre-created pipe.

## 6. Filesystem topology

```text
/var/lib/ordivon/control/                 root:root 0700
  registry/                              root:root 0700
  bundles/<attempt-id>/                  root:root 0700
  results/<attempt-id>/                  root:root 0700

/var/lib/ordivon/worker/                  root:ordivon-worker 0710
  workspaces/<workspace-id>/             ordivon-worker 0700
  attempts/<attempt-id>/                 ordivon-worker 0700

/var/cache/ordivon-worker/                ordivon-worker 0750
/run/ordivon/                             root:ordivon-worker 0750
```

Control bundles and final results are never worker-owned. Payload stdout/stderr are captured by supervisor-owned pipes; payload-writable files are treated as untrusted artifacts until Core validates and registers them.

Core creates Git worktrees from the root-owned source repository, then transfers only the worktree content required by the payload. The payload is not granted access to the source repository's `.git/worktrees` control metadata.

Builds that require Git metadata must use a separately reviewed read-only projection. M7 will not expose the root source repository merely to satisfy build scripts.

## 7. Runner evidence contract

The trusted Runner writes:

- `runner-start.json` under the root-owned control result root;
- bounded stdout/stderr through supervisor-owned handles;
- `result.json` under the root-owned result root;
- digest and identity fields already required by M6.

The payload receives only:

- its executable and argv;
- curated environment;
- workspace path;
- payload output path;
- selected cache roots.

It does not receive Registry paths, control bundle write access, launch tokens, result file descriptors, or supervisor-only IPC handles.

Any payload-authored file is evidence of payload output, not evidence of process termination or successful execution.

## 8. Toolchain and cache policy

Current Rust and MCP test harnesses depend on `/root/.rustup`, `/root/.cargo`, `/root/.npm`, and `/root/.local` paths. M7 must remove those dependencies from the runtime contract.

Selected direction:

- root-owned toolchains under `/opt/ordivon/toolchains/<identity>`;
- root-owned executable manifests with digest verification;
- worker-owned bounded build cache under `/var/cache/ordivon-worker`;
- no worker access to root home;
- cache identity included in execution plan and evidence when it can affect results.

## 9. Real WSL reboot recovery

An in-distro test process cannot survive `wsl.exe --terminate` to grade its own result. Reboot testing therefore requires an external Windows-side orchestrator.

Protocol:

1. Windows host starts an M7 reboot scenario inside the `archlinux` distro.
2. Scenario writes a root-owned reboot manifest containing Job IDs, Attempt IDs, pre-reboot boot ID, expected classifications, and an evidence digest.
3. Host waits until the Registry records the intended crash point.
4. Host invokes `wsl.exe --terminate archlinux`.
5. Host restarts the distro and waits for systemd and Ordivon reconciliation readiness.
6. In-distro verifier reads the new boot ID and Registry state.
7. Host collects final evidence and checks that no Attempt was executed twice.

Required scenarios:

- running long payload at reboot;
- persisted cancel intent at reboot;
- dispatch intent without bound Runner at reboot;
- Runner result present but not yet committed to Registry;
- held-orphaned reservation across reboot.

A boot ID change without valid terminal evidence must never be classified as success. Automatic redispatch remains forbidden.

## 10. Startup reconciliation gate

Before the M7 MCP Adapter accepts any new `workspace.exec`, startup must:

1. validate Registry and migrations;
2. compare current boot ID with all nonterminal Attempts;
3. reconcile systemd, cgroup, PID/start identity, bundle and result evidence;
4. preserve held-orphaned capacity;
5. commit pending terminal transactions;
6. publish a startup reconciliation Receipt;
7. open admission only after the gate succeeds.

## 11. Retention, quota, and garbage collection

Lifecycle policy is based on terminal resolution age, evidence class, size, and active references. It is not based on deleting the oldest directory blindly.

Minimum policy dimensions:

- maximum total Registry-associated Artifact bytes;
- maximum bytes per Job and Attempt;
- maximum terminal Job count;
- minimum retention period for failed, lost, orphaned, and cancelled Jobs;
- longer retention for security or recovery evidence;
- explicit legal hold / investigation hold;
- high-water and hard-stop thresholds.

GC protocol:

1. compute candidates in a read transaction;
2. write a signed/hashed GC plan and candidate digest;
3. revalidate candidate state and references in `BEGIN IMMEDIATE`;
4. mark records `gc_pending` in a future migration;
5. remove files using no-follow, containment-checked operations;
6. record actual deletions and failures;
7. finalize metadata without deleting append-only Job events.

Nonterminal Jobs, active/held reservations, open workspaces, current backups, and orphan evidence are never automatic GC candidates.

## 12. WAL checkpoint and backup

Checkpoint and backup are separate operations.

- Checkpoint reduces WAL growth but is not a backup.
- Backup uses SQLite's consistent backup mechanism or a transactionally valid copy process.
- Registry, migration identity, policy identity, Artifact manifest, and control bundle digests belong to one backup manifest.
- Worker cache and reproducible build outputs are excluded unless explicitly required.

Restore must occur into an empty staging root, validate migrations, `quick_check`, foreign keys, file digests and ownership, then atomically select the restored control root. Restore never overwrites a live Registry in place.

## 13. Held-orphaned remediation

M7 adds operator-mediated remediation, not automatic release.

Required operations:

- inspect orphan identity and all retained evidence;
- prove whether unit, cgroup and host PID/start identity still exist;
- request termination when a live process remains;
- re-observe after termination;
- release the held reservation only after absence is proven;
- append operator identity, reason, evidence digest and timestamps.

A missing unit alone is insufficient to release capacity. A reused PID or unit name must remain orphaned until start identity and cgroup evidence are resolved.

M7 does not permit deleting an orphaned Job to regain capacity.

## 14. Evidence envelope

All new M7 evidence must satisfy `evidence-envelope-v1` and include:

- source revision and exact implementation commit;
- harness paths and SHA-256 digests;
- environment identity;
- canonical observations digest;
- explicit claims not made.

Missing or mismatched envelope identity is a failed evidence gate, not a warning.

## 15. Stage closeout contract

Every M7 stage ends with a machine-generated snapshot containing:

```text
branch and HEAD
worktree status
matching units and processes
listeners
experimental worktrees and state roots
Receipt status and measured implementation SHA
```

A completion claim is blocked if the snapshot is inconsistent with the written Receipt or if runtime residue remains.

## 16. M7 phased implementation

### M7.0 — Preflight and contract freeze

- validate sysusers/tmpfiles templates in an isolated root;
- inventory root-specific paths and ownership assumptions;
- prove surrogate UID isolation;
- prove trusted supervisor / untrusted payload split feasibility;
- freeze evidence envelope and closeout snapshot;
- apply no real account or runtime changes.

### M7.1 — Identity and filesystem split

- create the static worker through reviewed packaging;
- migrate configurable control/worker/cache roots;
- split root-owned bundles/results from worker-owned workspace/output;
- remove root-home runtime dependencies;
- add permission and cross-workspace isolation tests.

### M7.2 — Runner payload privilege drop

- keep the trusted Runner supervisor minimal;
- retain only capabilities needed for downward UID/GID transition;
- close supervisor-only descriptors in the child;
- set groups/GID/UID and no-new-privileges before payload exec;
- prove payload cannot forge result or read Registry/control evidence.

### M7.3 — Reboot recovery

- implement Windows-side reboot orchestrator;
- add startup reconciliation gate;
- execute the required real WSL restart matrix;
- prove no duplicate dispatch and correct reservation convergence.

### M7.4 — Lifecycle governance

- add retention and quota policy;
- implement safe GC planning and execution;
- implement WAL checkpoint policy;
- implement backup and staged restore;
- preserve append-only events and investigation holds.

### M7.5 — Orphan remediation

- add inspect, terminate, re-observe and release operations;
- require operator identity and evidence digest;
- test stale PID, reused unit and surviving cgroup cases;
- prohibit automatic reservation release.

### M7.6 — Evaluation and decision

- rerun all M6 Runtime and MCP contracts;
- run payload escape and evidence-forgery negative tests;
- run real reboot matrix;
- run retention/GC/backup/restore matrix;
- run longer model-driven tasks under the worker identity;
- compare M6 root-payload and M7 worker-payload performance;
- authorize at most continued local hardening Dogfood.

## 17. Required gates

Identity and security:

- payload effective UID/GID equals the static worker;
- supplementary groups are empty;
- payload cannot read Registry, control bundle write paths, credentials, sockets, or other workspaces;
- payload cannot forge Runner result or terminal identity;
- Runner capability set contains no capability beyond the frozen minimum;
- cgroup cancellation still removes all descendants.

Recovery:

- every reboot scenario has a changed boot ID and externally collected evidence;
- no Job receives more than one Attempt from implicit recovery;
- startup admission remains closed until reconciliation completes;
- reservations converge or remain explicitly held-orphaned.

Lifecycle:

- no nonterminal or held-orphaned evidence is GC'd;
- backup and restore reproduce Registry and manifest digests;
- quota rejection is deterministic and auditable;
- cleanup is idempotent and leaves no partial metadata truth.

Performance and usability:

- M7 short-task median must not exceed M6 by more than 20%;
- Registry-only p95 budgets remain unchanged unless formally re-frozen;
- privilege-drop and evidence isolation add no model-visible calls;
- Context may not increase solely because of Worker identity metadata;
- longer tasks must preserve completion, repair and fallback gates.

Governance:

- all evidence passes the common envelope checker;
- every stage has a closeout snapshot;
- M7 adds no document-governance violation;
- Receipt language remains local-hardening-only and no-production-cutover.

## 18. Explicit non-goals

M7 does not include:

- Cloudflare, OAuth, or remote MCP;
- production port 8811 cutover;
- credentials or network authorization;
- Git push, merge, or deployment;
- external financial execution;
- multi-node scheduling or distributed SQLite replacement;
- automatic retry of ambiguous dispatch;
- Tool Foundry or dynamic tool promotion;
- broad product UI work.

## 19. Preparation findings

The current host supports systemd 261, cgroup v2, transient UID/GID execution, sysusers/tmpfiles dry-run, and Windows-side WSL orchestration. `/etc/wsl.conf` enables systemd and the distro is externally visible as `archlinux`.

The repository itself is root-owned mode 0700, and runtime/test code still contains root-specific paths. These are blockers for applying the real Worker but not blockers for the design.

A surrogate UID probe proves bundle read-only, control isolation, workspace/output write access, and no residual resources. A second probe proves a root supervisor with only `CAP_SETUID/CAP_SETGID` can drop a child to the worker UID while keeping its result path protected.

## 20. Entry decision

M7 design and preflight may proceed. Applying the real account and changing Runner execution are not yet authorized by this document alone.

The first implementation change must be M7.1 filesystem and identity preparation behind a feature gate, followed by M7.2 privilege-drop tests. A real reboot must not be attempted until the external host orchestrator and recovery manifest are independently reviewed.
