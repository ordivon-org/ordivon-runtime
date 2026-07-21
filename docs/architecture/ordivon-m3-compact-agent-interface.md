# Ordivon M3 Compact Agent Interface

Date: 2026-07-22
Phase: ORDIVON-MIGRATION-M3-2026-07-22
Branch: agent/m3-compact-agent-interface
Base: 3893ae4e462b02eb60e37e188269a75bd238c2bc
Measured implementation: e1a40df3878881e47a56fd428ec6eb316301799b

## Status

M3 is complete as a feature-gated local implementation and differential
evaluation. It does not expose an Ordivon MCP execution tool and does not
switch production routing.

M3 resolves the M2 agent-facing regression. The full-read journey now uses
fewer calls, returns less context, runs faster, and preserves durable recovery.
A second targeted-read journey meets the stricter Agent-native context budget.

No production Bridge, Cloudflare route, service, port, or MCP tool table was
changed.

## Problem established by M2

M2 proved that the M1 mechanical substrate was faster and recoverable, but its
model-facing protocol was inefficient.

| Metric | Legacy | M1 |
|---|---:|---:|
| Median wall time | 973 ms | 723 ms |
| Logical calls | 7 | 10 |
| Context bytes | 6,441 | 9,355 |
| Caller-disconnect recovery | no | yes |

The bottleneck was the separate task start, task observation, and two mandatory
Artifact reads, plus repeated workspace and Artifact metadata. M3 therefore
optimizes the Agent-facing projection rather than systemd or Runner startup.

## Implemented operations

M3 adds compact alternatives while retaining the M1 diagnostic operations:

- task-run starts a durable Task, waits locally, and returns bounded output tails;
- task-await reconstructs the same compact terminal observation later;
- workspace-mutate applies a bounded batch of write, append, or exact-replace operations;
- workspace-read-slice-compact returns a bounded slice and whole-file digest;
- workspace-open and compact read/diff projections omit repeated metadata.

The original task-start, task-get, artifact-read, workspace-read, and
workspace-diff operations remain available for diagnostics and large outputs.
Compact operations do not delete or weaken the underlying durable Artifacts.

## Compact Task semantics

A successful task-run response contains only the fields needed for the model's
next decision: Task ID, status, exit code, bounded stdout and stderr tails, and
whether complete Artifacts remain available.

If the wait budget expires, task-run returns WORKING with a polling hint. The
Task remains recoverable through task-await or the diagnostic task-get path.
Caller disconnection does not cancel the systemd-owned Task.

Small output is inlined. Retention limits, dropped-byte accounting, result.json,
and full stdout and stderr Artifacts remain unchanged inside the Runner.

## Batch mutation semantics

A batch supports at most 32 distinct paths. Every path, expected digest, exact
replacement condition, and final byte limit is checked before the first file is
written. Duplicate paths are rejected.

Each file is written through the existing atomic single-file writer. If a later
write fails, already applied files are rolled back. A rollback failure is
reported as WORKSPACE_MUTATION_INCOMPLETE rather than hidden as an ordinary
mutation failure.

## M3A full-read evaluation

M3A preserves the original M2 task semantics, including reading the complete
README before mutation. One warm-up pair and five measured pairs alternated
backend order.

Legacy median: 926 ms, 7 calls, and 6,447 context bytes.
Ordivon M3A median: 707 ms, 6 calls, and 6,205 context bytes.

Ordivon was about 23.7 percent faster, used one fewer call, and returned about
3.8 percent less context. All five pairs produced matching semantic digests.
M3A passes its bounded gates: at most six calls, no context regression,
acceptable latency, recovery, no fallback, and semantic equivalence.

## M3B targeted-read evaluation

M3B reads only a 64-byte slice plus the whole-file digest, batches related
mutations, inlines small terminal output, and validates the result through Task
output and workspace diff.

Optimized Legacy median: 1,015 ms, 5 calls, and 1,491 context bytes.
Ordivon M3B median: 694 ms, 5 calls, and 1,409 context bytes.

Ordivon was about 31.6 percent faster, used the same number of calls, returned
about 5.5 percent less context, and remained recoverable after caller
disconnection. Its 1,409 context bytes are below the 3,220-byte M3B budget.
All five pairs produced matching semantic digests.

## Safety and durability

M3 does not modify the M1 Runner or systemd sandbox. Executions still use an
exact executable plus argv, a cleared environment, private network and process
views, a read-only host view, bounded resources, and cgroup-wide stop.

The real M3 integration test completed the primary journey in six independent
CLI calls and recovered the terminal Task from a later CLI process. Full result
and output Artifacts remained available.

## Decision

M3 closes the Agent-facing performance regression found by M2. The local
interface now satisfies both full-read and targeted-read performance budgets.

This result does not authorize a production route switch. Ordivon is still a
local feature-gated CLI while Legacy is a real MCP endpoint. M4 must expose an
independent experimental Ordivon MCP adapter and repeat MCP-to-MCP evaluation.

## Remaining structural debt

- the Task registry remains a non-transactional directory tree;
- Job and Attempt are not yet separated in the M1 runtime;
- the worker identity is not yet a dedicated non-privileged account;
- Task listing, retention, garbage collection, and full reconciliation are absent;
- network and credentials have no scoped opt-in;
- only local Python journeys have been differentially measured;
- no production MCP execution tool or route is active.

## Evidence

- docs/audits/runtime/ordivon-m3a-differential-evidence-2026-07-22.json
- docs/audits/runtime/ordivon-m3b-differential-evidence-2026-07-22.json
- scripts/benchmarks/check_m3_evidence.py

The next bounded stage is M4: an independent experimental MCP adapter, followed
by a fresh MCP-to-MCP differential evaluation before any Dogfood routing.
