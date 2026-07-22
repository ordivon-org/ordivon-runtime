# Ordivon M6 Transactional Job and Attempt Control Plane

Date: 2026-07-22
Phase: ORDIVON-MIGRATION-M6-2026-07-22
Branch: agent/m6-transactional-job-attempt-control-plane
Design base: dfbfbe4a514bc28e6013b8475923bc87f5d411a3
Measured implementation: ffaca2f1989573d8d085d5cbeaa96b6a91726b35

## Status

M6 passes the bounded local transactional Dogfood gates. It does not modify the
production 8811 MCP route, expose Cloudflare, authorize credentials, perform
push or merge, or permit external side effects.

The correct decision is continued local transactional Dogfood eligibility. It
is not production cutover authorization.

## Control-plane model

M6 makes the following identities explicit:

- Job: stable user intent, policy and authority snapshot, and idempotency scope;
- Attempt: one concrete dispatch and execution lifecycle;
- Runner: one systemd unit and cgroup process tree;
- Artifact: immutable evidence produced by one Attempt;
- MCP Task: a protocol projection of the Job, not a second task record.

SQLite is the only Job and Attempt truth. Runner files remain mechanical
evidence and are validated before the Registry accepts them.

## Registry

The feature-gated Registry uses bundled SQLite with WAL, foreign keys,
`synchronous=FULL`, `trusted_schema=OFF`, a bounded busy timeout, and
`BEGIN IMMEDIATE` for all write transactions. The database and store use 0600
and 0700 permissions respectively.

The initial schema contains migrations, Jobs, Attempts, idempotency keys,
concurrency reservations, append-only events, Artifacts, and current Attempt
conditions. Migration checksum drift, newer schemas, corrupt databases, and
unknown stored enum values fail closed.

The `(principal, clientRequestId)` key is atomic. Identical operations replay
the original Job; changed operations produce `IDEMPOTENCY_CONFLICT` and do not
launch a second Attempt.

## Admission and dispatch

Admission creates the idempotency key, Job, initial Attempt, capacity
reservation, and initial events in one transaction. No bundle or process is
created before that commit.

The immutable bundle is reconstructed from Registry facts, written through a
private staging directory, synchronized, and atomically renamed. Dispatch
intent is committed before `systemd-run` crosses the at-most-once boundary.

If dispatch outcome is ambiguous and no matching launch-token, unit, Runner
start, or result evidence exists, the Attempt becomes `lost`. M6 does not
silently redispatch the same Attempt.

## Runner identity

The existing sandboxed Runner is reused. M6 adds optional Job, Attempt, launch
token, and unit identity without changing the M1-M5 wire shape when those
fields are absent.

`PrivatePIDs` and restricted `/proc` remain enabled. The Runner proves its
namespace identity, launch token, InvocationID, and cgroup. Core obtains the
host MainPID, host process start identity, and boot ID outside the sandbox.

## Terminal and cancellation transactions

A terminal transaction atomically records the Attempt state, immutable result
identity, Artifact metadata, result condition, reservation disposition, Job
resolution, and append-only events. Repeated observation is idempotent only
when the state and result digest match.

Cancellation persists desired state before stopping the unit. Cancellation
survives Core reconstruction and removes the complete cgroup. Orphaned
Attempts retain capacity as `held_orphaned` until remediation proves that the
process tree is gone.

## Reconciliation

Startup, targeted query, and explicit reconciliation compare SQLite, bundle
files, Runner start evidence, result evidence, systemd, cgroup, boot ID, and
host process identity.

The real systemd matrix proves:

1. transactional success, replay, Artifact registration, and capacity release;
2. Core reconstruction while a Task is running;
3. persistent cancellation and cgroup cleanup;
4. ambiguous dispatch becoming lost without redispatch;
5. live unbound units becoming orphaned while holding capacity;
6. bundle reconstruction after admission commit;
7. corrupt result quarantine;
8. ten consecutive fast failures becoming failed rather than racing into lost.

## MCP projection

The independent `ordivon-m6-http` server exposes nine tools on an experimental
loopback endpoint. `workspace.exec` accepts a client request identity but no
client Task ID. The server-generated Job ID is the Native MCP Task ID.

`tasks/get`, `tasks/result`, `tasks/list`, `tasks/cancel`, `task.observe`,
`task.list`, and `artifact.read` all project the same SQLite truth. MCP Session
state and projection files are not task truth.

## Wire and security evidence

The final Wire run used 64 concurrent reads. It produced 73 Core trace rows and
87 HTTP trace rows without duplicate trace identities. Typical successful tool
results remained below the frozen response budgets.

Transport negative paths returned 401 for missing or invalid bearer identity,
403 for disallowed Origin and Host, and 413 for an oversized body.

## Limited Dogfood

Eight local MCP journeys passed with 39 model-facing calls, 19,817 context
bytes, 4,818 consumed output bytes, one repair round, and zero Legacy fallback.
The matrix includes read-only audit, guarded mutation, multi-file test,
failure-repair, a real Rust target test, bounded large-log Artifact retrieval,
disconnect recovery and cancellation, and idempotency/list behavior.

A separate model-in-the-loop journey allowed the model to read actual lock and
implementation content, select tools, author an isolated validator and three
negative tests, execute them as a server-generated Job, inspect the delta, and
verify the Registry projection. This is one bounded model journey, not a broad
autonomous-agent claim.

## Concurrency

Real 2, 4, and 8 Job batches completed in parallel. Each Job had unique server
identity. The additional request at the final capacity slot returned
`CONCURRENCY_LIMIT`. All 14 reservations were released and active capacity was
zero after completion.

## Registry performance

Registry-only p95 results on the reference WSL environment:

| Operation | p95 | Budget |
|---|---:|---:|
| new admission | 20.30 ms | 50 ms |
| idempotent replay | 0.97 ms | 20 ms |
| status projection | 0.86 ms | 20 ms |
| list 100 Jobs | 5.11 ms | 50 ms |
| terminal commit | 21.15 ms | 50 ms |

All Registry budgets pass with `synchronous=FULL` retained.

## M5 versus M6 Shadow

Twelve alternating pairs across read-only audit, one-file edit, multi-file
test, and failure-repair produced identical semantic digests.

Overall medians:

| Metric | M5 file backend | M6 Registry backend |
|---|---:|---:|
| elapsed time | 509 ms | 603 ms |
| tool calls | 5 | 5 |
| context bytes | 1,687 | 1,754 |
| HTTP requests | 5 | 5 |
| repair rounds | 0 | 0 |
| fallback | 0 | 0 |

M6 adds about 18.5 percent median latency and 4.0 percent Context in this
bounded matrix. Both remain within the frozen 25 percent transactional budget.
The cost buys atomic idempotency, capacity reservation, Job/Attempt truth,
terminal transactions, and recovery classification.

During Shadow evaluation, an extremely fast failed process was once classified
as lost because Runner evidence appeared after the first file check. M6 now
rechecks bundle evidence after systemd observation and before committing lost.
Ten consecutive fast-failure integration runs pass after the repair.

## Decision

M6 authorizes continued bounded local transactional Dogfood. It does not
authorize production routing, remote exposure, credentials, deployment,
network access, push, merge, or external side effects.

The next structural risks are a dedicated non-privileged Worker, actual WSL
reboot recovery, retention and garbage collection, stronger capability
policy enforcement, and broader model-driven workloads. These must remain
separate from M6 evidence closure.

## Evidence

- `ordivon-m6-wire-contract-evidence-2026-07-22.json`
- `ordivon-m6-transport-security-evidence-2026-07-22.json`
- `ordivon-m6-limited-dogfood-evidence-2026-07-22.json`
- `ordivon-m6-concurrency-evidence-2026-07-22.json`
- `ordivon-m6-registry-shadow-evidence-2026-07-22.json`
- `ordivon-m6-registry-performance-evidence-2026-07-22.json`
- `ordivon-m6-model-loop-evidence-2026-07-22.json`
- `ordivon-m6-runtime-crash-matrix-evidence-2026-07-22.json`
- `scripts/mcp/check_m6_evidence.py`

The independent checker recomputes all material summaries and gates. A modified
Shadow median is rejected.
