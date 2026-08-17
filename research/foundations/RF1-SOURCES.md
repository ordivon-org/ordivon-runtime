---
schema_version: 1
id: runtime.foundations.rf1.sources
title: RF1 Sources — State, Change, Event, Behavior and Dynamics
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
summary: Primary and canonical sources used by RF1 to triangulate state, transition relations, behavior, stuttering/refinement, observability, nondeterminism, continuous and hybrid dynamics, event ordering and environment assumptions.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf1
---
# RF1 Sources — State, Change, Event, Behavior and Dynamics

RF1 deliberately compares formal-method, control/dynamical-system and hybrid/cyber-physical traditions. No source family is treated as the ontology of Reality.

## S1 — Åström & Murray: state as history summary for future prediction

Karl J. Åström and Richard M. Murray, **Feedback Systems: An Introduction for Scientists and Engineers**, state-space/system-modeling chapters, Caltech Control and Dynamical Systems, online author edition.

Canonical author site: Caltech FBS.

RF1 use:

- state variables summarize past history for predicting future evolution;
- model choice and state choice depend on the questions and fidelity;
- differential equations and difference equations provide continuous/discrete state-space models;
- state is not necessarily directly measured.

This is RF1's strongest external anchor for the `future-relevant history compression` interpretation.

## S2 — Åström & Murray: observability and state estimation

Same work, **Output Feedback / Observability** material.

RF1 use:

- the state of a model can differ from available sensor outputs;
- observers reconstruct/estimate state from input-output history when the system is observable;
- supports `state != observation != estimate`.

## S3 — Lamport: Temporal Logic of Actions

Leslie Lamport, **The Temporal Logic of Actions**, ACM TOPLAS 16, 1994.

Canonical author publication record: Microsoft Research.

RF1 use:

- actions as relations between old and new state values;
- state-transition relation can be specified directly rather than through programming-language control structures;
- useful formal model for discrete concurrent systems without treating code/process structure as ontology.

## S4 — Lamport: behavior, steps and stuttering

Leslie Lamport, TLA explanatory material on **behavior**, **step**, **stuttering steps**, and refinement.

Canonical source: Lamport's TLA site.

RF1 use:

- a step is a pair of states;
- a behavior is represented as a sequence of states in that formalism;
- stuttering leaves relevant variables unchanged while changes elsewhere can occur;
- implementation/refinement must tolerate different lower-level step granularities.

RF1 generalizes the anti-collapse but does not claim all Reality is an infinite discrete state sequence.

## S5 — Abadi & Lamport: refinement mappings

Martín Abadi and Leslie Lamport, **The Existence of Refinement Mappings**, LICS 1988.

Canonical author publication record: Microsoft Research.

RF1 use:

- lower-level state spaces can map to higher-level state spaces;
- lower-level steps and behaviors can implement higher-level steps and behaviors;
- supports the claim that state/transition granularity is abstraction-relative and one macro-step may hide many microsteps.

## S6 — Lamport: reduction / pretending atomicity

Leslie Lamport and Fred B. Schneider, **Pretending Atomicity**, SRC Research Report 44, 1989; related Lamport reduction work.

Canonical author publication record: Microsoft Research.

RF1 use:

- larger atomic actions can be reasoning abstractions over finer concurrent actions when reduction conditions hold;
- supports `atomic transition != physically indivisible instant`.

## S7 — Dijkstra: guarded commands and nondeterministic state evolution

Edsger W. Dijkstra, **Guarded commands, non-determinacy and formal derivation of programs**, EWD472 / Communications of the ACM 18(8), 1975.

Primary archive: E. W. Dijkstra Archive, University of Texas at Austin.

RF1 use:

- predicates over a state space;
- one initial state can permit several activities and even several permissible final states;
- falsifies `state + valid program => unique next/final state` as a universal law.

## S8 — Abadi & Lamport: environment assumptions and system guarantees

Martín Abadi and Leslie Lamport, **Composing Specifications**, ACM TOPLAS 15(1), 1993.

Canonical author publication record: Microsoft Research.

RF1 use:

- a system guarantee is meaningful relative to assumptions about environment behavior;
- supports making boundary/input/environment explicit rather than treating omitted world facts as nonexistent.

## S9 — Chandy & Lamport: global state observation

K. Mani Chandy and Leslie Lamport, **Distributed Snapshots: Determining Global States of a Distributed System**, ACM TOCS 3(1), 1985.

Canonical author publication record: Microsoft Research; institutional archive at Caltech.

RF1 use:

- independently observed local states do not automatically constitute one coherent global snapshot;
- observing a global/stable property requires an explicit observation protocol/model;
- reinforces `state != observation` and `local projections != atomic global Reality`.

## S10 — Henzinger: hybrid automata

Thomas A. Henzinger, **The Theory of Hybrid Automata**, IEEE LICS 1996; related Berkeley hybrid-system technical reports.

Canonical institutional record: UC Berkeley EECS.

RF1 use:

- hybrid models combine discrete modes/transitions with continuous state evolution;
- falsifies the choice between `everything is a discrete state machine` and `everything is only continuous flow`.

## S11 — Alur et al.: algorithmic analysis of hybrid systems

Rajeev Alur, Costas Courcoubetis, Nicolas Halbwachs, Thomas A. Henzinger, Pei-Hsin Ho, Xavier Nicollin, Alfredo Olivero, Joseph Sifakis and Sergio Yovine, **The Algorithmic Analysis of Hybrid Systems**, Theoretical Computer Science 138, 1995.

Canonical references preserved in Berkeley hybrid-systems publication records.

RF1 use:

- continuous variables and discrete control/mode structure can share one formal model;
- supports behavior as a mixed flow/jump trajectory rather than only an event list.

## S12 — Edward A. Lee: operational semantics of hybrid systems

Edward A. Lee and related Berkeley work, **Operational Semantics of Hybrid Systems**, UC Berkeley Technical Report EECS-2007-68.

Canonical institutional record: UC Berkeley EECS.

RF1 use:

- continuous-time and discrete-event subsystems interact;
- discontinuities and simultaneous discrete events require precise execution semantics;
- superdense time can represent multiple ordered discrete occurrences at one real-time coordinate;
- strongly falsifies `timestamp == event identity/order`.

## S13 — Edward A. Lee: constructive discrete/continuous physical models

Edward A. Lee, **Constructive Models of Discrete and Continuous Physical Phenomena**, UC Berkeley Technical Report EECS-2014-15.

Canonical institutional record: UC Berkeley EECS.

RF1 use:

- discrete and continuous physical phenomena can require richer semantic treatment than ordinary timestamped event sequences;
- constructive semantics can expose uncertainty/nonconstructive behavior rather than hiding it.

## S14 — Lee & Seshia: cyber-physical systems

Edward A. Lee and Sanjit A. Seshia, **Introduction to Embedded Systems: A Cyber-Physical Systems Approach**, author-hosted Berkeley edition; and Edward A. Lee, **Cyber Physical Systems: Design Challenges**, EECS-2008-8.

Canonical institutional source: UC Berkeley / Ptolemy Project.

RF1 use:

- computation, networking and physical processes form coupled feedback systems;
- physical dynamics and software/discrete dynamics must be modeled together when the question crosses the boundary.

## S15 — Henzinger & Majumdar: symbolic transition systems / state equivalence

Thomas A. Henzinger and Rupak Majumdar, **A Classification of Symbolic Transition Systems**, UCB/CSD-99-1086, 1999.

Canonical institutional record: UC Berkeley EECS.

RF1 use:

- bisimilarity and observational/reachability quotients group states according to preserved future behavior/properties;
- supports RF1's derived interpretation of state abstractions as equivalence classes of histories/configurations relative to future-relevant distinctions.

## S16 — Lamport: safety/liveness and behavior properties

Leslie Lamport and Susan Owicki, **Proving Liveness Properties of Concurrent Programs**, ACM TOPLAS 4(3), 1982; related Lamport safety/liveness work.

Canonical author publication record: Microsoft Research.

RF1 use:

- important correctness properties concern whole behaviors/temporal evolution rather than one current state;
- supports `state predicate != behavior property` and prevents terminal/current-state totalization.

## Local empirical source — current Ordivon Runtime

RF1 also audits the current source at the RF0/RF1 research line:

- `AttemptState` combines lifecycle phase, recovery activity, terminal outcome and uncertainty classifications in one operational enum;
- `JobProjection` separately exposes desired state, Attempt state, termination intent, execution terminality/disposition, delivery disposition and recovery requirement;
- `job_events` maintains a Runtime-owned ordered operational history.

RF1 treats these as empirical engineering choices to explain, not as external proof of the foundation model.

## Source discipline

Later rounds should preserve these rules:

```text
formal state != physical ontology by default
control state != directly observable state by default
discrete-event semantics != universal Reality event atoms
continuous dynamics != proof that discrete abstractions are invalid
hybrid models show coexistence rather than one universal mathematical representation
implementation state machine remains a falsifier/consumer, not foundations authority
```
