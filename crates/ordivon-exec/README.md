# ordivon-exec

Deterministic local execution primitives for AI agents.

This crate is not an agent harness and does not call a model. It converts typed
requests into bounded local operations. Protocol adapters such as MCP remain
outside the core contract.

## Implemented read-only scope

- `read_text`: 1-based ranges, byte budget, full-file SHA-256 revision,
  continuation metadata, and UTF-8-only fail-closed behavior.
- `read_many`: bounded batch reads with one total byte budget and independent
  item failures.
- `search_text`: bounded structured search over ripgrep JSON output, fixed or
  regex patterns, explicit globs, deterministic path order, and no shell.
- `repo_snapshot`: one Git porcelain-v2 call returning branch, exact HEAD,
  upstream distance, and staged/unstaged/untracked/conflict counts.

## Frozen durable-job contract

P1 defines pure, non-executing contracts for capability profiles, structured
job requests, resolved execution plans, internal/public state projection,
job records, bounded output cursors, list pagination, stable error codes, and
operational receipt events. The contract rejects unknown input fields and does
not expose a shell command or executable in `JobStartRequest`.

See `docs/architecture/ordivon-durable-job-contract-v0.md`. Contract presence
does not authorize or implement process execution.

## Verified supervisor spike

P2 proves locally that systemd transient services and cgroup v2 can own a Job
process tree after the launcher exits and can terminate a SIGTERM-resistant
root/child/grandchild tree. Recovery identity includes kernel boot ID, unit,
InvocationID, cgroup, PID, and process starttime. Successful units can be
systemd-garbage-collected, so a production runner must atomically persist its
terminal result before exit.

The fixture and real systemd tests require the explicit `supervisor-spike`
feature and are not part of the normal release target. See
`docs/architecture/ordivon-job-supervisor-spike-v0.md`.

## Migration contract

M0 adds pure contracts for new-versus-legacy backend routing, agent-facing Task
handles, Artifact references, fallback observations, and comparative performance
samples. Ordivon is preferred when available; automatic legacy fallback is
limited to read-only or isolated-workspace operations, and shadow comparison is
read-only. The module performs no execution or legacy calls.

See `docs/architecture/ordivon-migration-contract-v0.md`.

## Verified capability evaluator

P3 loads a bounded deny-by-default policy, rejects duplicate JSON keys and
unsafe file permissions, canonicalizes roots and CWD, verifies a non-symlink
executable and SHA-256, enforces exact argv/environment/runtime/output and
concurrency limits, and emits a closed `ExecutionPlan`. It never starts the
plan. See `docs/architecture/ordivon-capability-scope-v0.md`.

The tracked local Cargo policy is machine-bound supporting configuration. Its
presence does not activate execution, and toolchain or workspace drift makes
it fail closed.

## Minimal universal executor M1

M1 adds a feature-gated local vertical slice for AI-authored programs. It can
create a detached Git workspace, perform digest-bound text writes, run an
absolute executable plus argv as a durable systemd Task, recover Task state
from later CLI processes, retain bounded stdout/stderr/result Artifacts, cancel
a cgroup-owned Task, and report tracked diff plus untracked paths.

The target process receives a cleared environment, private network, bounded
resources, a read-only system view, and write access only to the workspace and
Task directory. The real integration test proved completion, cancellation,
cross-process recovery, Artifact reads, and denial of host `/etc` writes and
systemd control.

The binaries require the `universal-executor-m1` feature and are not exposed by
the current MCP server. The filesystem Task registry is experimental evidence,
not the future transactional registry. See
`docs/architecture/ordivon-universal-executor-m1.md`.

## Explicitly not implemented

- production MCP, HTTP, Cloudflare, or Hermes execution adapters;
- a transactional Task registry, atomic idempotency, or concurrency slots;
- Task listing, retention, garbage collection, or complete orphan recovery;
- scoped network opt-in, credential delegation, or non-root worker identity;
- binary Artifact decoding or model-oriented log summarization;
- authorization, sealed receipts, or production runtime observation emission.

M1 is a feature-gated local execution slice. Performance claims against the
legacy Desktop Commander route still require the M2 comparative benchmark.
