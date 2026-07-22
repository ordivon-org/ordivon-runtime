# Ordivon Runtime Architecture

## Purpose

Ordivon is the durable execution substrate beneath a capable Agent. The Agent reasons and chooses actions; Ordivon executes, owns the process tree, records reality, and recovers after interruption.

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
|---|---|
| Repository history, branch, diff, rollback | Git |
| Request identity, dispatch state, cancellation, terminal resolution | SQLite Registry |
| Running process tree and termination | systemd and cgroup |
| Output, result, and Artifact bytes | bounded files bound by Digests |
| Current architecture and next state change | `docs/current-state.md` |

Everything reconstructable from these sources should remain reconstructable rather than becoming another persistent Registry or Receipt.

## Authority

`trusted-local` is the default. A task inherits the Ordivon service user's network, environment, credentials, filesystem access, Git remotes, Docker, systemd, cloud tooling, and executable surface. The server binds the Principal and global concurrency limit once from deployment configuration. That limit is shared by every MCP client, while admission permits at most one active `workspace.exec` per Workspace so independent worktrees can run concurrently without allowing two arbitrary command trees to mutate the same worktree. Local HTTP uses a fixed Bearer token; an explicitly enabled Cloudflare Access path accepts the Access JWT assertion at the loopback origin. Policy, profile, authority-reference, and one-value plan-kind fields are not persisted because they do not contribute to recovery.

`isolated` is an explicit reduced-authority mode for untrusted input. It activates the non-root worker, private network and filesystem views, hidden credentials and host-control paths, and systemd hardening.

## Runtime invariants

These mechanisms preserve reality rather than govern Agent judgment:

- idempotent admission for repeated client requests;
- transactional capacity admission across all clients, with one active command tree per Workspace;
- retryable capacity errors that identify global or Workspace scope without creating a second queue;
- no speculative redispatch after an ambiguous launch;
- one owned cgroup for the complete process tree;
- persisted cancellation intent before termination;
- timeout and bounded retained output;
- Digest-bound executable, result, and Artifact identity;
- startup and observation-time reconciliation.

## Recovery semantics

A complete identity-bound Runner result is terminal truth even when a short-lived transient unit has already been collected. If supervisor observation first marks an Attempt `orphaned` and the trusted Runner result appears later, startup, admission, observation, or listing may correct the Attempt and Job to the Runner terminal state and release the held reservation atomically—but only after the recorded unit, PID identity, and cgroup no longer prove a live process tree. A malformed or identity-conflicting result remains quarantined as `orphaned`. A dispatch that cannot be proven running or terminal becomes `lost` or `orphaned`; it is not automatically repeated.

Git restores code. SQLite and result bundles restore task knowledge. systemd/cgroups restore observation and cancellation of live work. Backup and restore are operational helpers, not a second Runtime protocol.

## MCP surface

`workspace.open` isolates the checked-out code tree and Git state only. It does not define host filesystem, network, credential, or executable authority; those remain properties of the configured Runtime mode described above.

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

Mature host CLIs remain the preferred extension mechanism. A bespoke Adapter is justified only when repeated real use proves that ordinary command execution cannot provide the required semantics.
