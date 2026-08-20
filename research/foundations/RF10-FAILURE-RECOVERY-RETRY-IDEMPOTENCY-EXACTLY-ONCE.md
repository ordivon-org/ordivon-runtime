---
schema_version: 1
id: runtime.foundations.rf10
title: RF10 — Failure, Recovery, Retry, Idempotency and Exactly-Once
type: report
profile: research
lifecycle: completed
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - reader
  - researcher
  - builder
  - agent
updated: 2026-08-17
summary: RF10 separates failure from uncertainty, retry from recovery/reconciliation/resume/compensation, request idempotency from effect idempotency, and at-most-once dispatch from exactly-once effect. It classifies current Runtime replay as idempotent admission/reattachment, current Attempt dispatch as at-most-once, Lost/Orphaned as explicit uncertainty/conservative terminal states, and structured Release/Patch effects as adapter-specific reconcilable contracts rather than evidence that arbitrary execution is exactly-once.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF10
related:
  - runtime.foundations.start
  - runtime.foundations.rf9
  - runtime.foundations.rf10.sources
  - runtime.foundations.rf10.continuation
---
# RF10 — Failure, Recovery, Retry, Idempotency and Exactly-Once

## 0. Status and research question

RF9 separated admission, queueing, scheduling and dispatch. RF10 asks:

> **When execution, communication or observation breaks, what exactly failed, what is merely
> unknown, what may safely be repeated, what must be recovered in place, and at which
> boundary—request, admission, dispatch, execution or external Effect—can any at-most-once,
> at-least-once, idempotent or exactly-once claim truthfully hold?**

RF10 is explanatory research only. It does not authorize automatic retries, multi-Attempt
retry, generic effect adapters, exactly-once marketing claims, provider idempotency-key
support, checkpoint/resume, compensation workflows or state-machine changes.

---

# 1. First deletion — “failed” is too coarse

The sentence:

```text
The operation failed.
```

can mean:

```text
request rejected before commitment
admission committed but response was lost
dispatch did not occur
dispatch may have occurred but outcome is unknown
process started then crashed
process timed out
process exited nonzero
process exited zero but semantic Goal failed
external Effect partially committed
external Effect committed but receipt was lost
state is inconsistent and needs reconciliation
```

These are not interchangeable.

---

# 2. Failure

RF10 uses **Failure** only relative to a contract/claim:

> **A Failure is evidence that a required condition, transition or postcondition of a named
> contract was not achieved or cannot be achieved by the current realization.**

Therefore:

```text
Failure(Q) must name the failed claim Q.
```

---

# 3. Uncertainty is not failure

**Uncertainty/Ambiguity** means available evidence is insufficient to conclude which of
multiple materially different outcomes occurred.

Example:

```text
request sent
connection breaks
server may have committed Effect
client receives no response
```

The correct state may be:

```text
Unknown
```

rather than:

```text
Failed
```

Thus:

```text
UnknownOutcome != FailedOutcome
```

---

# 4. Timeout is an observation/control conclusion, not proof of non-occurrence

RF5 established observer-relative timeout semantics.

If an external POST times out, the caller knows:

```text
response did not arrive before its deadline
```

not necessarily:

```text
server did not apply the operation
```

Therefore:

```text
Timeout != EffectDidNotOccur
```

---

# 5. Transport disconnect is not execution state

Current Runtime already states:

```text
client reconnection does not change Attempt state
transport disconnects are not Attempt states
```

A lost MCP response can coexist with a durably admitted/running/succeeded Job.

Thus:

```text
DeliveryFailure != ExecutionFailure
```

---

# 6. Delivery disposition and execution disposition are orthogonal

Current terminal evidence projects separately:

```text
executionDisposition
deliveryDisposition
processTreeDisposition
```

This is foundationally correct.

For example:

```text
executionDisposition = succeeded
deliveryDisposition = unknown
```

is coherent if physical execution concluded but result delivery/commit evidence is
uncertain.

---

# 7. Semantic completion is another independent axis

Current Agent-facing projection explicitly keeps:

```text
semanticCompletionEvaluated = false
```

because:

```text
ProcessExitZero != GoalSatisfied
```

Thus RF10 distinguishes at least:

```text
transport/delivery outcome
Runtime execution outcome
external Effect outcome
domain/semantic outcome
```

---

# 8. Crash

A **Crash** is termination/loss of a process/component execution context that prevents its
normal future steps.

Crash location matters:

```text
client crash
Runtime daemon crash
Runner crash
target crash
OS/WSL crash
provider crash
```

No generic crash semantic exists without naming the owner and durable state.

---

# 9. Omission failure

An omission is expected communication/action evidence that never becomes observed within
the relevant contract.

Examples:

```text
reply omitted
result file never appears
heartbeat missing
provider receipt unavailable
```

Omission can leave outcome ambiguous rather than proving the action did not occur.

---

# 10. Partial failure

A compound operation can cross some physical/effect boundaries and fail before others.

Examples:

```text
file A changed, file B not changed
payment committed, notification failed
new binary installed, service probe failed
```

Thus:

```text
CompoundFailure != NoEffect
```

and partial state requires domain-specific reconciliation/compensation semantics.

---

# 11. Recovery

RF10 defines **Recovery** broadly as:

> **Actions that restore or converge the system to a valid, explainable state after crash,
> interruption, ambiguity or invariant violation, preferably by using durable identity and
> surviving evidence rather than blindly repeating work.**

Recovery may or may not execute the original work again.

---

# 12. Reconciliation

**Reconciliation** is a recovery technique:

> compare independent durable/physical truth owners and converge the durable model toward
> the strongest justified current/historical conclusion without inventing missing facts.

Current Runtime reconciliation compares:

```text
Registry
Runner Result
systemd/cgroup/process identity
Windows launcher/native evidence
structured Effect receipt where applicable
```

---

# 13. Reconciliation is not retry

Current maintenance reconciliation explicitly does not retry failed commands or redispatch
ambiguous opaque work.

Therefore:

```text
Reconcile != Retry
```

A reconciliation pass may discover that work already succeeded.

---

# 14. Retry

**Retry** means initiating another attempt to perform a requested action/operation after an
earlier attempt failed, was rejected or became ambiguous.

Retry can happen at many layers:

```text
transport packet retry
HTTP request retry
Runtime admission retry
Attempt re-execution
external Effect retry
```

These have different safety conditions.

---

# 15. Retry is not replay

Current Runtime **replay** of the same admitted request means:

```text
return/recover the already committed Job
```

not:

```text
run the command again
```

Therefore:

```text
HistoricalReplay/Reattachment != RetryExecution
```

---

# 16. Resume

**Resume** continues one interrupted realization from saved progress/checkpoint state rather
than starting the realization from its beginning.

Examples:

```text
restore VM checkpoint
resume download range
restart model training from checkpoint
```

Current Runtime arbitrary Attempts do not provide generic checkpoint/resume semantics.

Thus:

```text
Resume != RetryFromStart
```

---

# 17. Compensation

**Compensation** is a new domain action intended to counterbalance/reverse business meaning
of an already committed Effect when true rollback is unavailable.

Examples:

```text
refund after payment
cancel future reservation after booking
submit inverse accounting entry
```

Compensation itself can fail and creates another Effect.

Therefore:

```text
Compensation != Rollback
```

---

# 18. Rollback

RF10 reserves **Rollback** for a mechanism that restores a transaction/state scope to a
prior valid state under a contract that actually owns atomic rollback semantics.

A database transaction rollback and an external financial refund are not the same thing.

---

# 19. Idempotency

RF10 defines **Idempotency** relative to an effect equivalence relation:

> repeating the same logical request/action one or more times has the same intended effect
> under the contract as performing it once.

HTTP semantics defines idempotent methods in essentially this effect-relative way; it also
warns that non-idempotent requests should not be automatically retried without stronger
knowledge that retry is safe.

Thus:

```text
Idempotent != SideEffectFree
```

---

# 20. Safe/read-only and idempotent remain different

HTTP PUT and DELETE are idempotent but not safe/read-only.

Similarly an operation can perform a real mutation while duplicate logical requests
converge to the same intended effect.

Therefore:

```text
Idempotent != ReadOnly
```

---

# 21. Request idempotency is not Effect idempotency

Current Runtime guarantees:

```text
same principal + clientRequestId + same request identity
→ same Job
```

This is **admission/request identity idempotency**.

It does not prove arbitrary target external effects are idempotent.

Thus:

```text
RequestIdempotency != ExternalEffectIdempotency
```

---

# 22. Idempotency key is not magic

An idempotency key only helps if the owner that can duplicate the relevant effect:

```text
stores/recognizes the key
binds it to the same operation semantics
rejects/conflicts on semantic mismatch
retains it for the required failure window
returns/reconstructs the original outcome
```

A client-side string alone cannot create idempotency.

---

# 23. Current `clientRequestId` is a Runtime admission key

Current Registry key namespace is effectively:

```text
(principal, clientRequestId)
```

and stores/reconstructs one Job, while request identity digest rejects semantic reuse of the
same key with changed request semantics.

This is a strong durable request-deduplication contract.

---

# 24. `IdempotencyConflict` is as important as duplicate replay

If the same key is reused for a different request identity, Runtime fails closed rather
than aliasing two Operations.

Thus:

```text
SameKey + DifferentMeaning
→ Conflict
```

not “latest wins.”

---

# 25. Response-loss recovery is the core use case

If Runtime commits admission but the MCP response is lost:

```text
caller retries same authenticated identity/request
→ existing Job returned
```

No second Attempt is created merely because delivery failed.

This is precisely the failure that durable request idempotency solves.

---

# 26. At-most-once

**At-most-once** for event/action A means:

```text
count(A) ≤ 1
```

It permits zero occurrences.

Therefore:

```text
AtMostOnce != ExactlyOnce
```

---

# 27. Current physical dispatch is at-most-once per Attempt

Current Runtime commits `dispatch_issued` / Attempt `starting` before crossing the physical
provider boundary and does not speculatively redispatch an ambiguous Attempt.

Thus the intended contract is:

```text
PhysicalDispatch(Attempt) ≤ 1
```

---

# 28. At-most-once deliberately permits Lost

If dispatch intent is durable but Runtime cannot prove whether provider launch happened,
repeating launch would risk duplication.

Current Runtime instead allows:

```text
Lost / Orphaned / recovery-required
```

This is a direct consequence of at-most-once safety.

---

# 29. `DISPATCH_OUTCOME_UNKNOWN` is semantically valuable

A dispatch intent without matching unit/start/result evidence after the defined recovery
window can converge to a `lost` conclusion rather than triggering replacement execution.

This preserves:

```text
we do not know enough
```

instead of manufacturing certainty.

---

# 30. At-least-once

**At-least-once** for A means the system will continue/retry until at least one occurrence
is achieved under its model, accepting possible duplicates:

```text
count(A) ≥ 1
```

This requires a repeat strategy and usually duplicate tolerance/deduplication downstream.

---

# 31. Current opaque execution is not at-least-once

Because Runtime intentionally refuses speculative redispatch after ambiguous dispatch,
current one-Attempt arbitrary execution does not promise that physical execution definitely
occurred at least once.

It prefers:

```text
possible zero + no duplicates
```

over:

```text
force one-or-more + duplicate risk
```

---

# 32. Exactly-once

**Exactly-once** for A means:

```text
count(A) = 1
```

under a precisely named observation/effect domain.

Any exactly-once statement without naming A, owner, lifetime and equivalence relation is
underspecified.

---

# 33. “Exactly-once Operation” is usually too broad

An Operation can contain:

```text
admission
physical dispatch
process steps
filesystem writes
network calls
provider mutations
semantic domain effects
```

Each can have different delivery semantics.

Therefore RF10 requires scope-qualified claims such as:

```text
exactly-once admission identity
at-most-once dispatch
idempotent Workspace Patch effect
reconcilable release Effect
```

rather than one global label.

---

# 34. Exactly-once RPC historically depends on deduplication + surviving server identity/state

Classic RPC systems use unique call identifiers and duplicate suppression so retransmitted
calls after lost acknowledgements do not re-execute the procedure. But crash/restart and
state retention determine how strong that claim remains.

RF10 lesson:

```text
Exactly-once-like call semantics require owner-retained deduplication state,
not reliable transport alone.
```

---

# 35. Communication reliability cannot by itself prove Effect exactly-once

Even perfectly retransmitted bytes cannot tell a client whether a remote non-idempotent
Effect committed before a connection broke unless the Effect owner provides deduplication,
receipt/state query or a transaction protocol.

Thus:

```text
ReliableTransport != ExactlyOnceEffect
```

---

# 36. Kafka demonstrates exactly-once is scope-bounded

Kafka's idempotent producer deduplicates producer retries in the Kafka log; its transaction
model atomically coordinates Kafka output records and consumed offsets for Kafka-integrated
processing.

Kafka documentation itself scopes this guarantee to Kafka's storage/transaction domain.

RF10 lesson:

> exactly-once becomes tractable when one owner can atomically bind input progress,
> deduplication and output state.

---

# 37. Kafka does not magically make arbitrary external side effects exactly-once

If a Kafka consumer additionally calls an unrelated external HTTP payment API, Kafka's
transaction cannot atomically commit that provider's world unless the external system joins
an appropriate transaction/idempotency contract.

Thus:

```text
ExactlyOnceWithinDomain != ExactlyOnceAcrossOpenWorld
```

---

# 38. Atomicity is not exactly-once by itself

A transaction can guarantee:

```text
all-or-none state update
```

but if the client loses the commit response, it may still be uncertain whether commit
occurred unless transaction identity/state can be queried/recovered.

Therefore:

```text
AtomicCommit != ClientKnowledgeOfOutcome
```

---

# 39. Deduplication

**Deduplication** detects multiple presentations of the same logical operation/message and
suppresses/reuses one committed logical outcome under an identity window.

Deduplication is a mechanism often used to implement idempotency/exactly-once-like
semantics, but:

```text
Deduplication != semantic idempotency by itself
```

because identity/effect equivalence must still be correct.

---

# 40. Deduplication window matters

If a provider forgets idempotency keys after TTL T, retry after T may be treated as a new
Effect.

Therefore:

```text
IdempotencyGuarantee has a retention/lifetime.
```

Current Runtime retains Job/idempotency history append-oriented precisely because replay and
recovery depend on it.

---

# 41. Durable history is part of idempotency infrastructure

Deleting idempotency rows/Job history independently can turn a previously safe duplicate
request into a new admission.

Current docs therefore reject manual per-record deletion and require archival/cutover
contracts that preserve replay dependencies.

---

# 42. Retry safety is boundary-specific

A retry can be:

```text
Safe      — duplicate cannot add unintended effect
Unsafe    — duplicate may cause another effect
Unknown   — current evidence cannot prove either
```

This must be evaluated at the repeated boundary, not from generic “retryable=true” alone.

---

# 43. Pre-commit capacity retry is safe in a narrow sense

Current `CONCURRENCY_LIMIT` has:

```text
commitState = not_started
```

and no Job exists.

Retrying admission later does not duplicate a previously committed Runtime Job because
none was admitted.

This is very different from retry after dispatch ambiguity.

---

# 44. Retry after committed admission should usually reattach, not re-admit new semantics

If response loss happens after admission, the same request identity should locate the same
Job.

Using a new clientRequestId would instead request a new Operation and can create duplicate
physical/effect work.

Thus:

```text
ResponseLossRecovery → SameIdentityReattachment
```

not “new key, try again.”

---

# 45. Retry after dispatch ambiguity is unsafe for OPAQUE work

Current effect-kernel classifies arbitrary execution as `OPAQUE` because Runtime does not
know the number/identity of external effects inside the command.

Therefore:

```text
Ambiguous opaque dispatch/execution
→ never automatically repeat
```

This is one of current Runtime's strongest safety laws.

---

# 46. Process exit nonzero does not necessarily make retry safe

A program can:

```text
commit external effect
then crash/exit 1
```

So:

```text
ExitNonZero != NoExternalEffect
```

Arbitrary execution remains effect-opaque regardless of process status.

---

# 47. Process exit zero does not prove external effect success either

Likewise a command may swallow provider errors and exit 0.

Thus:

```text
ExitZero != ExternalEffectCommitted
```

and certainly not semantic completion.

---

# 48. Cancellation does not make previous effects disappear

RF3/RF6 already established:

```text
CancelExecution != RollbackEffect
```

RF10 adds:

```text
CancelThenRetry
```

is unsafe unless downstream effects are known idempotent/reconcilable.

---

# 49. Current `Lost` is evidence-scoped terminal uncertainty, not “definite zero executions”

A Lost Job says Runtime cannot prove a conclusive execution result under the available
identity/evidence contract.

It should not be interpreted as:

```text
target definitely never ran
```

or:

```text
external effects definitely did not occur
```

---

# 50. `Orphaned` is stronger uncertainty about ownership/identity continuity

Current Orphaned semantics retain reservation because recorded unit/process/cgroup may
still own live execution while identity confidence is insufficient for normal control.

This is a conservative unresolved-ownership condition.

---

# 51. Late evidence can correct prior conservative conclusions

Current tests prove an orphaned terminal record can later be superseded by a matching
identity-bound Runner Result, yielding the stronger execution conclusion.

Thus:

```text
RecoveryConclusion can be monotonic in evidence strength without erasing history.
```

---

# 52. Recovery is evidence revision, not history rewriting

The earlier orphan/lost observation remains part of append-oriented evidence history; a
later stronger artifact supersedes the previous conclusion.

This matches RF4 identity/history law:

```text
Correction != PretendEarlierUncertaintyNeverExisted
```

---

# 53. Reconciliation should prefer stronger direct evidence

Current Runtime lets verified Runner terminal result defeat delayed cancellation intent
because observed completed execution is stronger than a later desire to stop it.

This establishes an evidence precedence relation, not “last event wins.”

---

# 54. Reconciliation is not arbitrary inference

Runtime Doctor refuses to guess when runner evidence is missing; manual review/finalization
remains explicit.

Thus:

```text
NoEvidence != PermissionToInventTerminalOutcome
```

---

# 55. Manual repair is not automatic recovery

Admin repair requires:

```text
state snapshot
fingerprint
row versions
explicit selected cases
atomic batch semantics
principal/audit
```

and rejects stale/corrupt inputs.

This makes repair a deliberate authority-bearing mutation of durable truth, not background
heuristic reconciliation.

---

# 56. Repair can finalize unknown state without proving historical effect absence

An operator may explicitly finalize a Lost case to restore Registry consistency/capacity.

That administrative convergence does not retroactively prove arbitrary external effects
never occurred.

Therefore:

```text
AdministrativeFinalization != ExternalWorldProof
```

---

# 57. Resume requires checkpoint identity

A future resumable Attempt would need:

```text
checkpoint identity/digest
what state it captures
which external effects are already committed
compatibility with current code/environment
who owns checkpoint truth
```

Without this, “resume” is just retry/restart under another name.

---

# 58. Multi-Attempt retry would be a new contract

Current Job/Attempt model can represent Attempt histories, but current arbitrary execution
does not automatically create another Attempt after failure/ambiguity.

A future multi-Attempt retry policy would need to state:

```text
which terminal states allow retry
same Operation or new Operation?
authority revalidation?
input/environment revalidation?
external effect class?
capacity identity?
which Attempt result resolves the Job?
```

RF10 does not authorize it.

---

# 59. Same Operation can have multiple Attempts only if the contract defines equivalence

If retry changes:

```text
source
input
provider
credentials
budget
external state expectation
```

it may no longer be the same Operation realization.

RF4/RF7 identity rules therefore constrain retry design.

---

# 60. Exactly-once admission is easier than exactly-once execution

Runtime can atomically bind one idempotency key to one Job in SQLite because it owns both
identity and commit transaction.

It does not similarly own every external effect of arbitrary target code.

Therefore guarantee strength decreases across ownership boundaries.

---

# 61. At-most-once dispatch is easier than exactly-once successful execution

Runtime can ensure it does not intentionally issue the same Attempt dispatch twice.

But provider/OS crash can mean:

```text
zero realized executions
or one execution with unknown outcome
```

So at-most-once dispatch cannot promise exactly one completed execution.

---

# 62. Exactly-once Effect requires Effect-owner cooperation

For an external Effect E, a robust exactly-once-like contract generally needs the owner of
E to provide some combination of:

```text
stable operation/effect identity
durable deduplication/idempotency key
atomic commit or compare-and-set preconditions
durable receipt/current state query
sufficient retention window
reconciliation semantics
```

Generic Runtime cannot synthesize these from process supervision.

---

# 63. Structured Effect contracts are therefore special

Current Runtime has exactly two materially different structured Effects:

```text
Runtime self-release
Workspace Patch
```

They graduated because Runtime owns stable identity, preconditions, receipt/state evidence,
replay and ambiguity behavior for those specific domains.

---

# 64. Runtime Release is RECONCILABLE, not a proof that process execution is exactly-once

Release binds:

```text
deterministic effect ID
request identity
commit
candidate manifest
receipt path
Job
```

and `release.get` reads the deployment receipt without dispatching/retrying.

The canonical receipt can be stronger evidence of external release occurrence than generic
process state.

---

# 65. Release exact replay is request/effect reattachment

If the same release request is replayed after response loss, Runtime returns the existing
Job/effect binding before current-world checks.

It does not create a new release Effect merely to answer the replay.

---

# 66. Release can still require reconciliation

Receipt directory unavailable, malformed/mismatched receipt, rollback/recovery failure or
unknown receipt status produce:

```text
ReconciliationRequired
```

rather than automatic redeploy.

This is exactly the RF10 principle:

```text
UnknownEffectOutcome → inspect/reconcile, not repeat blindly.
```

---

# 67. Workspace Patch is a second structured recovery model

Workspace Patch durably records:

```text
request identity
prepared intent
complete before/after file digest plan
```

before direct mutation and stores states:

```text
prepared
committed
unknown
```

`workspace.patch.get` reconciles physical files against the plan without replaying a mixed
mutation.

---

# 68. Patch `unknown` proves structured Effect recovery can preserve mixed state

If files are neither consistently before nor consistently after the committed patch plan,
Runtime reports `unknown` rather than applying the patch again.

Thus:

```text
StructuredEffect != AlwaysRetryable
```

Even a structured adapter may need explicit ambiguity state.

---

# 69. Atomic filesystem commit is scoped

Workspace Patch can provide strong local effect semantics because Runtime owns the exact
Workspace mutation mechanism and before/after state model.

That does not generalize to arbitrary external APIs.

---

# 70. Effect classes are recovery contracts, not caller labels

Current effect-kernel explicitly refuses caller-authored `effectClass` for arbitrary
execution.

Why:

```text
caller description cannot make downstream operation idempotent/reconcilable
```

A non-opaque class must be implemented/tested by the adapter that owns the effect contract.

---

# 71. OPAQUE is an epistemic boundary

`OPAQUE` does not mean target is malicious or poorly implemented.

It means generic Runtime lacks sufficient ownership/evidence to safely deduplicate,
reconcile or repeat its external effects.

Therefore:

```text
Opaque != UnsafeProgram
Opaque = UnknownEffectContractToRuntime
```

---

# 72. Exactly-once is often implemented as at-least-once transport + idempotent/deduplicated effect

A common systems pattern is:

```text
retry delivery until acknowledged
+
receiver deduplicates stable operation identity
```

so duplicates in transport do not duplicate logical effect.

This demonstrates:

```text
ExactlyOnceLogicalEffect != ExactlyOneNetworkTransmission
```

---

# 73. Exactly-one physical execution is usually not the useful business property

If a payment API receives the same logical payment twice but deduplicates to one transfer,
there may have been multiple request executions but one logical Effect.

Conversely one process execution can itself perform the payment twice.

Thus business exactly-once must name **effect identity**, not process count.

---

# 74. At-most-once can trade liveness for safety

Current Runtime's ambiguous dispatch policy can leave a Job Lost rather than risking a
second launch.

This sacrifices guaranteed completion to preserve no-speculative-duplicate execution.

Therefore:

```text
AtMostOnceSafety can reduce Liveness.
```

---

# 75. At-least-once trades duplicate risk for liveness

A system that keeps retrying until it observes success improves probability of occurrence
but requires duplicate-tolerant/idempotent effect semantics.

Thus:

```text
AtLeastOnce without deduplication can duplicate effects.
```

---

# 76. Exactly-once combines safety and liveness only inside stronger assumptions

To get both:

```text
no missing logical effect
no duplicate logical effect
```

one needs sufficient durable identity, owner cooperation, retry/recovery and failure model.

Open-world arbitrary execution does not satisfy this generically.

---

# 77. Failure detector uncertainty matters

A missing supervisor process/unit can mean different things by target contract.

For Windows current R-W5, loss of launcher lineage plus `KILL_ON_JOB_CLOSE` semantics proves
the native tree cannot still be running, allowing deterministic failure rather than generic
Lost.

Thus stronger substrate contracts can shrink ambiguity.

---

# 78. Same observation can imply different outcome under different execution substrates

For Linux trusted-local:

```text
unit missing + no result
```

may require generic lost/failure reasoning.

For Windows native with kill-on-job-close lineage guarantee:

```text
launcher gone
```

can prove child tree termination.

Therefore:

```text
FailureClassification depends on substrate contract.
```

---

# 79. Recovery quality increases with independent truth surfaces

Examples:

```text
Registry intent
provider/unit identity
process identity
Runner start digest
Runner result
cgroup populated state
Effect receipt
filesystem before/after digest
```

Independent evidence can close ambiguity that one source cannot.

RF11 will study evidence more deeply.

---

# 80. But more telemetry is not automatically stronger proof

Logs/traces that are not identity-bound, durable or owned by the relevant effect domain may
be merely hints.

RF10 therefore distinguishes:

```text
RecoveryEvidence
```

from arbitrary observability output.

---

# 81. Transactional outbox/inbox pattern illustrates boundary co-location

The core idea of transactional messaging patterns is to atomically commit business state
and a durable record of the message/effect intent in the same transaction domain, then
perform/deduplicate delivery separately.

RF10 lesson:

> move the ambiguity boundary to a place where one owner can atomically commit the local
> facts it truly owns, rather than pretending remote delivery joined the transaction.

---

# 82. Current Runtime admission follows the same general discipline

Runtime atomically commits:

```text
Job
Attempt
reservation
idempotency identity
```

before physical dispatch.

It does not attempt a fictitious transaction spanning SQLite and systemd/Windows/external
provider.

---

# 83. Dispatch intent acts like a durable handoff marker

`mark_dispatch_issued` records the at-most-once boundary before launch.

After a crash, Runtime can reason:

```text
accepted only → dispatch still eligible
starting/dispatch_issued → never speculatively issue again
```

This is a crucial recovery distinction.

---

# 84. Accepted-before-dispatch can be safely cancelled/released under current contract

If an Attempt is still Accepted, no dispatch intent has crossed.

Current Registry can cancel it and release reservation.

Therefore:

```text
PreDispatchCancellation != PostDispatchCancellation
```

and RF10 can use the persisted boundary to classify retry/recovery safety.

---

# 85. Starting is the ambiguity frontier

Once `dispatch_issued` is committed, the provider boundary may have been crossed even if no
binding evidence is visible yet.

Therefore Starting cannot be treated as Accepted-after-delay.

---

# 86. Running has stronger execution identity but still not effect identity

A bound unit/cgroup/PID proves target process realization, not arbitrary external API
outcomes performed inside it.

Thus:

```text
RunningEvidence != EffectReceipt
```

---

# 87. Terminal Runner Result is strongest local execution truth after tree disappearance

Current invariant says once process tree is proven gone, a complete identity-bound Runner
Result is terminal execution truth.

This is a strong **local execution** conclusion only.

---

# 88. Failure should be classified by stage

RF10 proposes a conceptual matrix:

```text
PreAdmissionFailure
AdmissionResponseLoss
PreDispatchFailure
DispatchAmbiguity
RunningFailure
ExecutionTerminalFailure
EffectAmbiguity
EffectFailure
SemanticFailure
RecoveryFailure
```

These classes support safer retry policy than one boolean `failed`.

---

# 89. Retry policy should consume commit/effect classification, not exception type alone

An I/O exception can happen:

```text
before bytes sent
after request committed
after external effect committed
```

Therefore:

```text
ExceptionClass != RetrySafetyClass
```

This matches HTTP's rule that automatic retry safety depends on method/effect semantics, not
just connection failure.

---

# 90. `retryable=true` must remain scope-specific

For Runtime admission errors, retryable can mean:

```text
re-attempt admission later because no commitment occurred
```

It must never be interpreted generically as:

```text
repeat any previously dispatched external action
```

---

# 91. Effect receipt is not process receipt

A release deployment receipt can prove deployment Effect facts even if initiating process
state is ambiguous.

Conversely Runner result can prove process exit while no domain effect receipt exists.

Thus:

```text
ExecutionReceipt != EffectReceipt
```

---

# 92. External state query can implement reconciliation without replay

If an Effect owner exposes canonical state keyed by stable effect ID, recovery can ask:

```text
Did effect E commit? What is its result?
```

before considering any repeat.

This is strictly stronger than retry-first recovery for non-idempotent effects.

---

# 93. Idempotency and reconciliation are complementary

Idempotency makes duplicate presentation safer.

Reconciliation reduces unnecessary duplicate presentation by learning whether the first
presentation already committed.

A strong adapter may use both.

---

# 94. Preconditions add another safety layer

Compare-and-set/state-generation preconditions can reject stale duplicate operations even
if idempotency history is incomplete.

Current Workspace Patch before-digest plan and Release candidate/commit bindings exemplify
this.

---

# 95. Idempotency alone cannot fix wrong intent

If the caller accidentally generates two distinct idempotency keys for the same intended
payment, the provider may legitimately perform two Effects.

Therefore:

```text
IdempotencyKeyGeneration is part of semantic identity correctness.
```

---

# 96. Reusing one key for different intent is equally dangerous

Runtime's `IdempotencyConflict` correctly rejects this.

A provider that silently returns the old result for changed parameters under the same key
would conflate identities.

Thus effect adapters need key-to-operation binding.

---

# 97. Exactly-once has a garbage-collection problem

Deduplication state cannot be retained forever for free.

Deleting it creates a point after which old retries may become new operations.

Therefore exactly-once/idempotency claims must specify retention/epoch/cutover semantics.

---

# 98. Runtime's append-oriented retention is a deliberate current tradeoff

It preserves replay/recovery evidence at the cost of unbounded historical growth until an
explicit archival/versioned cutover exists.

RF10 validates this as correctness infrastructure, not merely logging.

---

# 99. Recovery can be convergent without being automatically completing

Current Runtime may converge to:

```text
Lost
Orphaned
ReconciliationRequired
Unknown Patch state
```

These are valid stable conclusions when evidence is insufficient for success/failure.

Thus:

```text
RecoverySuccess != OperationSuccess
```

Recovery succeeds when system truth becomes consistent/explainable, even if the operation
outcome remains adverse or unknown.

---

# 100. “Exactly once” should be decomposed into boundary contracts

For one Runtime Operation:

```text
Request/Admission:
  same key+meaning → one durable Job

Dispatch:
  one Attempt → physical dispatch issued at most once

Local execution:
  may succeed/fail/timeout/lost/orphaned; no generic exactly-one successful execution claim

External Effect:
  opaque unless structured adapter owns identity/receipt/reconciliation

Semantic Goal:
  Host/domain owned; no exactly-once claim from Runtime
```

This is substantially more accurate than one global delivery label.

---

# 101. RF10 current-operation guarantee ladder

```text
strongest generic Runtime guarantee
        │
        ├─ durable idempotent request reattachment
        ├─ one committed Operation identity per key+meaning
        ├─ durable admission before dispatch
        ├─ at-most-once physical dispatch per Attempt
        ├─ process-tree ownership/evidence
        ├─ conservative ambiguity states + reconciliation
        │
        └─ NO generic external-effect idempotency/exactly-once
```

Structured adapters can add stronger effect-level guarantees locally.

---

# 102. Cross-domain falsifier matrix

| Case | Collapse attacked | Surviving distinction |
|---|---|---|
| admission committed, response lost | delivery failure = admission failure | replay reattaches same Job |
| capacity reject before commit | all retries dangerous | pre-commit admission retry is safe |
| dispatch intent persisted, launch evidence missing | no result = safe retry | ambiguous dispatch must not redispatch opaque work |
| target exits 1 after payment | execution failed = no effect | external Effect may already exist |
| target exits 0 but API silently failed | process success = Effect success | effect proof is independent |
| HTTP POST times out after server commit | timeout = non-occurrence | outcome is ambiguous |
| same Runtime clientRequestId | same key = run again | idempotent replay returns original Job |
| same key, different request digest | dedup = ignore payload difference | idempotency conflict protects identity |
| new key after lost response | “retry” = same operation | can create duplicate Operation/effect |
| at-most-once dispatch | exactly-once execution | zero realization remains possible |
| at-least-once retry | no lost effects | duplicates possible without dedup |
| Kafka idempotent producer | exactly once everywhere | scope is Kafka log/transaction domain |
| external API outside Kafka txn | Kafka EOS = external EOS | open-world side effect remains separate |
| Patch mixed before/after state | structured = always retryable | adapter can still return unknown |
| Release receipt exists, process state unclear | process truth = effect truth | effect receipt can be stronger |
| Orphaned then late Runner result | terminal state can never improve | stronger evidence may supersede uncertainty |
| Doctor missing evidence | recovery = guess terminal | manual review remains required |
| compensation refund | compensation = rollback | inverse business Effect is new action |
| idempotency TTL expires | key = eternal guarantee | retention window bounds guarantee |
| suspend/resume without checkpoint contract | resume = retry | checkpoint identity/state is required |

---

# 103. RF10 anti-laws

RF10-L1: `UnknownOutcome != FailedOutcome`.

RF10-L2: `Timeout != EffectDidNotOccur`.

RF10-L3: `DeliveryFailure != ExecutionFailure`.

RF10-L4: `ExecutionSuccess != SemanticCompletion`.

RF10-L5: `Reconcile != Retry`.

RF10-L6: `Replay/Reattachment != RetryExecution`.

RF10-L7: `Resume != RetryFromStart`.

RF10-L8: `Compensation != Rollback`.

RF10-L9: `Idempotent != SideEffectFree`.

RF10-L10: `Idempotent != ReadOnly`.

RF10-L11: `RequestIdempotency != ExternalEffectIdempotency`.

RF10-L12: `IdempotencyKey != IdempotencyWithoutOwnerState`.

RF10-L13: `AtMostOnce != ExactlyOnce`.

RF10-L14: `AtMostOnceDispatch != ExactlyOnceExecution`.

RF10-L15: `AtLeastOnce != NoDuplicates`.

RF10-L16: `ExactlyOnceOperation != OneUniversalUnscopedProperty`.

RF10-L17: `ReliableTransport != ExactlyOnceEffect`.

RF10-L18: `ExactlyOnceWithinDomain != ExactlyOnceAcrossOpenWorld`.

RF10-L19: `AtomicCommit != ClientKnowledgeOfOutcome`.

RF10-L20: `Deduplication != SemanticIdempotencyByItself`.

RF10-L21: `ExitNonZero != NoExternalEffect`.

RF10-L22: `ExitZero != ExternalEffectCommitted`.

RF10-L23: `Lost != DefinitelyNeverRan`.

RF10-L24: `AdministrativeFinalization != ExternalWorldProof`.

RF10-L25: `RunningEvidence != EffectReceipt`.

RF10-L26: `ExceptionClass != RetrySafetyClass`.

RF10-L27: `RetryableAdmission != RetryableExternalEffect`.

RF10-L28: `ExecutionReceipt != EffectReceipt`.

RF10-L29: `RecoverySuccess != OperationSuccess`.

RF10-L30: `CancelExecution != RollbackEffect`.

---

# 104. Minimum RF10 grammar

## 104.1 Failure `Fail(Q,evidence)`

Named contract/postcondition Q is proven unmet.

## 104.2 Unknown/Ambiguous `Unknown(Q)`

Evidence cannot distinguish materially different outcomes for Q.

## 104.3 Recovery `Recover(S,evidence)`

Restore/converge system truth/state after failure/ambiguity.

## 104.4 Reconciliation `Recon(model,owners)`

Compare independent truth owners and converge to strongest justified conclusion.

## 104.5 Retry `Retry(A)`

Initiate another attempt/presentation of action A.

## 104.6 Replay/Reattachment `Replay(id)`

Return/reconstruct previously committed identity/history without new realization.

## 104.7 Resume `Resume(checkpoint)`

Continue same realization from committed checkpoint state.

## 104.8 Compensation `Compensate(effect)`

New domain Effect intended to offset prior committed Effect.

## 104.9 Idempotency `Idem(A,≈E)`

Repeated logical A yields effect equivalent to one application under ≈E.

## 104.10 At-most-once

`count(A) ≤ 1`.

## 104.11 At-least-once

`count(A) ≥ 1` under declared liveness/failure assumptions.

## 104.12 Exactly-once

`count(A) = 1` under declared identity/domain/failure model.

## 104.13 Receipt

Identity-bound evidence authored/owned by the effect/execution domain that can support
reconciliation of occurrence/result.

---

# 105. Current Runtime failure/recovery map

| Current mechanism/state | RF10 interpretation |
|---|---|
| `clientRequestId` + request digest | durable admission idempotency identity |
| exact replay | reattachment to existing Job, not re-execution |
| `IdempotencyConflict` | same key/different meaning rejection |
| admission transaction | exactly-one durable Job binding under key/meaning |
| `Accepted` | admitted, not yet dispatch-issued |
| `dispatch_issued` / `Starting` | durable at-most-once dispatch boundary |
| `Running` | stronger local realization identity |
| Runner Result | local execution terminal evidence |
| `Lost` | conclusive Runtime uncertainty/failure classification, not no-effect proof |
| `Orphaned` | unresolved ownership/identity condition with reservation held |
| late Runner recovery | stronger evidence supersedes uncertainty |
| reconciliation | converge Registry/provider/process evidence without retry |
| Doctor/repair | explicit audited recovery for cases automation cannot prove |
| `CONCURRENCY_LIMIT commitState=not_started` | safe future re-admission class |
| arbitrary `workspace.exec` | `OPAQUE` effect contract; no generic retry/effect idempotency |
| Runtime Release | structured RECONCILABLE Effect with deterministic receipt |
| `release.get` | effect reconciliation/query without redispatch |
| Workspace Patch | structured local Effect with prepared/committed/unknown states |
| `workspace.patch.get` | before/after reconciliation without repeating mixed mutation |
| `semanticCompletionEvaluated=false` | explicit boundary beyond Runtime execution truth |

---

# 106. What RF10 strongly supports in current design

## 106.1 Durable idempotent admission before physical work

Response loss does not create duplicate Jobs when callers replay exact identity.

## 106.2 Dispatch intent before provider crossing

This enables at-most-once dispatch recovery instead of guessing whether to launch again.

## 106.3 Conservative Lost/Orphaned states

Unknown outcomes remain explicit and block unsafe speculative repetition.

## 106.4 Evidence-first reconciliation

Late identity-bound evidence can refine conclusions; missing evidence is not guessed.

## 106.5 Separate structured Effect contracts

Release and Patch prove that stronger idempotency/reconciliation belongs where Runtime
actually owns Effect identity/preconditions/receipts.

## 106.6 Opaque arbitrary execution

Generic execution correctly refuses caller-declared idempotency/exactly-once semantics.

---

# 107. What RF10 reopens

## 107.1 Error/retry vocabulary may need more explicit scope

Agent-facing `retryable` is useful but can be misunderstood across boundaries. Future tool
contracts may benefit from more explicit retry class/commit state/effect class where the
existing generated error envelope does not already cover it. No API change is authorized.

## 107.2 Multi-Attempt retry remains undefined

A real consumer would need to justify same-Operation retry and define effect safety,
authority/environment revalidation and Job resolution rules.

## 107.3 Provider idempotency is adapter-specific

Finance/cloud/message providers may expose idempotency keys or transaction identifiers, but
generic Runtime should import them only through structured adapters that own retention and
reconciliation semantics.

## 107.4 Checkpoint/resume is workload-specific

Long model training/build/data workflows may eventually justify resumable state, but a
checkpoint contract must precede any generic “resume Attempt” feature.

---

# 108. Research constitution added by RF10

1. Never infer non-occurrence from timeout, disconnect or missing response.
2. Distinguish proven failure from unknown outcome.
3. Keep transport delivery, Runtime execution, external Effect and semantic outcome axes
   separate.
4. Treat replay/reattachment as historical identity recovery, not execution retry.
5. Never automatically redispatch an opaque operation after dispatch ambiguity.
6. Scope idempotency to the owner/effect equivalence relation it actually protects.
7. Treat idempotency keys as contracts requiring durable owner-side binding/retention, not
   magic caller strings.
8. Keep request idempotency separate from external-effect idempotency.
9. State at-most-once/at-least-once/exactly-once relative to a specific event/effect boundary.
10. Accept that at-most-once may produce Lost/zero execution; safety can trade liveness.
11. Do not promote process success/failure into external Effect occurrence proof.
12. Prefer reconciliation/state query over blind retry when the first Effect may have
    committed.
13. Preserve ambiguity (`Lost`, `Orphaned`, `unknown`, `ReconciliationRequired`) when proof
    is insufficient.
14. Allow stronger late evidence to supersede a conservative conclusion without erasing
    historical evidence.
15. Separate compensation from rollback; compensation is another Effect.
16. Require effect-owner cooperation for exactly-once external effects.
17. Require retention/cutover semantics for any durable deduplication guarantee.
18. Treat multi-Attempt retry and checkpoint/resume as new contracts requiring concrete
    consumers.
19. Do not infer a generic Effect framework from two structured adapters with shared
    vocabulary but different physical commit mechanisms.
20. Do not enter RF11 until evidence/truth can be studied independently from retry policy.

---

# 109. RF10 result — conceptual compression

```text
Failure says a named contract is proven unmet.
Unknown says evidence cannot distinguish outcomes.
Recovery restores/converges valid explainable state.
Reconciliation compares surviving truth owners without blindly repeating work.
Retry starts another attempt/presentation.
Replay returns the already committed historical identity.
Resume continues one realization from a checkpoint.
Compensation creates a new inverse/offsetting domain Effect.
Idempotency makes repeated logical requests effect-equivalent to one under a defined owner.
At-most-once forbids duplicates but allows zero.
At-least-once pursues occurrence but may duplicate.
Exactly-once combines both only inside a precisely scoped identity/effect/failure contract.
```

The strongest Runtime-specific compression is:

```text
Runtime generic execution:
  request/admission replay = idempotent
  physical dispatch/Attempt = at-most-once
  execution outcome = evidence-backed, may be Lost/Orphaned
  arbitrary external Effect = opaque
  semantic completion = outside Runtime

Structured adapters:
  may add effect identity + receipt + reconciliation
  but only for the domain they actually own
```

---

# 110. RF10 closeout

RF10 closes with twenty-four durable results:

1. **Failure and uncertainty are distinct; timeout/disconnect do not prove non-occurrence.**
2. Transport delivery, physical execution, external Effect and semantic completion are
   orthogonal outcome axes.
3. Recovery, reconciliation, retry, replay/reattachment, resume and compensation are
   distinct operations.
4. **Current exact request replay is durable reattachment to the same Job, not execution
   retry.**
5. `clientRequestId` + request identity provides durable Runtime admission idempotency;
   changed meaning under the same key fails `IdempotencyConflict`.
6. Response-loss after admission is recovered by same-identity replay rather than creating
   a new Job.
7. `Idempotent` means repeated logical requests have equivalent intended effect under a
   contract; it does not mean read-only or side-effect-free. HTTP's standard semantics make
   the same distinction.
8. Request/admission idempotency does not imply arbitrary external-effect idempotency.
9. **Current Attempt physical dispatch is at-most-once: dispatch intent is durable before
   provider crossing and ambiguous dispatch is never speculatively repeated.**
10. At-most-once permits zero realizations and therefore can converge to Lost/unknown rather
    than duplicate work.
11. Current generic opaque execution does not promise at-least-once or exactly-once physical
    execution.
12. Process exit zero/nonzero cannot prove external Effect success/absence for arbitrary
    commands.
13. `Lost` is not proof the target never ran; `Orphaned` is unresolved ownership/identity
    uncertainty with conservative capacity retention.
14. Late identity-bound Runner evidence can supersede prior conservative uncertainty while
    append-oriented history preserves both observations.
15. Reconciliation converges surviving truth; it does not retry failed/ambiguous commands.
16. Doctor/repair correctly refuses to guess and requires explicit audited repair when
    evidence cannot decide.
17. Exactly-once claims must be scoped to a specific event/effect/owner; one global
    “exactly-once Operation” label is misleading.
18. Kafka's exactly-once mechanisms illustrate that strong guarantees become possible when
    one transaction/deduplication domain can atomically bind input progress and output; they
    do not automatically extend to unrelated external effects.
19. **Exactly-once external Effect requires Effect-owner cooperation** such as durable
    identity/deduplication, atomic/preconditioned commit, receipt/state query and retention.
20. Runtime Release is a structured reconcilable external Effect: deterministic effect
    identity plus deployment receipt lets `release.get` reconcile without redispatch.
21. Workspace Patch is a separate structured Effect with prepared/committed/unknown state;
    mixed state is preserved as unknown rather than blindly repeated.
22. Structured Release/Patch contracts validate the Effect-contract shape but do not make
    arbitrary execution idempotent or justify a generic EffectAdapter.
23. Compensation is a new domain Effect, not rollback; checkpoint resume is distinct from
    retry from the beginning.
24. **Current Ordivon Runtime's strongest generic reliability shape is durable idempotent
    admission + at-most-once dispatch + evidence-first reconciliation + explicit ambiguity,
    not generic exactly-once execution/effects.**

The next frontier is:

> **What counts as evidence, observation, receipt, proof, truth source and confidence?
> How should Runtime expose facts without turning one observer's projection into global
> truth, and how do competing evidence sources supersede or constrain one another?**

That is RF11 — Observation, Evidence, Proof and Truth Surfaces.
