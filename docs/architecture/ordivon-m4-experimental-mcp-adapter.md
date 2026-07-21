# Ordivon M4 Experimental MCP Adapter

Date: 2026-07-22
Phase: ORDIVON-MIGRATION-M4-2026-07-22
Branch: agent/m4-experimental-mcp-adapter
Base: 405fe2ec57d90456f5811b6912e2beaf2ef95bf3
Measured implementation: 4592689dc9183fcb08f4828d3d752a4cf57e318f

## Status

M4 is complete as an independent, feature-gated, localhost-only Streamable HTTP
MCP adapter and differential evaluation. Both bounded MCP-to-MCP benchmark
journeys pass their local eligibility gates.

This result authorizes only a later limited Dogfood stage. It does not modify
the production Desktop Commander bridge, port 8811, Cloudflare routes, deployed
service definitions, or default tool routing.

## Problem inherited from M3

M3 proved that compact Ordivon operations were faster, recoverable, and within
the model-facing call and context budgets. The comparison remained asymmetric:
Legacy used a real Streamable HTTP MCP endpoint while Ordivon used a local CLI.
M4 removes that transport asymmetry.

## Architecture

```text
MCP client
  -> localhost Streamable HTTP front door
  -> schema, authentication, Host and Origin validation
  -> stateless M4 protocol adapter
  -> ordivon-exec Core
  -> durable Task store
  -> systemd and cgroup Runner
  -> compact Observation or complete Artifact
```

The MCP Server is a protocol adapter, not the Task owner. HTTP or MCP Session
state is not execution truth. Workspace IDs and Task IDs are explicit durable
handles generated outside transport state.

Server restart therefore does not recreate or duplicate a Task. The adapter
reconstructs MCP Task status and results from the same Ordivon Task store.

M4 uses the classical control plane for schemas, admission, durable identity,
state classification, cancellation, evidence, and resource boundaries. The
Agentive plane retains universal program generation, workspace mutation,
execution, observation, and repair.

## Tool thin waist

M4 exposes exactly eight tools:

- `workspace.open`
- `workspace.read`
- `workspace.mutate`
- `workspace.diff`
- `workspace.exec`
- `task.observe`
- `task.cancel`
- `artifact.read`

Read mode, projection, slice bounds, and diagnostic depth are parameters rather
than separate tools. Only `workspace.exec` declares MCP `taskSupport=optional`.
The other seven tools reject task-augmented invocation.

Tool results use strict input and output schemas, structured content, stable
error identities, and a minimal compatibility text surface. Full structured
JSON is not duplicated into text content, and normal results do not inject
trace objects into model context.

## Native MCP Tasks and compatibility handles

A client that supports MCP Tasks may invoke `workspace.exec` as a Task and use:

```text
tasks/get
tasks/result
tasks/list
tasks/cancel
```

A client without MCP Tasks receives the same Ordivon Task ID through the
ordinary compact result and can use `task.observe` or `task.cancel`.

Both protocol modes project one underlying Task. The native projection stores
only bounded MCP presentation settings such as output-tail limits and TTL. It
cannot change execution state or create a second source of truth.

Completed, failed, and cancelled Tasks produce distinct protocol states.
`tasks/result` returns structured tool errors for failed or cancelled work
instead of presenting those terminal states as successful results.

## Transport boundary

The experimental Server is available only with the `experimental-http-m4`
feature and a separate binary. Without that feature, the binary cannot build.
The existing stdio MCP service remains the default crate behavior.

The M4 Server:

- binds only to a loopback address;
- uses an independent experimental port;
- requires a bearer token of at least 32 characters;
- validates Host to resist local DNS rebinding;
- validates configured browser Origins;
- rejects oversized request bodies before MCP dispatch;
- uses stateless JSON Streamable HTTP responses;
- records Core and HTTP traces without credentials.

The formal evaluation used `127.0.0.1:8895`. Production port 8811 was used only
as the unchanged Legacy comparison endpoint.

## M4A full-read MCP evaluation

M4A preserves the complete-file-read journey. Both backends used MCP SDK 1.29.0
through real Streamable HTTP. Three warm-up pairs preceded twenty measured
pairs, and backend order alternated. Every pair produced the same
backend-neutral semantic digest.

| Metric | Legacy MCP | Ordivon M4 | Change |
|---|---:|---:|---:|
| Median wall time | 924 ms | 698 ms | 24.5% lower |
| p95 wall time | 1006 ms | 710 ms | 29.4% lower |
| Logical calls | 7 | 6 | one fewer |
| Task HTTP requests | 8 | 6 | two fewer |
| Context bytes | 7192 | 7113 | 1.1% lower |
| Caller-disconnect recovery | no | yes | improved |

M4A passes completion, call, context, median, p95, HTTP request, recovery,
fallback, and semantic-equivalence gates.

## M4B targeted-read MCP evaluation

M4B uses a 64-byte targeted read plus whole-file digest, one bounded batch
mutation, compact execution output, and compact diff. Legacy was also reduced
to five generic operations.

| Metric | Legacy MCP | Ordivon M4 | Change |
|---|---:|---:|---:|
| Median wall time | 1010 ms | 691 ms | 31.6% lower |
| p95 wall time | 1031 ms | 727 ms | 29.5% lower |
| Logical calls | 5 | 5 | equal |
| Task HTTP requests | 6 | 5 | one fewer |
| Context bytes | 1534 | 1604 | 70 bytes higher |
| Caller-disconnect recovery | no | yes | improved |

M4B remains below the fixed 3220-byte Agent-native context budget and passes
all bounded gates. The small Context increase versus optimized Legacy is
reported rather than hidden.

## Performance corrections discovered during M4

The first real MCP smoke exposed three adapter overheads that local CLI tests
could not reveal:

1. structured results were serialized into both text and structured content;
2. per-call trace objects were returned to the model despite durable trace files;
3. empty success compatibility text still consumed response structure.

These were removed in separate performance commits. The final adapter returns
one structured machine result, keeps durable traces server-side, and retains
only the minimum compatibility surface required by MCP clients.

This is why the final M4A Context result moved from an initial regression to a
small improvement over Legacy without deleting execution evidence or weakening
the Runner.

## Resilience and transport evidence

The formal resilience probe started a native MCP Task, disconnected the client,
restarted the experimental Server, and recovered the completed Task from a new
client. A second native Task was cancelled and projected as `cancelled` with a
`TASK_CANCELLED` result. Both Task cgroups and target processes were absent
after cleanup.

The transport probe established:

- missing authorization -> 401;
- invalid authorization -> 401;
- disallowed Origin -> 403;
- disallowed Host -> 403;
- oversized request body -> 413.

Core and HTTP traces use process-wide unique IDs. The independent checker found
no duplicate IDs or bearer material in the sealed trace snapshots.

## Decision

M4 closes the transport asymmetry left by M3. For the two bounded local coding
journeys, the real Ordivon MCP path is faster at both p50 and p95, uses no more
logical calls or task HTTP requests, stays within its Context budgets, preserves
semantic equivalence, and recovers after client or Server disconnection.

The correct status is:

```text
LOCAL_MCP_GATES_PASS_LIMITED_DOGFOOD_ELIGIBLE_NO_PRODUCTION_CUTOVER
```

This permits designing M5 limited Dogfood and shadow routing. It does not permit
production routing, remote exposure, external side effects, or removal of the
Legacy fallback.

## Remaining structural debt

- the filesystem Task registry is still non-transactional;
- Job and Attempt are not yet separate durable entities;
- the local Worker still lacks a dedicated non-privileged identity;
- Task retention, garbage collection, full reconciliation, and quotas are absent;
- network and credential capabilities remain closed rather than scoped;
- benchmark coverage is limited to local Python coding journeys;
- concurrency, WSL reboot, long builds, disk pressure, and large-output matrices remain pending;
- the experimental Server has no production authentication, OAuth, Cloudflare, or deployment contract;
- no production route or high-risk external action is authorized.

## Evidence

- `docs/audits/runtime/ordivon-m4a-mcp-differential-evidence-2026-07-22.json`
- `docs/audits/runtime/ordivon-m4b-mcp-differential-evidence-2026-07-22.json`
- `docs/audits/runtime/ordivon-m4-resilience-evidence-2026-07-22.json`
- `docs/audits/runtime/ordivon-m4-transport-security-evidence-2026-07-22.json`
- `docs/audits/runtime/ordivon-m4-core-trace-2026-07-22.jsonl`
- `docs/audits/runtime/ordivon-m4-http-trace-2026-07-22.jsonl`
- `scripts/benchmarks/check_m4_evidence.py`

The next bounded stage is M5: limited low-risk Dogfood and shadow routing with
explicit Legacy fallback evidence. M5 must not silently expand authority.
