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

## Verified capability evaluator

P3 loads a bounded deny-by-default policy, rejects duplicate JSON keys and
unsafe file permissions, canonicalizes roots and CWD, verifies a non-symlink
executable and SHA-256, enforces exact argv/environment/runtime/output and
concurrency limits, and emits a closed `ExecutionPlan`. It never starts the
plan. See `docs/architecture/ordivon-capability-scope-v0.md`.

The tracked local Cargo policy is machine-bound supporting configuration. Its
presence does not activate execution, and toolchain or workspace drift makes
it fail closed.

## Explicitly not implemented

- patch or write operations;
- process creation, supervision, persistence, or durable-job MCP tools;
- MCP, HTTP, Cloudflare, or Hermes adapters;
- search continuation cursors or hard execution timeout;
- authorization, sealed receipts, or runtime observation emission.

The current crate is a local read-only core slice. Performance claims require a
separate benchmark against the legacy Desktop Commander route.
