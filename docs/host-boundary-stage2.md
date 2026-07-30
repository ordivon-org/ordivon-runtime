# Host Boundary Stage 2

Status: R1 contract and conformance vector implemented; R2 live Host integration remains
Canonical experiment: `ordivon-computing/research/experiments/harness-boundary-v0/`

## R1 result

The frozen vector passes without Runtime production changes. Four opaque `ordivon.host` references bind one Host Task, Host Task Attempt, Assignment generation, and Harness Run to one Runtime Job and Runtime Attempt. Exact replay returns the original Job and creates no second Attempt; changing the Assignment generation or digest under the same `clientRequestId` produces `IdempotencyConflict`; terminal evidence preserves the exact references plus Job, Runtime Attempt, Workspace, source revision, and executable; and a fresh Registry instance locates the same Job and evidence through the exact request identity.

The vector also confirms the negative boundary: terminal evidence records `executionDisposition=succeeded` but contains no semantic-completion or TaskOutcome claim. No Registry migration, Job or Attempt field, Host-specific schema enum, observation projection, Tool rename, foreign-reference index, service, or production Runtime branch was required. R2 is therefore limited to consuming one real Host H2 request and comparing it with this fixture.

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
| `task` | Host | current Task revision | current Task event/projection digest |
| `task_attempt` | Host | omitted | immutable Task-Attempt descriptor digest |
| `assignment` | Host | Assignment generation | immutable Assignment digest |
| `harness_run` | Harness adapter, admitted by Host | omitted | run-intent or Run receipt digest |
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

## Required Runtime changes

### R1 — documentation

Add this contract to the public Runtime model and link it from the README and Agent UX documentation.

Clarify in touched text:

- `foreignReferences` are correlation inputs, not foreign state ownership;
- Job success is physical execution evidence;
- Host Assignment generation performs semantic stale-worker fencing;
- Runtime request identity prevents changed generations from masquerading as replay;
- Runtime never validates semantic completion.

### R2 — conformance test

Add the vector above to `transactional_runtime.rs` or the closest existing foreign-reference integration test.

The test must inspect the retained terminal-evidence Artifact rather than only the submitted request.

### R3 — schema and catalog regression

Retain existing MCP schema assertions that `foreignReferences`:

- are present on execution requests;
- use `ForeignReference` schema;
- reject unknown fields;
- remain bounded.

Add no Host-specific enum to Runtime JSON Schema. Reference type values remain opaque strings.

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

## Acceptance

Stage 2 is complete when:

- the conformance vector passes;
- terminal evidence contains exact Host references;
- changed Assignment generation conflicts under one request identity;
- fresh Host recovery locates the original Job without duplicate dispatch;
- public text consistently distinguishes Host Task Attempt from Runtime Attempt;
- no Runtime schema migration or Tool rename is required.

## Falsifiers

Expand Runtime only if the live vector proves one of these:

- foreign references are not recoverable from retained Runtime evidence;
- the original Job cannot be located after Host restart;
- changed Assignment generation can be silently treated as replay;
- terminal evidence cannot bind the cross-layer identities;
- reading the evidence introduces repeated material recovery cost that a bounded projection would remove.

Absent such evidence, close `ordivon-runtime#64` with the existing field retained and no broader Runtime abstraction.

## Implementation batches

### R1 — contract and test

Documentation, terminology audit, terminal-evidence conformance vector.

### R2 — Host integration verification

Consume one real request emitted by the Host Harness experiment and compare it with the Runtime fixture.

### R3 — closeout

Record costs and decide:

- retain the convention;
- localize additional helpers to Host;
- add one bounded projection if proven necessary;
- otherwise close without production Runtime changes.
