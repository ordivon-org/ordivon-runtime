# Ordivon Migration Contract v0

Status: supporting architecture evidence for M0. It does not enable execution,
route production traffic, or replace Desktop Commander.

## Purpose

Ordivon will migrate from the legacy Desktop Commander path by task journey,
not by counting tool names. The new runtime must prove that one complete agent
workflow is faster or more reliable before the corresponding legacy path is
retired.

The migration thin waist is:

```text
Task
Artifact
Backend Route
Performance Sample
Legacy Fallback Record
```

These contracts remain stable while the Ordivon backend grows from read-only
operations to a durable universal executor and later to promoted tools.

## Current backends

- `ORDIVON`: the new structured and eventually durable execution backend.
- `LEGACY_DESKTOP_COMMANDER`: the current general WSL operator route.

Backend selection is explicit and observable. A caller must not infer success
from an MCP connection, launcher process, or tool name.

## Routing rules

The router is deterministic and side-effect free.

1. If Ordivon supports the requested capability, select Ordivon.
2. Legacy fallback is opt-in, never implicit.
3. Automatic fallback is limited to read-only or task-workspace mutations.
4. Task-control, host, production, credential, network, or external side effects
   never automatically fallback.
5. Shadow comparison is limited to read-only operations and requires both
   backends to be available.
6. A route decision can deny execution without treating the request as invalid.

The first fallback policy is deliberately small:

```text
DENIED
WORKSPACE_ONLY
```

`WORKSPACE_ONLY` means the legacy backend may operate only inside the same
isolated migration workspace. It does not authorize production Bridge,
systemd, Cloudflare, remote Git, credential, or host mutations.

## Agent-facing Task contract

The migration-facing Task status vocabulary is intentionally smaller than the
internal Job and Attempt state machines:

```text
WORKING
INPUT_REQUIRED
COMPLETED
FAILED
CANCELLED
```

A working Task must provide a bounded poll interval and cannot claim a result.
An input-required Task must explain the requested input and must not ask the
model to keep polling. A terminal Task must expose a retrievable result and
cannot continue polling or request new input.

Artifacts are durable references rather than inline bulk output. Each reference
binds:

- artifact ID;
- parent Task ID;
- kind;
- SHA-256 digest;
- media type;
- byte length.

The first artifact kinds are stdout, stderr, diff, test report, execution
result, and other. Large output remains outside the model context until read.

## Performance evidence

Every comparable task journey records one sample per backend. The minimum
sample includes:

- success or failure;
- elapsed milliseconds;
- tool calls;
- remote round trips;
- context bytes;
- output bytes;
- disconnect recovery;
- fallback count.

The comparison function reports `Ordivon - legacy`. Negative latency, call,
round-trip, context, or output deltas therefore represent improvement.

## Initial migration gates

A task journey may become Ordivon-default only after fresh comparative evidence
shows:

- completion rate is not lower than legacy;
- tool calls are at least 30 percent lower, or the task has a documented
  reliability benefit that justifies the difference;
- context input is at least 50 percent lower for log-heavy work;
- disconnect recovery succeeds in every tested interruption;
- duplicate execution is not observed;
- elapsed time is no worse than 10 percent above legacy unless the reliability
  improvement is material and explicit.

These are migration gates, not universal product claims. Each journey requires
its own benchmark evidence.

## Fallback as demand evidence

Every legacy fallback must record:

- requested capability;
- operation class;
- selected legacy tool;
- missing or degraded Ordivon capability;
- elapsed time;
- outcome.

Fallback records are inputs to capability prioritization. Repeated fallback is
not hidden technical debt: it is measured evidence for the next primitive or
candidate tool.

Host and external side effects are excluded from automatic fallback records
because those operations require an explicit authorization path rather than a
performance router decision.

## Minimum guardrails during performance-first migration

M0 does not freeze a complete security architecture. It preserves only the
minimum controls required for trustworthy performance experiments:

- task work occurs in an isolated workspace;
- production Bridge, Cloudflare, host services, credentials, and remote Git are
  outside automatic routing;
- long work receives a durable Task handle;
- time, output, and process use are bounded before universal execution ships;
- every execution exposes result and artifact references;
- legacy fallback is explicit and measured.

These constraints prevent a fast prototype from corrupting the machine whose
performance it is supposed to measure.

## First vertical slice

M1 will cover one bounded coding journey:

```text
create isolated workspace
→ read files
→ write or patch one bounded file
→ execute one target test as a durable Task
→ read only the relevant output artifact
→ inspect the resulting diff
```

The journey must complete without production changes. The same journey will be
run through the legacy backend to produce the first comparative samples.

## Non-goals

M0 does not implement SQLite persistence, a Runner, process creation, automatic
legacy invocation, MCP Tasks, workspace creation, file mutation, tool
promotion, deployment, or production routing.

## Implemented code surface

`ordivon-exec::migration` now provides pure contracts and validation for:

- backend route requests and decisions;
- agent-facing Task handles;
- input-required states;
- artifact references;
- legacy fallback records;
- comparable performance samples and deltas.

The implementation performs no file, process, network, MCP, systemd, or legacy
backend calls. It is a migration contract, not a migration runtime.

## Next bounded stage

M1 should implement the smallest real universal-executor path behind these
contracts. It should begin with an isolated temporary workspace and a fixture
program before running a real repository build or exposing an MCP execution
tool.
