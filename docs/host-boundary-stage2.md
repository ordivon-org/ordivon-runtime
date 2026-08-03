# Host Boundary Stage 2

> **Historical boundary evidence:** This report preserves the Stage 2 Host/Runtime correlation experiment and closeout. It is not a current architecture or policy source. Current Runtime ownership is defined by [`runtime.md`](runtime.md), [`operations.md`](operations.md), and [`authority.md`](authority.md); current Host and Harness semantics live in their own repositories.

Status: historical evidence; R1 and R2 completed without Runtime production expansion
Canonical experiment: `ordivon-computing/research/experiments/harness-boundary-v0/`

## R1 result

The frozen vector passes without Runtime production changes. Four opaque `ordivon.host` references bind one Host Task, Host Task Attempt, Assignment generation, and Harness Run to one Runtime Job and Runtime Attempt. Exact replay returns the original Job and creates no second Attempt; changing the Assignment generation or digest under the same `clientRequestId` produces `IdempotencyConflict`; terminal evidence preserves the exact references plus Job, Runtime Attempt, Workspace, source revision, and executable; and a fresh Registry instance locates the same Job and evidence through the exact request identity.

The vector also confirms the negative boundary: terminal evidence records `executionDisposition=succeeded` but contains no semantic-completion or TaskOutcome claim. No Registry migration, Job or Attempt field, Host-specific schema enum, observation projection, Tool rename, foreign-reference index, service, or production Runtime branch was required.

## R2 live result

Runtime independently consumes the committed Host receipt [`../evidence/harness-h2-runtime-r2-live-d50f609-20260731.json`](../evidence/harness-h2-runtime-r2-live-d50f609-20260731.json). The request was emitted by Host implementation revision `d50f609ef2d269b5adc36e7e2088400c1971d0eb` against the active Runtime and bound one real Job and Runtime Attempt.

The live trajectory matched the R1 fixture: exact replay returned the original Job; changing Assignment generation or digest under the old `clientRequestId` produced an idempotency conflict; terminal evidence retained the exact four Host references and bound Job, Attempt, Workspace, executable, and source revision; a fresh client recovered the same Job; Runtime made no semantic-completion or TaskOutcome claim; and the Workspace closed. The Runtime-side checker verifies receipt integrity, all thirteen live assertions, exact identity equality, reference order, terminal evidence, and the Host handoff projection.

No R1 falsifier occurred. Runtime therefore retains the existing opaque `foreignReferences`, request identity, Job lookup, and terminal-evidence mechanisms. Canonical construction remains Host-local. Stage 2 adds no Runtime production code, state owner, projection, query index, service, or Tool alias.

## Objective

Prove that the current Runtime contract can bind a Host Task, Task Attempt, Assignment, and Harness Run to one physical Job and Runtime Attempt without importing Host state, adding a second correlation database, or renaming the public `task.*` Tools.

The expected outcome is a small documentation-and-conformance change. Runtime already implements the required mechanical field: immutable `foreignReferences` inside `RuntimeExecutionPlan`.

## Current mechanism

```rust
pub struct ForeignReference {
    pub namespace: String,
    pub reference_type: String,
    pub id: String,
    pub generation: Option<String>,
    pub digest: Option<String>,
}
```

Current guarantees:

- at most sixteen references per operation;
- uniqueness by namespace, type, and ID;
- bounded identifier, generation, and digest validation;
- references participate in operation request identity;
- same `clientRequestId` plus changed references conflicts;
- references are persisted in the immutable Execution Plan;
- terminal evidence contains the references;
- Job and Runtime Attempt identity remain Runtime-owned.

These guarantees are sufficient unless the cross-layer conformance vector proves otherwise.

## Canonical Host references

Namespace:

```text
ordivon.host
```

Reference types:

| type | identity owner | generation | digest |
|---|---|---|---|
| `task` | Host | Assignment-bound Task revision | Host Task runtime-binding digest |
| `task_attempt` | Host | omitted | immutable Task-Attempt descriptor digest |
| `assignment` | Host | Assignment generation | immutable Assignment digest |
| `harness_run` | Harness adapter, admitted by Host | omitted | pre-completion Run binding digest |
| `effect` | Host Effect owner | omitted | Effect digest |
| `dispatch` | Host execution owner | omitted | Dispatch or exact request digest |

A Harness-originated Runtime operation carries the first four. Effect and Dispatch are added when the operation implements an admitted Effect.

Runtime treats every reference as opaque identity-bound input. It does not interpret Task state, Assignment validity, Harness capability, or semantic completion.

## Operation identity behavior

Foreign references are part of `operation_request_identity_digest`.

Therefore:

```text
same principal
+ same clientRequestId
+ same execution request
+ same foreign references
→ return the original Job
```

and:

```text
same clientRequestId
+ changed Assignment generation or digest
→ request identity conflict
```

A replacement Harness receives a new Assignment and forms a new Runtime request identity for new physical work. It observes the old Job when reconciling an uncertain prior operation instead of changing the old request.

## Cross-layer conformance vector

Fixture identities:

```text
Task             task:fixture
Task revision    7
Task Attempt     task-attempt:fixture:1
Assignment       assignment:fixture:1:g1
Generation       1
Harness Run      harness-run:codex:1
clientRequestId  request:harness-fixture:g1:step1
```

### Admission

Submit one deterministic execution with four sorted Host references:

```json
[
  {
    "namespace": "ordivon.host",
    "type": "assignment",
    "id": "assignment:fixture:1:g1",
    "generation": "1",
    "digest": "sha256:..."
  },
  {
    "namespace": "ordivon.host",
    "type": "harness_run",
    "id": "harness-run:codex:1",
    "digest": "sha256:..."
  },
  {
    "namespace": "ordivon.host",
    "type": "task",
    "id": "task:fixture",
    "generation": "7",
    "digest": "sha256:..."
  },
  {
    "namespace": "ordivon.host",
    "type": "task_attempt",
    "id": "task-attempt:fixture:1",
    "digest": "sha256:..."
  }
]
```

### Assertions

1. Runtime admits one Job and one Runtime Attempt.
2. Exact replay returns the original Job.
3. Exact replay creates no second physical dispatch.
4. Reusing `clientRequestId` with Assignment generation `2` fails as a request conflict.
5. Reusing `clientRequestId` with a changed Assignment digest also fails.
6. Terminal evidence contains all four exact references.
7. Terminal evidence binds Job ID, Runtime Attempt ID, Workspace, source revision, command, and Artifacts.
8. A fresh Host can find the Job through exact `clientRequestId`, inspect terminal evidence, and reconstruct the cross-layer map.
9. Runtime reports execution success while making no semantic Task-completion claim.

## Public terminology

Use qualified terms in new and touched public text:

```text
Host Task
Host Task Attempt
Runtime Job
Runtime Attempt
Harness Run
```

The current MCP Tool names remain:

```text
task.observe
task.list
task.cancel
```

They are compatibility names for Job control. Their descriptions and documentation already identify Job semantics. A rename or alias is admitted only after a live Host consumer demonstrates measurable ambiguity that qualified descriptions and foreign references fail to solve.

## Implemented Runtime validation

### R1 — deterministic conformance

The Core vector verifies exact replay, Assignment generation and digest conflicts, terminal-evidence retention, fresh Registry recovery, and the absence of Runtime semantic-completion claims.

### R2 — live Host receipt

The committed receipt and `scripts/check_host_h2_r2_receipt.py` independently verify the real Host request, all thirteen live assertions, exact cross-layer identities, and receipt integrity. Tamper tests reject changed references and failed live assertions even after integrity is recomputed.

### R3 — schema and catalog regression

Existing MCP tests retain the generic contract:

- execution requests expose bounded `foreignReferences`;
- references use the generic `ForeignReference` schema;
- unknown fields remain rejected;
- reference type values remain opaque strings;
- Runtime introduces no Host-specific enum.

## Host-owned changes

Host, not Runtime, implements:

- canonical ordering of Host references;
- Task, Task Attempt, Assignment, and Harness Run digests;
- new `clientRequestId` formation for each Assignment operation;
- semantic stale-generation validation;
- CompletionProposal adjudication;
- TaskOutcome commitment.

Runtime continues to execute the exact request and preserve its physical evidence.

## Changes deliberately avoided

- no Registry migration;
- no new Job or Attempt fields;
- no Host Task table;
- no Assignment table;
- no foreign-reference query index;
- no automatic Task lookup by Host identity;
- no `job.*` alias in Stage 2;
- no global trace service;
- no Harness lifecycle in Runtime;
- no semantic completion state.

## Optional projection gate

`TaskObservation` and `RuntimeJobSummary` currently omit foreign references. Stage 2 does not add them.

A projection becomes justified only when the live experiment shows that reading terminal evidence is a repeated material cost or prevents timely recovery. Until then:

- immediate recovery uses `clientRequestId` to find the Job;
- authoritative cross-layer binding is read from immutable Execution Plan or terminal evidence;
- Runtime avoids duplicating reference state into every observation.

## Acceptance result

Stage 2 passed every admission condition:

- deterministic and live conformance vectors passed;
- terminal evidence contained the exact Host references;
- changed Assignment generation and digest conflicted under the old request identity;
- a fresh Host client recovered the original Job without duplicate dispatch;
- public text distinguishes Host Task Attempt, Harness Run, Runtime Job, and Runtime Attempt;
- no Runtime schema migration, Tool rename, or production change was required.

## Reopen conditions

Reopen the Runtime boundary only if later live work proves one of these:

- foreign references cannot be recovered from retained Runtime evidence;
- the original Job cannot be located after Host restart;
- changed Assignment generation can be treated as replay;
- terminal evidence cannot bind the cross-layer identities;
- repeated recovery cost justifies one bounded projection.

Current evidence supports closing `ordivon-runtime#64` with the existing generic field retained and no broader Runtime abstraction.

## Implementation batches

### R1 — contract and test

Documentation, terminology audit, terminal-evidence conformance vector.

### R2 — Host integration verification

Completed with the committed live receipt and Runtime-side checker. The real Host request matched the R1 fixture without a Runtime production change.

### R3 — closeout

Completed with this disposition:

- retain opaque Runtime foreign references, exact request identity, Job lookup, and terminal evidence;
- localize reference construction, semantic digests, generation fencing, and completion decisions to Host;
- reject a duplicated observation projection, Host-specific schema enum, correlation service, and Tool rename;
- close Stage 2 without production Runtime expansion.
