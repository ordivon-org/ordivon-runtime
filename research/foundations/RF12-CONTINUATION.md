---
schema_version: 1
id: runtime.foundations.rf12.continuation
title: RF12 Continuation — Enter RF13
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
summary: Cross-conversation checkpoint after RF12, preserving composition/transaction/atomicity distinctions and the exact RF13 frontier on Isolation, Containment and Trust.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf12
  - runtime.foundations.rf12.sources
---
# RF12 Continuation — Enter RF13

## Completed

RF12 — Composition, Transactions and Cross-Operation Atomicity.

Primary record:

`research/foundations/RF12-COMPOSITION-TRANSACTIONS-CROSS-OPERATION-ATOMICITY.md`

Source map:

`research/foundations/RF12-SOURCES.md`

## Durable results to carry forward

1. Composition, sequence, Compound Operation and Transaction are distinct.
2. A Transaction requires a named owner/domain and commit/abort contract over a declared
   participating state/effect set.
3. Atomicity is all-or-none commit under that contract and is distinct from durability,
   isolation, consistency and semantic success.
4. `LocalAtomicity != CrossOwnerAtomicity`; atomicity scope cannot exceed actual transaction
   participant scope.
5. Current Runtime admission is a genuine Registry-local SQLite transaction binding Job,
   Attempt, reservation, idempotency and relevant Runtime-owned side truth.
6. SQLite admission COMMIT is the durable Runtime admission commit point.
7. Physical dispatch is deliberately outside admission transaction scope; a committed
   Accepted Job before launch is a valid durable-handoff state.
8. Admission→dispatch is a durable handoff, not a distributed transaction.
9. `dispatch_issued` is a Registry-local at-most-once handoff frontier, not atomic process
   creation with systemd/Windows.
10. One process execution can commit many independent/open-world external Effects; process
    cardinality is not transaction cardinality.
11. Current `workspace.execPlan` is ordered orchestration with per-step progress/failure and
    explicit `continueOnError`, not transactionality.
12. Stop-on-error does not roll back prior step Effects; compound execution partiality must
    remain visible.
13. Distributed transactions require actual resource-manager participation in a common
    prepare/commit/abort/recovery protocol.
14. Runtime preflight/precondition validation is not a 2PC prepare vote.
15. Classic 2PC shows transaction atomicity safety can block under coordinator failure;
    atomicity does not imply nonblocking liveness.
16. Paxos Commit can strengthen commit-decision fault tolerance but cannot make arbitrary
    external APIs transactional participants.
17. Saga means independently committed subtransactions plus compensation policy, not one long
    ACID transaction.
18. Compensation is a new Effect, may be imperfect/non-invertible and can itself fail.
19. Orchestration is the truthful default for independent open-world systems without a
    shared commit protocol.
20. Transactional-outbox thinking can atomically bind local state + durable intent, while
    downstream delivery remains outside that local transaction and may duplicate.
21. `commitState` is scoped to Runtime operation commitment, not external Effect commitment.
22. Current terminal commit and admin repair batch are true Registry-local atomic
    transactions; they do not roll back external Reality.
23. `workspace.mutate` can own a local atomic mutation without durable replay identity;
    response-loss safety and atomicity are distinct.
24. Workspace Patch owns a structured correlated multi-file effect with before/after plan and
    reconciliation, but mixed physical state can be `unknown`; it is not a universal
    filesystem transaction.
25. Individually atomic primitives such as rename do not compose automatically into a
    compound atomic effect.
26. Runtime Release `not_committed` and `rolled_back` are distinct histories; domain rollback
    does not erase Operation history.
27. Exchange order + local ledger illustrates the dual-write problem: sequencing and even
    idempotency/receipts cannot become literal cross-owner ACID without shared participation.
28. Shared Job ID/clientRequestId/coordinator state provides correlation/identity, not atomic
    grouping.
29. Current responsibility boundary remains sound: Runtime exposes execution/effect evidence;
    Host/domain owns semantic multi-Operation orchestration/Saga policy until consumers prove
    otherwise.
30. Governing theorem: `AtomicityScope <= TransactionParticipantScope`.

## RF12 compact grammar

```text
Compose(A1...An,R)                 composition
Op{A1...An}                        compound logical Operation
Txn(owner,participants,stateSet)   transaction
Atomic_C(S)                        all-or-none commit under C
Commit_O(T)                        owner commit point
Abort_O(T)                         discard provisional transaction state
Prepare_P(T)                       durable participant promise for final decision
Handoff(O1→O2,intent)              local durable intent then later realization
Partial({E1...En})                 some effect owners committed, others did not/unknown
Saga(T1...Tn,C1...Cn)              independent commits + compensation policy
Orch(A1...An,control)              ordered/conditional coordination without atomicity
Comp(E)                            new offsetting domain Effect
```

## Current implementation interpretation

```text
SQLite admission transaction          true local transaction
Job+Attempt+reservation+idempotency    one Registry commitment set
Accepted after commit                  durable handoff state
dispatch_issued                        at-most-once handoff fence
systemd/Windows launch                 separate physical owner
workspace.exec / target                open-world OPAQUE Effects
workspace.execPlan                     ordered orchestration
stop/continue-on-error                 control flow, not rollback
terminal commit                        local Registry transaction
admin repair batch                     local atomic Registry batch
workspace.mutate                       local atomic mutation, weaker replay safety
Workspace Patch                        structured correlated effect + reconciliation
Runtime Release                        structured deploy/recovery orchestration
not_committed / rolled_back            distinct release histories
commitState                            Runtime commitment evidence only
Host                                   semantic multi-Operation composition owner
```

## RF13 exact frontier

Enter:

# RF13 — Isolation, Containment and Trust

Core questions:

```text
What is isolation versus atomicity?
What does non-interference mean for two concurrent executions?
What does containment mean physically versus semantically?
What is authority reduction versus adversarial isolation?
What does trusted_local actually trust?
What does contained_local actually contain?
What boundaries exist across process, PID/cgroup/Job Object, mount namespace, user identity,
filesystem, network, credentials, environment and kernel?
What can one same-authority target mutate despite path/file commitments?
What is protection from accidents versus protection from hostile code?
What is multi-tenant isolation?
What is secrecy/confidentiality versus integrity versus availability?
How does capability/authority interact with isolation?
When does cancellation/kill prove containment and when can child/external Effects escape?
What is an isolation boundary versus merely an observation/cleanup boundary?
```

Mandatory falsifiers:

```text
trusted_local process writes another Workspace
contained_local process reaches host credential path
same UID process signals/ptraces sibling
mount namespace P4 target-view substitution
network egress from locally contained process
child daemon escapes supervisor ownership
Windows limited token versus elevated administrator
Windows Job Object KILL_ON_JOB_CLOSE without filesystem/network isolation
read-only immutable input tree but mutable external dependency
shared package/build caches across Workspaces
root-owned Runtime executing user-controlled repositories
container boundary with privileged kernel escape assumptions
VM boundary versus namespace/container boundary
```

Do not enter RF14 Determinism/Nondeterminism until RF13 establishes which portions of the
execution world are actually isolated/frozen versus merely trusted, observed or bounded.
