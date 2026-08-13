---
schema_version: 1
id: runtime.data-privacy
title: Runtime Data and Privacy
type: decision
profile: organization
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - user
  - operator
  - builder
  - agent
updated: 2026-08-04
summary: Data inventory, sensitivity, retention, export, migration, deletion, and redaction boundaries for Runtime instances.
evidence_status: verified
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.start
  - runtime.status
  - runtime.operations
  - runtime.recovery
  - runtime.releases
---
# Runtime Data and Privacy

## Context

Runtime persists source, commands, process output, Artifacts, identifiers, operational receipts, and recovery state under owner-trusted local authority. Those bytes may contain sensitive information that Runtime cannot classify reliably.

## Decision

Treat the complete Runtime state root, exports, receipts, and backups as sensitive operator-owned data. Retention, export, migration, sharing, redaction, and deletion remain explicit operations rather than implicit background policy.

## Consequences

Runtime preserves evidence needed for idempotency and recovery, does not promise automatic redaction, and requires whole-instance archival or a future versioned cutover when historical state must be bounded.

## Status

Accepted and active for the current owner-trusted deployment model. Reopen when Runtime gains a new authority profile, remote storage owner, archival contract, or automatic deletion mechanism.

## Principle

Runtime data belongs to the operator and the users whose work it executes. Runtime must make storage and deletion behavior explicit, but it cannot promise that arbitrary commands, output, source trees, or Artifacts are free of sensitive information.

Runtime does **not** automatically redact command arguments, environment values supplied to target processes, source content, immutable input materializations, stdout, stderr, Artifacts, paths, client identifiers, or foreign references. Treat the entire Runtime state root and every backup or receipt as potentially sensitive.

`workspace.content` creates no additional retained media store. It opens one already-existing Workspace PNG/JPEG through descriptor-bound Workspace path authority, performs the bounded read and digest on that exact opened file, verifies the caller-supplied SHA-256 digest, and projects those exact bytes through the MCP response. The source image keeps the Workspace's existing lifecycle; Agent observation does not register a second Artifact or persist another image copy.

## Data inventory

| Data | Typical location | Purpose | Default lifecycle |
| --- | --- | --- | --- |
| Registry database | `/var/lib/ordivon/registry/registry.sqlite3` | Job, Attempt, reservation, event, Artifact metadata, recovery truth | retained while the instance remains authoritative |
| Attempt bundles and Artifacts | `/var/lib/ordivon/registry/attempts/` | request, plan, progress, output, result, terminal evidence | retained with Registry history unless the instance is retired or an explicit future archival policy applies |
| Workspace records and worktrees | `/var/lib/ordivon/runtime/` | isolated source state and lifecycle identity | policy classes: ephemeral, review, or pinned; dirty/active/unknown state is never removed automatically |
| execution caches | `/var/lib/ordivon/runtime/cache/` | reusable package and build state | Workspace-scoped state is protected while referenced; legacy source-build state is reconstructible and capacity-reclaimable; shared package-cache semantics remain package-manager-owned |
| immutable input staging and Job-owned bytes | `/var/lib/ordivon/runtime/input-materializations/` and `/var/lib/ordivon/runtime/job-inputs/` | exact digest-verified foreign bytes prepared during admission and then owned by the admitted input-bound Job | retained with current Runtime state; no automatic per-set reclamation contract yet |
| Windows immutable-input presentations | `%ProgramData%\OrdivonImmutableInputs\<inputSetId>` | reconstructible native NTFS realization of one committed Job-owned input tree for a limited Windows child | retained as provider-owned realization state; it is not the authority copy and no automatic presentation GC contract is claimed yet |
| protocol trace | configured `ORDIVON_TRACE_PATH` | bounded protocol-version and client observations | current segment plus one rotated segment |
| deployment receipts | `/var/lib/ordivon/deployments/` | binary, protocol, rollback, and catalog proof | current rollback and audit evidence; explicit pruning only |
| lifecycle, reclaim, cache, repair, and quarantine receipts | Runtime state roots | prove administrative actions and preserve uncertain bytes | explicit operator policy; quarantine is never silently deleted |
| backups | operator-selected destination | disaster recovery | operator-owned; never overwritten by restore |

Paths are defaults from the packaged deployment. The configured environment and deployment receipt are authoritative for a specific instance.

## Sensitive content

Runtime may persist:

- source code and untracked files;
- absolute paths and repository identities;
- commands and arguments;
- explicit target-process environment values;
- stdout and stderr;
- model- or user-produced files;
- API responses and external identifiers;
- client names, protocol versions, Task/Assignment correlation references;
- deployment, repair, and recovery history.

Do not place secrets in command arguments or source files when a narrower secret-delivery mechanism exists. Prevent target processes from printing credentials. Use `contained_local` only for the declared authority reduction; it is not a substitute for a dedicated secret broker or hostile-code boundary.

## Access control

The packaged service uses restrictive permissions:

- service and Runner `UMask=0077`;
- Registry files created as `0600`;
- Runtime and Registry state directories created as `0700` where possible;
- MCP origin restricted to loopback;
- Bearer authentication required for direct clients;
- a dedicated hosted-client Bearer, when configured, is separate from the trusted-local Bearer and remains full Runtime authority;
- public ingress is operator-owned: either an authenticated Access path or a dedicated Tunnel hostname whose origin still enforces the remote Bearer.

These controls protect against ordinary local disclosure. They do not protect data from root, the installed service user, or commands deliberately executed with that authority.

## Retention

Workspace retention is policy-driven and implemented through `ordivon-runtime-lifecycle`; it never deletes dirty, active, pinned, unknown, or orphan-directory state automatically.

Job, Attempt, event, Artifact, and terminal-evidence history is currently append-oriented and has no automatic per-record deletion path. This preserves idempotency, reconciliation, and recovery evidence. Operators who require bounded historical retention must use one of two explicit models:

1. archive and retire the complete Runtime instance after verifying the export; or
2. implement a future versioned archival/cutover contract that preserves every still-required replay, rollback, and reconstruction dependency.

Do not delete individual Registry rows or Attempt bundles manually.

## Export and migration

A complete logical export should include:

- a verified SQLite backup created by `scripts/backup.py`;
- the control root selected for that backup;
- required Attempt bundles and Artifact bytes;
- open or preserved Git Workspace state;
- deployment and recovery receipts needed to interpret the instance;
- the exact Runtime source commit, binary digests, protocol versions, and Tool catalog digest.

Verify the snapshot manifest and SQLite integrity before considering the export complete. Restore into a new destination with `scripts/restore.py`; restore never overwrites live state.

Cross-host migration may require repairing Git worktree paths, executable locations, service configuration, cache paths, and systemd identity. A successful file copy alone does not prove a recoverable Runtime instance.

## Deletion

There are three deletion levels:

### Workspace release

Use `workspace.close` or the receipted lifecycle/reclaim tools. Never delete a worktree directory directly. Closure refuses active Jobs and dirty state unless the caller explicitly chooses force and accepts the loss.

### Cache deletion

Use `ordivon-runtime-cache prune`. It protects current Workspace-scoped caches associated with open or active Workspaces, treats legacy source-scoped build caches as reconstructible reclaimable state because current execution never consumes them, and does not interpret package-manager global caches it does not own.

### Instance retirement

To remove all Runtime data:

1. stop new admission;
2. reconcile or explicitly classify every nonterminal Job;
3. export any state, Artifacts, receipts, or audit evidence that must survive;
4. stop and disable the Runtime and lifecycle units;
5. verify no Runtime-owned process tree remains;
6. remove the configured Runtime, Registry, deployment, receipt, backup, and quarantine roots according to operator policy;
7. remove the environment file and rotate any tunnel, token, or credential that could still authorize access.

Instance retirement is destructive and intentionally remains an operator procedure rather than a remotely callable Runtime Tool.

## Sharing diagnostics

Before sharing health reports, traces, receipts, logs, or Artifacts:

- prefer `ordivon-runtime-status --health --json`, which is designed to be secret-free;
- inspect all other output manually;
- remove source paths, client identifiers, command arguments, foreign references, and Artifact content when they are not required;
- never publish the environment file, Bearer token, raw Registry, Attempt bundle, backup, or quarantine bytes.

## Privacy non-goals

Runtime does not currently provide:

- content classification or automatic PII detection;
- cryptographic encryption at rest;
- per-user tenancy or access-control lists;
- selective deletion of append-only execution history;
- automatic redaction of arbitrary process output;
- a legal privacy policy for a hosted service.

A hosted Ordivon product must define those responsibilities above Runtime rather than claiming they are supplied by this trusted-local repository.
