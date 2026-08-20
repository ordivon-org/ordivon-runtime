---
schema_version: 1
id: runtime.foundations.rf0.sources
title: RF0 Sources — Runtime Phenomenon Space
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
summary: Primary and canonical sources used to triangulate RF0 across state-transition semantics, nondeterminism, event ordering, global-state observation, termination, transactions, concurrency and runtime lifecycle.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf0
---
# RF0 Sources — Runtime Phenomenon Space

RF0 prioritizes original papers, author-hosted publication records and canonical specifications. Sources are used as competing projections, not authorities for a single final ontology.

## S1 — Lamport: processes as views, not ontology

Leslie Lamport, **Processes are in the Eye of the Beholder**, Theoretical Computer Science / SRC Research Report 132, 1994/1997.

Primary author publication record: Microsoft Research.

RF0 use:

- demonstrates that the same system behavior can admit different process decompositions;
- supports treating process boundaries as modeling/implementation views rather than foundation primitives;
- motivates behavior/state-sequence reasoning below PID/process topology.

## S2 — Lamport: event ordering and partial causality

Leslie Lamport, **Time, Clocks, and the Ordering of Events in a Distributed System**, Communications of the ACM 21(7), 1978, pp. 558–565.

Primary author publication record: Microsoft Research.

RF0 use:

- `happened-before` as a partial order;
- wall-clock/log order must not be equated with causal order;
- state-machine realization can be distributed without one physically privileged total event sequence.

## S3 — Chandy & Lamport: global state and stable-property detection

K. Mani Chandy and Leslie Lamport, **Distributed Snapshots: Determining Global States of a Distributed System**, ACM Transactions on Computer Systems 3(1), 1985, pp. 63–75.

Primary author publication record: Microsoft Research; institutional copy also available through CaltechAUTHORS.

RF0 use:

- local observations do not automatically form one coherent global instant;
- termination and deadlock are examples of stable/global property detection problems;
- observation protocol is part of knowing system state, not merely a UI concern.

## S4 — Dijkstra & Scholten: termination is relational/global

Edsger W. Dijkstra and C. S. Scholten, **Termination detection for diffusing computations**, EWD687 / Information Processing Letters 11(1), 1980.

Primary archive: E. W. Dijkstra Archive, University of Texas at Austin.

RF0 use:

- local inactivity is insufficient to establish global termination;
- activity can remain represented by messages/engagement relations after one node is idle;
- completion needs a declared termination criterion and evidence mechanism.

## S5 — Dijkstra: nondeterministic program semantics

Edsger W. Dijkstra, **Guarded commands, non-determinacy and formal derivation of programs**, Communications of the ACM 18(8), 1975, pp. 453–457; EWD472.

Primary archive: E. W. Dijkstra Archive, University of Texas at Austin.

RF0 use:

- valid computation need not be deterministic;
- one initial state need not imply one unique activity or one unique final state;
- correctness can constrain a set of permissible outcomes without requiring deterministic reproduction.

## S6 — Harel: complex behavior needs hierarchy, concurrency and communication

David Harel, **Statecharts: A Visual Formalism for Complex Systems**, Science of Computer Programming 8(3), 1987, pp. 231–274. DOI: 10.1016/0167-6423(87)90035-9.

Primary publication: Elsevier / ScienceDirect; institutional record: Weizmann Institute of Science.

RF0 use:

- flat state-transition diagrams are often an inadequate behavior representation;
- hierarchy, concurrency and communication are first-class complications;
- state machines are modeling formalisms rather than proof that one flat state enum is the system ontology.

## S7 — Gray & Reuter: transaction processing

Jim Gray and Andreas Reuter, **Transaction Processing: Concepts and Techniques**, Morgan Kaufmann, 1991/1992.

Primary publication record: Microsoft Research.

RF0 use:

- transaction as a mature framework for reliable updates under failures and concurrency;
- commit/abort/recovery as distinct from raw process execution;
- useful comparison, but not a universal model for open-world effects.

## S8 — Gray & Lamport: distributed commit is an agreement problem

Jim Gray and Leslie Lamport, **Consensus on Transaction Commit**, MSR-TR-2003-96 (2004), ACM Transactions on Database Systems 31(1), 2006, pp. 133–160.

Primary publication record: Microsoft Research.

RF0 use:

- commitment in distributed systems is not reducible to one local process exit;
- Two-Phase Commit and Paxos Commit expose failure/blocking/agreement dimensions;
- strengthens the distinction between execution evidence and effect/commit finality.

## S9 — Herlihy & Wing: operation invocation/response versus abstract effect point

Maurice P. Herlihy and Jeannette M. Wing, **Linearizability: A Correctness Condition for Concurrent Objects**, ACM TOPLAS 12(3), 1990, pp. 463–492. DOI: 10.1145/78969.78972.

RF0 use:

- invocation and response bound an interval within which an operation may be abstractly treated as taking effect;
- shows that an operation's reasoning-level commit/effect point is distinct from API call boundaries;
- retained only for closed concurrent-object semantics, not generalized to arbitrary external effects.

## S10 — OCI Runtime Specification: lifecycle realization

Open Container Initiative, **Runtime Specification — Runtime and Lifecycle**, current canonical specification.

Canonical source: `opencontainers/runtime-spec`.

RF0 use:

- explicit identity, state and lifecycle operations;
- separation of `create` from `start` and cleanup/delete;
- configuration becomes effective/frozen at specific realization boundaries;
- demonstrates a mature `runtime = environment realization + lifecycle control` tradition that is still narrower than open-world effect reconciliation.

## S11 — Google MapReduce: runtime as hidden distribution/failure machinery

Jeffrey Dean and Sanjay Ghemawat, **MapReduce: Simplified Data Processing on Large Clusters**, OSDI 2004.

Primary publication record: Google Research.

RF0 use:

- runtime can own scheduling, partitioning, communication and machine-failure handling while the user supplies a higher-level computation;
- demonstrates that scheduler/resource/failure management are common runtime functions but need not define Runtime's ontology.

## S12 — AWS Builders' Library: retries, idempotency and ambiguity

Malcolm Featonby, **Making retries safe with idempotent APIs**, Amazon Builders' Library.

Canonical source: AWS.

RF0 use:

- network timeout can leave occurrence of a mutating action uncertain;
- stable request identity/idempotency can make retries safe only when the effect owner supports that contract;
- practical external confirmation of `response loss != effect absence` and `recovery != blind retry`.

## S13 — Abadi & Lamport: environment assumptions and guarantees

Martín Abadi and Leslie Lamport, **Composing Specifications**, ACM TOPLAS 15(1), 1993, pp. 73–132.

Primary author publication record: Microsoft Research.

RF0 use:

- system guarantees are meaningful only relative to environmental assumptions;
- supports treating authority, dependency and external environment as declared coordinates rather than invisible ambient facts.

## Evidence rule for later rounds

RF0 sources should not be converted into a citation-count vote. Later rounds must continue to:

```text
identify the exact claim a source supports
separate formal model from physical ontology
retain source-specific scope
seek counterexamples outside software
prefer falsification over analogy
```

The current Ordivon Runtime itself remains a major empirical source: 55k+ accumulated Jobs, Linux/Windows pressure tests, provider/dependency drift falsifiers, response-loss recovery and structured-effect experiments are implementation evidence to compare against these external traditions.
