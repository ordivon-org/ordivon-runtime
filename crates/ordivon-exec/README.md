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

## Explicitly not implemented

- patch or write operations;
- process creation, supervision, persistence, or durable-job MCP tools;
- MCP, HTTP, Cloudflare, or Hermes adapters;
- search continuation cursors or hard execution timeout;
- authorization, sealed receipts, or runtime observation emission.

The current crate is a local read-only core slice. Performance claims require a
separate benchmark against the legacy Desktop Commander route.
