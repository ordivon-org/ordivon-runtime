# Ordivon Runtime Architecture

## Purpose

Ordivon is a local execution runtime for capable agents. It makes reversible repository work fast while preserving the small set of facts that reasoning cannot reconstruct after a crash or concurrent race.

## Active execution chain

```text
MCP request
→ exact Git workspace
→ validated execution plan
→ SQLite admission transaction
→ Job + Attempt + concurrency reservation
→ immutable runner bundle
→ systemd transient unit / cgroup
→ bounded stdout, stderr, result, and artifacts
→ reconciliation
→ terminal Job projection
```

## Persistent truth

The SQLite Registry owns:

- idempotency through principal and client request identity;
- Job desired state and terminal resolution;
- Attempt lifecycle and row-version transitions;
- launch, process, boot, unit, invocation, and cgroup identity;
- global and profile concurrency reservations;
- result and Artifact identity.

The filesystem stores immutable execution bundles, bounded output, runner-start evidence, terminal results, and Artifacts. Digests bind those files back to the Registry.

## Execution ownership

A task runs as a systemd transient unit. The runtime records the unit, invocation, cgroup, PID, process-start identity, boot identity, and launch-token evidence. Cancellation persists intent before stopping the unit and reconciling the complete process tree.

Timeout and output bounds remain runtime controls because they protect machine resources rather than police code style.

## Recovery semantics

At startup and observation time, Ordivon reconciles nonterminal Attempts against:

- committed Registry state;
- runner-start evidence;
- result files;
- current boot identity;
- systemd unit and cgroup state;
- process identity.

A dispatch whose outcome cannot be proven is not automatically retried. It becomes `lost` or `orphaned` so an external side effect cannot be duplicated by speculation.

## MCP surface

The transactional surface is:

```text
workspace.open
workspace.read
workspace.mutate
workspace.diff
workspace.exec
task.observe
task.cancel
task.list
artifact.read
```

The public server and binary are `ordivon-mcp`. Runtime modules, features, tests, and schema types are unversioned; M-series names remain only in the two historical closure documents and Git history.

## Current simplification boundary

The following are not runtime dependencies:

- Python governance services;
- document registries or wording gates;
- Receipts and staged evidence machinery;
- PostgreSQL, NATS, Temporal, OpenFGA, OPA, Redis, or DuckDB;
- M-series benchmark and dogfood harnesses.

Historical implementations and proof artifacts remain recoverable through Git tag `m-series-closed-2026-07-22`.
