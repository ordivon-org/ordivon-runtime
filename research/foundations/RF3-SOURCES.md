---
schema_version: 1
id: runtime.foundations.rf3.sources
title: RF3 Sources — Execution, Realization, Effect and Consequence
type: reference
profile: research
lifecycle: completed
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - researcher
  - builder
  - agent
updated: 2026-08-17
summary: Primary and canonical sources used by RF3 to separate execution envelopes, realization, provider acceptance, external effect, receipt, atomicity, durability, idempotency and wider consequence.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf3
---
# RF3 Sources — Execution, Realization, Effect and Consequence

RF3 triangulates operating-system execution, container lifecycle, HTTP/API semantics,
transactions, concurrency objects, filesystem atomicity/durability and end-to-end system
design. No source family is treated as a complete ontology of physical realization.

## S1 — OCI Runtime Specification: create versus start

Open Container Initiative, **Runtime Specification**, runtime lifecycle section.

Canonical source: Open Container Initiative runtime-spec repository.

RF3 use:

- `create` establishes the runtime environment/resources while the user-specified program
  has not yet run;
- `start` later transitions the container into running user code;
- supports `EnvironmentRealization != TargetExecution` and the existence of multiple
  start boundaries inside one higher operation.

## S2 — POSIX exec family: process-image replacement

The Open Group / IEEE, **POSIX.1-2024 exec family — execute a file**.

Canonical source: The Open Group Base Specifications Issue 8 / IEEE Std 1003.1-2024.

RF3 use:

- successful `exec` replaces the process image and does not return to the old image;
- gives one strong process-image execution boundary without implying application/domain
  effect success;
- supports `SuccessfulExec != SuccessfulOperationEffect`.

## S3 — Structural operational semantics

Gordon D. Plotkin, **A Structural Approach to Operational Semantics**, DAIMI FN-19,
Aarhus University, 1981; reprinted in Journal of Logic and Algebraic Programming, 2004.

RF3 use:

- program execution can be modeled through rule-governed transitions between
  configurations rather than by identifying execution with one OS process;
- retained as programming-language ancestry for execution behavior, not as a complete
  open-system Runtime model.

## S4 — RFC 9110: HTTP request acceptance and fulfillment

R. Fielding, M. Nottingham and J. Reschke (eds.), **RFC 9110 — HTTP Semantics**, IETF,
2022.

Canonical source: RFC Editor.

RF3 use:

- `202 Accepted` explicitly separates acceptance from completed processing and allows the
  accepted request ultimately not to be acted upon;
- `201 Created` and `204 No Content` demonstrate protocol-specific stronger fulfillment /
  applied semantics;
- supports `ProviderAcceptance != EffectCompletion` and the principle that fulfillment
  semantics belong to the protocol/effect owner rather than local transport or process
  status.

## S5 — AWS Builders' Library: idempotent API request identity and response-loss ambiguity

Amazon Web Services, **Making retries safe with idempotent APIs**, AWS Builders' Library.

Canonical source: AWS Builders' Library.

RF3 use:

- network timeout can leave the client unable to know whether a resource/effect was
  created;
- blind retry can duplicate effects without provider-supported idempotency semantics;
- stable client request identity must be interpreted by the service that owns the
  resource mutation;
- supports `TransportFailure != ProviderRejection`, `ResponseLoss != EffectAbsence`, and
  effect-owner identity/reconciliation.

## S6 — Gray & Lamport: transaction commit is a protocol decision

Jim Gray and Leslie Lamport, **Consensus on Transaction Commit**, ACM Transactions on
Database Systems 31(1), 2006.

Canonical author publication record: Microsoft Research.

RF3 use:

- distributed transaction commit is a protocol deciding commit/abort among participants;
- supports `TransactionCommit != CoordinatorProcessExit` and distinguishes domain
  finality from one process lifecycle.

## S7 — Herlihy & Wing: linearizability

Maurice Herlihy and Jeannette M. Wing, **Linearizability: A Correctness Condition for
Concurrent Objects**, ACM TOPLAS 12(3), 1990.

Canonical author publication record: Brown University / Carnegie Mellon publication
archives.

RF3 use:

- provides the canonical abstraction in which a concurrent operation can be reasoned
  about as taking effect atomically between invocation and response;
- RF3 uses this only as an abstraction lesson: an abstract effect point does not imply
  one physically indivisible implementation instruction.

## S8 — POSIX rename and directory-operation durability

The Open Group / IEEE, POSIX.1-2024 **rename()** and filesystem/directory-operation
requirements and rationale.

Canonical source: The Open Group Base Specifications Issue 8 / IEEE Std 1003.1-2024.

RF3 use:

- rename/replacement gives a strong namespace-level atomic/serializable effect contract;
- POSIX separately distinguishes that such directory operations are not automatically
  durable across system failure without required synchronization;
- supports `AtomicVisibility != Durability` and the need to name the effect projection
  and failure model.

## S9 — Saltzer, Reed & Clark: end-to-end arguments

Jerome H. Saltzer, David P. Reed and David D. Clark, **End-to-End Arguments in System
Design**, ACM TOCS 2(4), 1984, plus author-hosted retrospective/commentary.

RF3 use:

- functions requiring application-specific knowledge can be incompletely implemented by
  lower layers that lack that knowledge;
- used narrowly as a system-design heuristic for leaving domain-effect semantics and
  authoritative receipts with capable endpoints rather than inventing weaker Runtime
  approximations.

## S10 — SQLite transactions as a local commitment/atomicity reference

SQLite, **Transaction** and atomic commit documentation.

Canonical source: sqlite.org.

RF3 use:

- local durable transaction semantics distinguish transaction commit/rollback from
  surrounding process lifetime;
- used as local implementation ancestry, not as a universal model of external effects.

## Local empirical source — current Ordivon Runtime

RF3 audits current source and tests at the research line.

### Dispatch ordering

`Registry::mark_dispatch_issued`:

```text
Accepted + committed bundle
→ persist Attempt Starting
→ persist DISPATCH_ISSUED / AT_MOST_ONCE_BOUNDARY_COMMITTED
→ commit Registry transaction
```

Only afterward `dispatch_attempt` invokes `systemd_run` or `windows_systemd_run`.
Runner/start identity is separately observed and bound later.

RF3 therefore interprets current `DISPATCH_ISSUED` as an at-most-once dispatch
commitment/issue boundary, not process-start proof.

### Workspace Patch

`workspace_patch_operations` stores:

```text
prepared
committed
unknown
```

before/after direct filesystem mutation. Tests demonstrate both:

- physically committed after-state can be rediscovered after Patch receipt loss and the
  operation converges to `committed` without replaying the mutation;
- mixed physical state converges to `unknown` and same-request replay is rejected with
  reconciliation required.

This is RF3's decisive counterexample to `StructuredEffect = Job/Runner execution`.

### Runtime self-release

Canonical Runtime documentation and tests treat the deterministic deployment receipt as
stronger evidence of the release Effect than generic Job/process status during Runtime
self-replacement.

### Generic execution evidence

Terminal evidence separately exposes execution disposition, delivery certainty and
process-tree disposition while refusing semantic-completion claims. Arbitrary execution
remains external-effect opaque.

## Source discipline

Later rounds should preserve:

```text
OS process boundary != universal execution boundary
container environment realization != target execution
transport/request acceptance != domain effect completion
abstract atomicity != physical indivisibility
effect visibility != durability/finality
receipt authority is scoped to its issuer/contract
domain-specific effect truth should not be synthesized from weaker generic process data
local Runtime mechanisms remain specializations, not the ontology of realization
```
