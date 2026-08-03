---
schema_version: 1
id: runtime.start
title: Ordivon Runtime
type: start
profile: organization
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - user
  - builder
  - operator
  - agent
updated: 2026-08-04
summary: Public entry to Runtime execution, evidence, reconciliation, recovery, and authority boundaries.
evidence_status: not_applicable
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.quickstart
  - runtime.status
  - runtime.model
  - runtime.effect-kernel
  - runtime.operations
  - runtime.data-privacy
  - runtime.releases
  - runtime.authority
---
# Ordivon Runtime

Ordivon Runtime is the trusted-local execution subsystem of Ordivon. It turns an admitted operation into a durable Job, owns its process tree, preserves bounded evidence, and makes interruption, cancellation, reconciliation, and recovery explicit.

It is **not** the complete Ordivon system. Host or another participant decides what work means and whether the user's objective is satisfied; Runtime owns the physical execution facts needed to make that judgment.

```text
participant or Host admits work
→ Runtime commits one operation identity
→ isolated Git Workspace + durable Job/Attempt
→ owned systemd/cgroup process tree
→ bounded Result and Artifacts
→ observation, cancellation, reconciliation, or recovery
```

## Status

Runtime is operational for owner-trusted Linux environments and is used for real Ordivon work. The public interface remains pre-1.0: current `main` is supported, persisted state and protocol compatibility are managed explicitly, and breaking public changes require a documented migration or cutover.

See [`docs/status.md`](docs/status.md) for the exact maturity claim, supported platform, known limits, and the commands that report live state.

## What it does

- creates detached Git Workspaces bound to an exact local revision;
- reads, writes, patches, diffs, and safely closes Workspace state;
- admits idempotent Jobs and ordered execution plans;
- binds execution to the observed source state before dispatch;
- owns one systemd unit and cgroup process tree per Attempt;
- bounds time, output, memory, process count, and CPU when requested;
- records digest-bound Results, Artifacts, and terminal supervision evidence;
- exposes Job observation, cancellation, listing, reconciliation, backup, restore, and repair paths;
- preserves ambiguous outcomes as `lost`, `orphaned`, or unknown instead of guessing success or redispatching opaque work.

The generated public Tool inventory is [`docs/reference/tools.md`](docs/reference/tools.md).

## What it does not do

Runtime does not own:

- Goal, Task, or semantic-completion decisions;
- model selection, Provider loops, cognition, or planning;
- hostile multi-tenant isolation;
- approval policy for arbitrary commands;
- domain-specific proof that an external-world effect occurred;
- a scheduler or hidden work queue.

`trusted_local` intentionally grants the installed service user's local authority. `contained_local` removes common ambient credentials, network access, capabilities, and host-state visibility, but it is not a hostile-code sandbox. Use an external disposable VM or container boundary for untrusted workloads.

## Requirements

The complete Runtime path requires:

- Linux with systemd as PID 1 and cgroup v2;
- Git;
- Rust 1.95.0, fixed by [`rust-toolchain.toml`](rust-toolchain.toml);
- Python 3.11 or newer for operational scripts;
- root or an equivalently privileged dedicated service account for the current trusted-local deployment model.

Portable unit and protocol checks run on ordinary Linux CI. Real process-supervision acceptance requires the local systemd/cgroup environment.

## Quick start

Build and run the portable verification contract:

```bash
cargo build --workspace --all-features
cargo test --workspace --all-targets --all-features
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_docs.py
scripts/local-acceptance check
```

Run the complete real-system journey on a disposable or owner-trusted machine:

```bash
sudo scripts/local-acceptance run
```

That command builds the Runtime and Runner, executes the ignored systemd/cgroup tests, starts a temporary loopback MCP server, and exercises Workspace creation, mutation, durable Patch replay, execution, observation, Artifact reading, cancellation, restart recovery, and safe closure.

For installation and receipted deployment, follow [`docs/quickstart.md`](docs/quickstart.md) and [`docs/operations.md`](docs/operations.md).

## First Runtime workflow

A normal client journey is:

```text
workspace.open
→ workspace.read / workspace.patch
→ workspace.exec or workspace.execPlan
→ task.observe
→ artifact.read
→ workspace.diff
→ workspace.get
→ workspace.close
```

After response loss, reuse the exact `clientRequestId` or reconnect through `workspace.list`, `workspace.get`, `task.list`, and `task.observe`. Do not create a new operation merely because delivery is uncertain.

## Responsibility boundary

| Concern | Canonical owner | Not owned here |
| --- | --- | --- |
| Workspace, Job, Attempt, process tree, Artifact, physical cancellation, execution recovery | `ordivon-runtime` | Task meaning or domain completion |
| durable Task continuity, Journal/CAS, commitment admission, verification records, Task outcomes | `ordivon-host` | physical process truth |
| Assignment, Agent Run, Provider adapter, model–Tool loop, Tool-step checkpoint, Run recovery | `ordivon-harness` | another Task database or Runtime supervisor |

The exact architecture and truth owners are defined in [`docs/runtime.md`](docs/runtime.md). The effect-commit contract is [`docs/effect-kernel.md`](docs/effect-kernel.md).

## Security and data

Read [`SECURITY.md`](SECURITY.md) before exposing the service or executing high-consequence work. The MCP origin must remain loopback-bound and authenticated; remote access must terminate at an operator-owned authenticated tunnel boundary.

Runtime persists commands, Job/Attempt state, bounded output, Artifacts, Workspace records, traces, and operational receipts. It does not automatically redact sensitive content. Storage, retention, export, migration, and deletion behavior are defined in [`docs/data-and-privacy.md`](docs/data-and-privacy.md).

## Documentation map

| Need | Start here |
| --- | --- |
| install and verify | [`docs/quickstart.md`](docs/quickstart.md) |
| current maturity and limits | [`docs/status.md`](docs/status.md) |
| architecture and truth owners | [`docs/runtime.md`](docs/runtime.md) |
| effect commitment and uncertainty | [`docs/effect-kernel.md`](docs/effect-kernel.md) |
| deployment, health, lifecycle, rollback | [`docs/operations.md`](docs/operations.md) |
| recovery and administrative repair | [`docs/recovery.md`](docs/recovery.md) |
| compatibility and deletion gates | [`docs/compatibility.md`](docs/compatibility.md) |
| security and vulnerability reporting | [`SECURITY.md`](SECURITY.md) |
| data, retention, export, deletion | [`docs/data-and-privacy.md`](docs/data-and-privacy.md) |
| versions and release gates | [`docs/releases.md`](docs/releases.md) |
| canonical document ownership | [`docs/authority.md`](docs/authority.md) |
| repository work by an Agent | [`AGENTS.md`](AGENTS.md) |

## Contributing

Read [`CONTRIBUTING.md`](CONTRIBUTING.md). A change must identify an observed failure or repeatedly missing operation, preserve one execution path, and include tests or live evidence that can falsify it.

## Project family

- [Public project directory](https://ordivon.com/projects) — reader-facing role, maturity, and next steps.
- [Cross-project map](https://github.com/zycxfyh/ordivon-computing/blob/main/projects/README.md) — stable roles, repository links, and authority entry points for all nine repositories.
- Related owners: [Ordivon Host](https://github.com/zycxfyh/ordivon-host) preserves durable Task continuity; [Ordivon Harness](https://github.com/zycxfyh/ordivon-harness) owns Assignment-scoped Agent Runs.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
