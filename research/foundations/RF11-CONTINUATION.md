---
schema_version: 1
id: runtime.foundations.rf11.continuation
title: RF11 Continuation — Enter RF12
type: continuation
profile: research
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - researcher
  - agent
updated: 2026-08-17
summary: Cross-conversation checkpoint after RF11, preserving Runtime epistemology and the exact RF12 frontier on Composition, Transactions and Cross-Operation Atomicity.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf11
  - runtime.foundations.rf11.sources
---
# RF11 Continuation — Enter RF12

## Completed

RF11 — Observation, Evidence, Proof and Truth Surfaces.

Primary record:

`research/foundations/RF11-OBSERVATION-EVIDENCE-PROOF-TRUTH-SURFACES.md`

Source map:

`research/foundations/RF11-SOURCES.md`

## Durable results to carry forward

1. `Reality != Fact != Claim != Observation != Record != Evidence != Proof != Projection`.
2. Observation is observer/method/scope/time-relative; a correct observation can be
   insufficient for a broader claim.
3. Evidence is claim-relative and only supports a claim under a declared contract/model.
4. Proof is the inference from evidence+premises to a claim; evidence alone is not proof.
5. “Truth owner” is refined to the component/domain with canonical commit/reconstruction
   authority for a **named fact class under a scope/contract**, not owner of Reality itself.
6. Current Runtime remains correctly federated across Git/filesystem, SQLite Registry,
   systemd/cgroup, Windows Job/launcher, Runner evidence, structured Effect receipts and
   Host/domain truth owners.
7. Provenance, authenticity, integrity, correctness, completeness and freshness are separate
   evidence dimensions.
8. SHA digests establish byte identity/change relationships but not provenance, authority or
   semantic validity.
9. Digital signatures add origin/integrity evidence under key/trust assumptions but do not
   make signed content semantically true.
10. Timestamps/generations/row versions prove only the time/order/freshness relations their
    owner contracts define; they do not provide universal causality/global time.
11. Identity binding (Job/Attempt/operation/invocation/process/source/provider/effect IDs) is
    central to evidence strength.
12. Direct/domain evidence differs from derived projections, though direct evidence still
    requires identity/integrity/contract validation.
13. Correctness and completeness are orthogonal; a bounded list can be entirely correct but
    incomplete.
14. Current truncation/cursor metadata is therefore epistemically significant and must block
    closed-set inference when incomplete.
15. SQLite read snapshots provide Registry-local consistency only; Git/filesystem/process/
    external receipts remain separate observations.
16. Cross-owner “global state” requires an explicit snapshot/reconciliation protocol rather
    than arbitrary multi-source reads.
17. Freshness is claim-relative; stale evidence may remain correct for historical claims.
18. Evidence precedence is claim-specific; newer evidence is not automatically stronger.
19. `supersedesArtifactId` correctly models stronger later evidence revising the current
    conclusion without deleting prior evidence history.
20. Corroboration requires provenance/failure-domain independence; multiple views derived
    from one source are not independent evidence.
21. `NoObservedX != ProvenNoX` unless observer coverage/timing/substrate contracts guarantee
    X would have been observed.
22. Windows `KILL_ON_JOB_CLOSE` yields stronger negative process evidence than generic Linux
    unit/PID absence because the substrate contract closes more alternatives.
23. P4 permanently falsifies host-path/no-inotify evidence as proof of target-byte
    consumption; current scope `runtime_host_namespace_path_witness` is correct.
24. A Receipt is authoritative only for transitions/effects its issuer/domain actually owns;
    Release receipt can outrank process status for deployment Effect but not for unrelated
    claims.
25. Artifact existence/digest/truncation proves retained bytes and completeness limits, not
    semantic correctness.
26. Generic scalar confidence is not justified for deterministic mechanical Runtime facts
    without a calibrated probabilistic model.
27. Preferred epistemic architecture: federated scoped authority + identity-bound evidence
    + explicit provenance/integrity/completeness/freshness boundaries + claim-directed
    reconciliation, not one synthetic global truth database.

## RF11 compact grammar

```text
R                               Reality
F(R,B,t)                        true scoped Fact
Q                               asserted/queried Claim
O(observer,target,method,S,t)   Observation
Rec(...)                        durable/transient Record
E(Q,C)                          Evidence relevant to Q under Contract C
Proof_C(E => Q)                 valid inference under C
Owner(F,C)                      canonical fact commit/reconstruction authority
Prov(E)                         provenance/derivation of evidence
Int(E)                          integrity property
Complete(E,D)                   coverage completeness for domain D
Fresh(E,Q,t)                    freshness relative to Claim Q
π_Q(S)                          question-relative Projection
Receipt(owner,effect,outcome)   effect-owner evidence
E2 >_Q E1                      stronger evidence supersedes prior conclusion for Q
```

## Current implementation interpretation

```text
Git/filesystem                    source/worktree fact domain
sourceStateDigest                 scoped byte/state identity evidence
SQLite Registry                   Runtime commitment/state authority
row_version                       Registry-local stale-write/CAS guard
systemd/cgroup                    Linux process ownership observation domain
Windows Job/launcher              native process ownership/termination domain
Runner start/result               local realization/terminal execution evidence
terminal-evidence Artifact        durable identity-rich synthesized evidence
supersedesArtifactId              evidence correction lineage
Artifact digest/truncated         byte identity + completeness boundary
Release receipt                   deployment Effect-domain evidence
Patch before/after digests        Workspace Patch reconciliation evidence
task.get/task.list                passive bounded Registry projections
task.observe                      active observation/reconciliation surface
inspection SQLite snapshot        Registry-local coherent read snapshot
Git/filesystem inspection         separate physical observation
truncation flags/cursors          completeness evidence
Doctor fingerprint/row versions   repair freshness/identity guards
trace JSONL                       bounded diagnostics, not canonical execution truth
Host/domain outcome               semantic authority outside Runtime
```

## RF12 exact frontier

Enter:

# RF12 — Composition, Transactions and Cross-Operation Atomicity

Core questions:

```text
What does it mean for multiple Operations to compose?
When is a sequence one compound Operation versus several independent Operations?
What is atomicity and relative to which state/effect owner?
What do commit, abort and prepare mean across one owner versus multiple owners?
What is a local database transaction versus distributed transaction versus Saga?
Can Runtime provide atomicity across SQLite admission and systemd/process launch?
Can Runtime provide atomicity across process execution and an external Effect?
What is linearizability/serializability relative to composition?
When can two structured Effects participate in one transaction?
What is all-or-nothing versus exactly-once versus compensation?
What is a commit point when several effect owners exist?
How should partial commit be represented and reconciled?
When is orchestration sequencing enough without transaction semantics?
```

Mandatory falsifiers:

```text
one SQLite transaction over Job+Attempt+reservation
SQLite commit then Runtime crashes before systemd dispatch
systemd launch then external API mutation
Workspace Patch touching multiple files under one local adapter contract
release deploy then rollback
financial order + local ledger update without shared transaction owner
two independent external APIs with no distributed transaction support
classic two-phase commit coordinator crash
Paxos Commit / replicated commit decision
Saga with compensating actions
outbox pattern: local atomic intent + non-atomic external delivery
multi-step workspace.execPlan with one process execution but multiple external effects
atomic rename versus multi-file semantic transaction
```

Do not enter RF13 Isolation, Trust and Containment until RF12 separates local atomicity,
distributed commit, orchestration, compensation and open-world partial effects well enough
that containment is not mistakenly treated as transactionality.
