# Ordivon Runtime Architecture

## Purpose

The Agent reasons and chooses actions. Ordivon executes, owns the process tree, records reality, and recovers after interruption.

## Active path

```text
workspace request
→ exact Git worktree
→ server-bound execution context
→ SQLite admission
→ Job + Attempt
→ immutable runner bundle
→ systemd transient unit / cgroup
→ bounded stdout, stderr, result, and Artifacts
→ reconciliation
```

## Minimal sources of truth

| Fact | Owner |
| --- | --- |
| Repository history, branch, diff, rollback | Git |
| Request identity, dispatch, cancellation, terminal resolution | SQLite Registry |
| Running process tree and termination | systemd and cgroup |
| Output, result, and Artifact bytes | bounded Digest-bound files |
| Current deployed architecture | `docs/current-state.md` |

Everything reconstructable from these sources should remain reconstructable.

## Authority

Ordivon is trusted-local. A task inherits the service user's network, environment, credentials, filesystem, Git remotes, Docker, systemd, cloud tooling, and executable surface. Untrusted workloads require an external VM or container boundary.

The server binds one Principal and one global concurrency limit from deployment configuration. Every MCP client shares that limit. Admission permits at most one active `workspace.exec` per Workspace, allowing independent worktrees to run concurrently without multiple command trees mutating the same tree.

Local HTTP uses a fixed Bearer token. An explicitly enabled Cloudflare Access path accepts the Access JWT assertion at the loopback origin.

## Runtime invariants

- idempotent admission for repeated client requests;
- transactional global and per-Workspace capacity admission;
- no speculative redispatch after ambiguous launch;
- one owned cgroup for the complete process tree;
- persisted cancellation intent before termination;
- timeout and bounded retained output;
- Digest-bound executable, result, and Artifact identity;
- startup and observation-time reconciliation;
- dirty Workspaces survive accidental close requests;
- active or held Jobs always prevent Workspace removal;
- Job pages are newest-first and expose semantic identity without command arguments or environment values;
- incremental output cursors consume retained UTF-8 text without replaying earlier bytes or exceeding requested byte bounds.

## Recovery

A complete identity-bound Runner result is terminal truth after the original process tree is proven gone. A malformed or identity-conflicting result remains quarantined. A dispatch that cannot be proven running or terminal becomes `lost` or `orphaned`; it is not automatically repeated.

Git restores code. SQLite and result bundles restore task knowledge. systemd/cgroups restore observation and cancellation of live work. Backup and restore are operational helpers, not another Runtime protocol.

## MCP surface

`workspace.open` isolates the checked-out code tree and Git state; it is not a host security sandbox.

```text
workspace.open
workspace.close
workspace.read
workspace.mutate
workspace.diff
workspace.exec
task.observe
task.cancel
task.list
artifact.read
```

`workspace.close` rejects dirty state unless `force=true`, and never removes a Workspace with active or held Jobs. `task.list` returns newest semantic summaries. `task.observe` retains tail mode while optional stdout/stderr byte offsets provide incremental reads; returned offsets remain UTF-8 boundaries. `artifact.read` uses the same bounded range semantics.

Mature host CLIs remain the preferred extension mechanism. A bespoke Adapter is justified only after repeated real use proves ordinary command execution insufficient.
