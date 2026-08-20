---
schema_version: 1
id: runtime.foundations.rf15
title: RF15 — External World and Open-System Effects
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
summary: RF15 establishes the open-system Effect model for Runtime. It separates command/request, transport delivery, provider acceptance, provider-side operation identity, external state transition, observation/receipt, reconciliation and semantic outcome. External Reality is independently mutable and usually outside Runtime transaction, isolation and determinism boundaries. Runtime can safely make generic claims only about its own Operation/Attempt/mechanical evidence; stronger external-effect claims require an Effect owner/provider contract that supplies identity, state, query/receipt, idempotency/retention and convergence semantics. Release and Workspace Patch remain structured owner-specific examples rather than evidence for a universal Effect kernel. Finance partial fills, cloud asynchronous operations, SMTP acceptance/delivery and HTTP response-loss cases demonstrate why acceptance and final outcome are distinct. The governing law is ExternalEffectClaimScope <= ProviderOwnedEvidenceAndControlScope.
evidence_status: verified
readiness: READY
applies_to:
  - RUNTIME-FOUNDATIONS-001
  - RF15
related:
  - runtime.foundations.start
  - runtime.foundations.rf14
  - runtime.foundations.rf15.sources
  - runtime.foundations.rf15.continuation
---
# RF15 — External World and Open-System Effects

## 0. Status and research question

RF14 established that Runtime deliberately leaves many world dimensions open. RF15 asks:

> **When execution crosses into an independently changing external domain, what exactly is an
> Effect, what stages can occur between a local command and a domain outcome, which facts can
> Runtime prove, and how can a system safely reconcile with a world it does not own?**

RF15 is explanatory research only. It does not authorize a generic Effect kernel, universal
provider adapters, automatic external retry, trading/payment/cloud/email adapters, webhook
infrastructure, generic compensation or production API changes.

---

# 1. Open system

RF15 defines an **Open System** as one whose behavior/state can be influenced by actors,
resources or processes outside the modeled authority boundary.

Examples:

```text
market
cloud provider
remote Git service
SMTP network
human organization
payment network
public web API
```

---

# 2. Closed-system intuition fails at external boundaries

Inside one owner, we may reason:

```text
state before
operation
state after
```

Across an open boundary:

```text
our request
provider processing
other actors
provider state transitions
asynchronous work
later observation
```

can all interleave.

Therefore:

```text
ExternalEffect != LocalFunctionCall
```

---

# 3. External Effect

RF15 defines an **External Effect** as:

> **A change, obligation, transfer, action or externally visible state transition in a domain
> whose authoritative state is owned outside the initiating Runtime Operation.**

Examples:

```text
exchange order/fill
payment capture
cloud resource creation
email acceptance/delivery
GitHub issue mutation
DNS record change
human receives/acts on message
```

---

# 4. Local state transition versus External Effect

A Registry row update is Runtime-owned local state.

An OKX order is exchange-owned state.

A Workspace Patch is local filesystem state under a dedicated Runtime-owned effect contract.

A cloud instance is provider-owned state.

Thus:

```text
PhysicalLocation != EffectOwnership
```

Ownership is semantic/control authority, not simply “remote versus local.”

---

# 5. The open-system pipeline

RF15 introduces the minimal staged model:

```text
Intent
  ↓
RequestConstruction
  ↓
TransportAttempt
  ↓
ProviderAcceptance
  ↓
ProviderOperation/DomainTransition
  ↓
ProviderObservation/Receipt
  ↓
Reconciliation
  ↓
SemanticOutcome
```

Not every domain exposes every stage.

---

# 6. Intent

**Intent** is what the caller/domain wants to cause.

Example:

```text
“buy 1 ETH”
“deploy version V”
“send message M”
```

Intent is not evidence that any external request was sent.

---

# 7. Request

A **Request** is a concrete message/action proposal submitted toward a provider/domain.

Examples:

```text
HTTP POST bytes
exchange place-order request
SMTP DATA transaction
cloud CreateResource call
```

Request existence does not prove provider receipt or acceptance.

---

# 8. Transport delivery

Transport may establish that bytes were written or a connection progressed.

It generally cannot prove how far provider semantic processing proceeded after a disconnect.

Thus:

```text
BytesSent != ProviderAccepted
```

and:

```text
NoResponse != ProviderRejected
```

---

# 9. Provider Acceptance

**Provider Acceptance** means the provider has acknowledged responsibility for/request intake
under its contract.

Acceptance may still precede final external Effect completion.

---

# 10. HTTP response success is method/resource-contract scoped

HTTP defines request-method semantics and idempotency of intended server effects, but generic
HTTP success is not a universal business-outcome proof.

The resource/provider contract determines what a response status means operationally.

---

# 11. Response loss ambiguity

Canonical failure:

```text
client POST
server commits external operation
server sends response
connection fails
client receives nothing
```

Client knowledge is:

```text
unknown
```

while provider reality may already be:

```text
committed
```

Therefore:

```text
ClientUnknown != ProviderUncommitted
```

---

# 12. Idempotency can bridge response-loss ambiguity only under provider cooperation

A provider-side key can map repeated logical requests to one effect/result.

But the key's semantics, parameter equality, retention horizon and result caching belong to
the provider contract.

---

# 13. Stripe demonstrates owner-side idempotency

Stripe stores the first result under an idempotency key and returns that result to retries;
parameters are compared for misuse. Its documentation also specifies key pruning after a
retention period, after which reuse can create a new request.

The important abstraction is:

```text
CallerKey + ProviderDurableBinding + Retention
```

not merely:

```text
CallerKey
```

---

# 14. Idempotency horizon

RF15 defines **Idempotency Horizon** as the period/state horizon for which the Effect owner
still guarantees key→effect/result deduplication.

After it expires:

```text
same key
```

may no longer mean:

```text
same Effect
```

under provider semantics.

---

# 15. Runtime replay horizon and provider idempotency horizon are distinct

Runtime can remember a Job longer than a provider remembers an external idempotency key.

Therefore:

```text
RuntimeHistoricalIdentity != ProviderDeduplicationLifetime
```

---

# 16. Provider Operation

Many APIs create a separate **Provider Operation** identity rather than completing the effect
synchronously.

Conceptually:

```text
request
→ operationToken
→ IN_PROGRESS
→ SUCCESS/FAILED
```

---

# 17. AWS Cloud Control is a direct example

Cloud Control resource mutation returns a `ProgressEvent` with a `RequestToken`; operation
status can be `PENDING`, `IN_PROGRESS`, `SUCCESS`, `FAILED`, `CANCEL_IN_PROGRESS` or
`CANCEL_COMPLETE`. The token is then queried independently.

Therefore:

```text
RequestAccepted != ResourceTransitionComplete
```

---

# 18. Provider operation identity differs from resource identity

A cloud request token identifies:

```text
this create/update/delete operation
```

while resource identifier identifies:

```text
the cloud resource
```

Thus:

```text
ProviderOperationId != ExternalResourceId
```

---

# 19. Resource identity can become known before final operation success

Some provider contracts can surface a resource identifier while the create operation is still
in progress.

This illustrates:

```text
ResourceIdentityKnown != ResourceReady
```

---

# 20. Asynchronous completion

External Effects can continue after local process/request completion.

Example:

```text
CLI exits after provider accepts create
cloud operation continues for minutes
```

Therefore:

```text
LocalProcessFinished != ExternalEffectTerminal
```

---

# 21. Current generic Runtime gets this exactly right

`workspace.exec` returns:

```text
executionDisposition
Runtime deliveryDisposition
result/evidence
semanticCompletionEvaluated=false
```

It deliberately does not claim arbitrary provider/domain completion.

---

# 22. Runtime `deliveryDisposition=committed` is local/mechanical

Current enum explicitly describes Runtime certainty about delivery of the **physical
execution result**.

So:

```text
RuntimeDeliveryCommitted
```

means:

```text
Runtime has durably committed a conclusive local execution result
```

not:

```text
all external Effects committed
```

---

# 23. Generic process exit zero remains effect-opaque

A command can:

```text
send remote request
receive “accepted”
print success
exit 0
```

while external operation remains pending or later fails.

Thus:

```text
ExitZero != ExternalEffectTerminalSuccess
```

---

# 24. Generic process exit nonzero also cannot prove no Effect

A process may:

```text
submit order successfully
then crash parsing response
exit 1
```

Therefore:

```text
ExitNonZero != NoExternalEffect
```

---

# 25. Structured Effect contract

RF15 defines a **Structured Effect Contract** as a domain-specific contract that names at
least some of:

```text
Effect identity
Effect owner
request identity
provider operation identity
resource identity
commit/acceptance frontier
receipt/query mechanism
state machine
idempotency semantics
retention horizon
reconciliation rules
```

---

# 26. Stronger external claims require stronger provider semantics

The general rule is:

```text
ExternalEffectClaimScope <= ProviderOwnedEvidenceAndControlScope
```

This is RF15's governing law.

---

# 27. Receipt

RF10 defined receipt as identity-bound evidence authored/owned by the effect/execution domain.

RF15 adds:

> a receipt must state **which frontier/fact** it evidences.

---

# 28. Acceptance receipt versus completion receipt

A provider response might mean:

```text
request accepted
```

or:

```text
resource fully created
```

These cannot share a generic “success receipt” semantics.

Therefore:

```text
AcceptanceReceipt != CompletionReceipt
```

---

# 29. Receipt is not semantic outcome

A payment provider may say:

```text
payment captured
```

while business Goal was:

```text
customer successfully received goods
```

Thus:

```text
ProviderReceipt != DomainGoalCompletion
```

---

# 30. Observation

An **Observation** is a fact read from an external owner at a time.

Examples:

```text
GET order status
poll cloud operation
fetch remote object
receive webhook
```

Observation is time-scoped.

---

# 31. External truth has freshness

RF15 defines observation:

```text
Obs(resource,state,observedAt,source)
```

A correct old observation may be stale now.

Therefore:

```text
CorrectAt(t1) != CurrentAt(t2)
```

---

# 32. Freshness is not truth quality alone

A provider-authoritative response can be perfectly authentic yet too old for a decision.

Hence external evidence needs both:

```text
authority
freshness
```

---

# 33. Eventual consistency complicates read-after-write

A provider may accept/commit a mutation on one subsystem while a read replica briefly returns
older state.

So:

```text
PostWriteReadMissing != EffectDefinitelyAbsent
```

unless the provider contract guarantees strong read-after-write consistency on that path.

---

# 34. Reconciliation

RF15 defines **External Reconciliation** as:

> comparing Runtime/local durable intent/history with authoritative provider observations or
> receipts to infer the best supported external Effect state without blindly repeating the
> action.

---

# 35. Reconciliation is not retry

Reconciliation asks:

```text
what happened?
```

Retry asks:

```text
perform again?
```

Therefore:

```text
Reconcile != Retry
```

---

# 36. Reconciliation may be read-only

Ideal external reconciliation often uses:

```text
GET/status query
receipt lookup
operation token query
order lookup by client ID
```

and does not mutate the world.

---

# 37. Reconciliation can itself observe stale/partial truth

If provider reads are eventually consistent, one query may be insufficient.

A reconciliation algorithm may need:

```text
retrying reads
multiple endpoints
webhook + polling
terminal-state rules
freshness windows
```

---

# 38. Query identity matters

Searching “recent orders” heuristically is weaker than querying an exact provider-owned order
ID/client effect key.

Therefore:

```text
HeuristicSearch != IdentityBoundReconciliation
```

---

# 39. ForeignReference is correlation, not effect proof

Current Runtime can bind foreign references from Host/Edge/Harness.

These preserve relation/lineage identity.

They do not by themselves prove an external Effect's state.

---

# 40. External resource can change after our Effect

Example:

```text
Runtime changes GitHub issue title to A
another human changes it to B
later query sees B
```

It would be false to infer:

```text
our update never happened
```

from current state B alone.

---

# 41. State observation is not causal history

Current resource state tells:

```text
what is now
```

not necessarily:

```text
which actor transitions occurred earlier
```

Thus:

```text
CurrentState != CompleteCausalHistory
```

---

# 42. Causation needs identity/history evidence

Strong attribution can use:

```text
provider request ID
actor/audit log
version ID
order/trade ID
operation token
resource generation
```

when provider supplies them.

---

# 43. Compare-and-set/preconditions can strengthen causation

External APIs with version/ETag/generation preconditions can say:

```text
apply this mutation only if resource is still version V
```

This closes a race that blind last-write-wins cannot.

---

# 44. Preconditions are not transactions across domains

They strengthen one provider mutation relative to provider-owned state.

They do not make Runtime Registry + provider state one transaction.

---

# 45. Finance exposes the richest open-effect state machine

An order can pass through:

```text
submitted
accepted/live
partially_filled
filled
canceled
```

and fills may occur incrementally.

---

# 46. OKX acceptance is explicitly not final amend outcome

OKX documents that a successful amend response only means the amend request was accepted by
the server; the actual result is determined by order-channel updates or order-status query.

This is a direct real-world instance of:

```text
ProviderAcceptance != DomainTransitionResult
```

---

# 47. Partial fill is a first-class Effect state

For an order of size 10:

```text
filled 3
remaining 7 live
```

means some external Effects already occurred.

It is neither:

```text
not executed
```

nor:

```text
fully completed
```

---

# 48. Order Effect versus Fill Effects

A useful finance decomposition is:

```text
Order resource O
  has Fill effects F1...Fn
```

Order identity and fill/trade identities are different.

---

# 49. Terminal canceled can coexist with committed fills

OKX order semantics explicitly allow an IOC order to be partially filled and then canceled,
with non-zero accumulated fill size.

Therefore:

```text
OrderCanceled != NoTradeOccurred
```

---

# 50. Cancel is another external Effect/request

A cancel request races with matching/fill activity.

Possible reality:

```text
cancel requested
fill occurs
cancel becomes terminal afterward
```

So:

```text
CancelRequested != RemainingQuantityDefinitelyUnfilled
```

---

# 51. Cancel acknowledgment scope must be provider-defined

A provider may acknowledge cancel request acceptance before cancellation is final.

A structured adapter must distinguish those states if the API does.

---

# 52. Financial semantic outcome is higher still

Even fully filled order does not answer:

```text
Was strategy decision correct?
Was risk objective satisfied?
Was PnL positive?
```

So:

```text
OrderFilled != TradingGoalSucceeded
```

---

# 53. Payment has analogous layers

Conceptually:

```text
request
provider accepted
payment authorized/captured
settlement
refund/chargeback
business fulfillment
```

Different provider/domain contracts place receipts at different frontiers.

---

# 54. Provider “500” does not universally mean no Effect

Stripe's documented idempotency layer can persist and replay a first result even when that
result is HTTP 500 after endpoint execution begins.

The broader lesson:

```text
HTTPStatusClass alone != ExternalEffectOccurrence
```

---

# 55. Validation failure is a different frontier

A provider can guarantee that validation failed before endpoint execution began.

Then retry can be safer because no effect execution was initiated under that contract.

This distinction belongs to the provider, not generic transport inference.

---

# 56. Error taxonomy should be frontier-aware

Useful external classifications include:

```text
not_sent
rejected_before_effect
accepted_pending
committed/terminal
partially_committed
unknown
reconciliation_required
```

A generic `error=true` erases too much.

---

# 57. Cloud asynchronous operations make this obvious

A create call can return:

```text
IN_PROGRESS + RequestToken
```

and only later become:

```text
SUCCESS
```

or:

```text
FAILED
```

The initial network call has completed, while the provider Effect has not.

---

# 58. Cloud cancellation is also asynchronous

Provider states such as:

```text
CANCEL_IN_PROGRESS
CANCEL_COMPLETE
```

show that cancellation itself is an operation, not instantaneous erasure.

---

# 59. Resource stabilization differs from request success

A resource may exist but not yet satisfy readiness/stability conditions.

Therefore:

```text
Created != Ready
```

where provider/domain semantics distinguish them.

---

# 60. Deployment domains often have multiple commit frontiers

Example:

```text
bytes installed
service restarted
health probe passed
traffic switched
old version removed
```

“deployed” must name which frontier the contract owns.

---

# 61. Current Runtime Release is intentionally domain-specific

It owns deterministic release Effect identity, a deployment receipt, rollback status and
`release.get` projection.

That stronger vocabulary is valid because Runtime itself owns the release mechanism and
receipt contract.

---

# 62. Runtime Release receipt outranks generic process status for release state

Current Tool docs explicitly state the deployment receipt, not process exit alone, is
authoritative for whether the release Effect committed.

RF15 strongly supports this.

---

# 63. `release.get` is a pure effect projection

It joins durable Runtime Job/Attempt truth with the deterministic deployment receipt without
redispatching.

This is exactly the reconciliation shape RF15 wants for structured Effects.

---

# 64. Release rollback remains a new domain transition

If deployed state is restored to the previous version:

```text
Effect history still contains deployment attempt + rollback
```

RF12 already established:

```text
RolledBack != NeverHappened
```

RF15 places that in open-effect history.

---

# 65. Workspace Patch is a different kind of structured Effect

Patch owns:

```text
before-state digests
intended after-state digests
physical mutation
reconciliation by current file state
```

It is local, but effect-like because it crosses a durable intent→physical state boundary.

---

# 66. Patch reconciliation shows owner-specific truth

If all files match after-digests:

```text
committed
```

If all remain before:

```text
prepared/not committed
```

If mixed:

```text
unknown
```

This works because Runtime knows the exact state equivalence relation for this Effect.

---

# 67. Two structured Effects still do not imply universal Effect schema

Release and Patch differ in:

```text
resource owner
commit frontier
receipt mechanism
rollback semantics
reconciliation query
```

Shared words such as `committed/unknown` do not erase physical differences.

---

# 68. Effect contract must follow domain ontology

Finance needs:

```text
orders/fills/trades
```

Cloud needs:

```text
operation/resource/stabilization
```

Email needs:

```text
acceptance/relay/delivery/DSN
```

Human world may need:

```text
message seen/acknowledged/action taken
```

A single generic state enum is unlikely to be truthful.

---

# 69. SMTP is a strong acceptance-versus-delivery example

SMTP/DSN standards distinguish server acceptance of responsibility from later final delivery
or failure notification.

A successful acceptance does not mean a human has read or acted on the message.

---

# 70. Email has multiple semantic frontiers

Conceptually:

```text
API accepted message
MTA accepted responsibility
recipient mailbox delivery
recipient user agent retrieval
human read
human understood
human acted
```

Each frontier has a different possible evidence source.

---

# 71. SMTP “delivery” itself has a technical definition

In DSN semantics, local delivery can mean placement into the recipient mailbox/service, not
human retrieval or comprehension.

Thus:

```text
SMTPDelivered != HumanRead
```

---

# 72. Human outcomes often lack machine-authoritative receipts

Examples:

```text
person noticed email
person changed belief
person performed physical task
```

may only be evidenced by later messages, sensor data or direct acknowledgement.

Runtime cannot manufacture stronger certainty.

---

# 73. Human-world actions are often partially observable

A message can cause an action without explicit acknowledgment.

Conversely, acknowledgment can occur without the desired action.

Therefore:

```text
Acknowledged != GoalCompleted
```

---

# 74. Provider acceptance can transfer responsibility

SMTP is instructive: once server accepts mail under protocol semantics, responsibility shifts
to that server to deliver/relay or report failure.

This is different from final success but materially stronger than mere transport delivery.

---

# 75. Responsibility transfer is a useful open-system concept

RF15 defines **Responsibility Transfer Frontier** as the point where a downstream owner has
accepted contractual responsibility for continuing an action/outcome path.

Examples:

```text
SMTP server accepts message
cloud service accepts async operation
exchange accepts live order
```

---

# 76. Responsibility transfer is not outcome transfer

After acceptance, downstream owner may still legitimately end in failure/cancel states.

So:

```text
ResponsibilityAccepted != OutcomeSucceeded
```

---

# 77. External actors create interference

Provider resource may be modified by:

```text
other API clients
humans
provider automation
market matching engine
controllers
```

Therefore local intent does not monopolize causality.

---

# 78. Open-world invariant ownership is distributed

If Runtime does not own all writers, it cannot promise that external resource stays in a
state merely because it once observed/created that state.

Thus:

```text
EffectCommittedAt(t1) != ResourceStillEqualAt(t2)
```

---

# 79. Desired state versus observed state

Controllers often maintain:

```text
desired state D
observed external state O
```

then repeatedly reconcile toward D.

This is different from one-shot transaction semantics.

---

# 80. Kubernetes-style reconciliation is a competing root model

A controller does not assume one API write permanently solves the world; it continuously
compares desired and observed state and acts until/concurrently while conditions change.

RF15 uses this conceptual pattern without making Runtime itself a generic controller.

---

# 81. Convergence is open-system specific

In independently mutable reality, success can mean:

```text
system currently converged to desired state
```

rather than:

```text
one historical command executed successfully
```

---

# 82. Current Runtime should not own semantic convergence generically

Host/domain is better positioned to know whether:

```text
site is healthy
portfolio matches policy
human task is done
```

Runtime provides operations/evidence.

---

# 83. Effect authority

RF15 separates three authorities:

```text
Action Authority       who may request/cause the external action
Observation Authority  who can authoritatively report external state
Semantic Authority     who decides whether the domain Goal is satisfied
```

They may be different actors.

---

# 84. Example: Finance

```text
Runtime/executor      action mechanism
OKX                   authoritative order/fill state
Finance policy/Host   semantic strategy/risk interpretation
```

No one surface owns all three.

---

# 85. Example: Cloud

```text
Runtime/CLI           action execution
AWS control plane     resource/operation truth
Host/domain            product/service completion judgment
```

---

# 86. Example: Email

```text
Runtime/mail client   send action
SMTP/provider         transport/delivery truth
human/domain          read/action meaning
```

---

# 87. Observation authority can be narrower than resource ownership

A webhook may report an event but not expose the complete current resource state.

A query API may expose current state but omit historical transitions.

So different provider surfaces own different facts.

---

# 88. Webhook versus polling

Webhook:

```text
provider pushes event
```

Polling:

```text
client asks current state
```

Neither universally dominates.

---

# 89. Webhook can arrive before/after a poll observes corresponding state

Network and replication paths differ.

Therefore:

```text
DeliveryOrderOfObservations != ProviderCausalOrder
```

unless provider contract guarantees ordering.

---

# 90. Event identity matters for webhook deduplication

If provider supplies stable event ID, consumer can deduplicate repeated delivery of the same
notification.

Again:

```text
WebhookDeliveryIdempotency != UnderlyingEffectIdempotency
```

---

# 91. Notification loss does not imply Effect loss

Provider may commit Effect but webhook never reaches consumer.

A query path can be essential for reconciliation.

---

# 92. Notification duplication does not imply Effect duplication

At-least-once webhook delivery commonly means the same event notification may arrive more
than once.

This is a transport/event-delivery property, distinct from domain-effect multiplicity.

---

# 93. Event stream and resource state can disagree transiently

A newly received event may represent a transition not yet visible on a stale read replica, or
a later state may already have superseded it.

Consumers need ordering/version/freshness semantics.

---

# 94. External version/generation is a powerful reconciliation primitive

If resources expose:

```text
version
revision
generation
ETag
sequence
```

then observations can be ordered or conditionally mutated more safely.

---

# 95. Version is owner-relative

Provider revision number orders that provider resource history; it does not globally order
Runtime Job history, network events and human observations.

RF5's partial-order discipline still applies.

---

# 96. Partial success is normal in open systems

Examples:

```text
3/10 order quantity filled
7/10 recipients delivered
4/5 cloud subresources created
2/3 API calls succeeded
```

A binary succeeded/failed status can erase actionable truth.

---

# 97. Structured adapters should preserve domain partiality

Do not flatten:

```text
partially_filled
```

into:

```text
failed
```

if fills are economically real.

---

# 98. Partial effect may require compensation for only the committed subset

A Saga must know what actually committed before deciding compensating action.

Blind rollback is impossible.

---

# 99. Compensation may race further external change

Selling back an asset after a partial buy occurs in a new market state.

Deleting a cloud resource may race consumers created after it.

So:

```text
Compensation != TimeReversal
```

---

# 100. Reversibility is domain-specific

Potential classes:

```text
exactly reversible within owner transaction
compensatable approximately
cancelable before commit
irreversible
informationally irreversible
```

Do not infer reversibility from a generic DELETE/undo command.

---

# 101. Information Effects are special

Once information is sent/read/published:

```text
confidentiality impact
human knowledge
external caches
```

cannot be perfectly undone.

Thus email/publication compensation is usually another message, not rollback.

---

# 102. External Effect state machine can outlive Runtime history activity

A provider operation may remain pending after local Job terminal state.

Therefore a higher layer may need to maintain external observation/reconciliation after the
Runtime process has ended.

---

# 103. This does not imply Runtime Job should remain Running

Local execution can legitimately be terminal while external provider operation is pending.

Hence:

```text
RuntimeExecutionTerminal != DomainEffectTerminal
```

---

# 104. Structured adapter can expose both axes

A future domain adapter could return:

```text
runtimeExecution = succeeded
providerOperation = in_progress
semanticCompletion = unknown
```

without contradiction.

---

# 105. Current Release already partially demonstrates multi-axis state

Its projection includes both:

```text
release Effect disposition
Attempt/execution state
Runtime delivery disposition
semanticCompletionEvaluated=false
```

This is good ontology.

---

# 106. Effect terminality is owner-defined

For OKX order:

```text
filled/canceled
```

can be terminal order states.

For cloud operation:

```text
SUCCESS/FAILED/CANCEL_COMPLETE
```

are operation terminal states.

For email:

technical delivery may be terminal for MTA but not for human Goal.

---

# 107. Terminal resource state differs from terminal operation state

An operation can finish successfully, leaving a mutable resource that continues to evolve.

Thus:

```text
OperationTerminal != ResourceImmutable
```

---

# 108. Effect identity versus state identity

An order's ID persists as state changes from live→partial→filled.

So:

```text
Effect/ResourceIdentity != StateFingerprint
```

RF4 identity theory applies externally too.

---

# 109. External identity should be provider-owned when available

Runtime should not invent an exchange order ID or cloud request token.

It can correlate its own Operation with provider identifiers.

---

# 110. Client-assigned IDs can be powerful if provider validates/retains them

Examples include client order IDs or idempotency keys.

Their strength comes from provider binding, uniqueness and retention semantics.

---

# 111. Client ID and provider ID may coexist

Useful relation:

```text
Runtime Operation ID
↔ client effect key
↔ provider operation/resource ID
```

Each lives in a different namespace.

---

# 112. Do not collapse correlation into identity

A client order ID may identify the caller's logical request while provider order ID identifies
provider's resource.

They may map 1:1 under contract, but that mapping itself is a provider fact.

---

# 113. Receipt retention is part of recoverability

If provider status/history expires quickly, late reconciliation can become impossible even
when effect identity was once known.

Thus:

```text
RecoveryHorizon <= ProviderEvidenceRetentionHorizon
```

unless caller independently preserves authoritative receipt material.

---

# 114. OKX history retention illustrates horizon sensitivity

Provider documentation distinguishes pending/live order views and limited history windows for
completed/canceled orders.

This reinforces that reconciliation capability decays with provider retention policy.

---

# 115. Durable local receipt copies can preserve historical evidence

If provider returns a signed/authoritative receipt, Runtime/domain can store it as immutable
Artifact.

But storing a receipt does not make it a current-state query.

---

# 116. Historical receipt and current observation answer different questions

```text
Receipt: what provider asserted at t1
Query: what provider reports at t2
```

Both can be simultaneously correct when resource changed.

---

# 117. Provider audit logs can bridge history and current state

Where available, provider-owned audit/event history may establish causal transitions more
strongly than a single current GET.

This is adapter/domain-specific.

---

# 118. Network timeout classification

After timeout:

```text
not sent
partly sent
provider received
provider accepted
provider committed
response lost
```

may all be observationally compatible at client side.

Therefore generic Runtime must preserve:

```text
unknown
```

unless stronger provider evidence exists.

---

# 119. DNS/TLS/connect failures can sometimes narrow uncertainty

Failure before any application request bytes are transmitted may support stronger
non-occurrence claims for that provider request.

But generic command execution often does not expose enough transport-level proof to Runtime.

---

# 120. Adapter can provide stronger phase evidence

A dedicated provider client may know:

```text
request never left process
request rejected locally
provider returned validation error
provider assigned request ID
```

This justifies structured adapter value.

---

# 121. Effect state and caller knowledge state are distinct

RF15 models:

```text
ExternalRealityState E
ObserverKnowledgeState K
```

with potentially:

```text
E=committed
K=unknown
```

or:

```text
E=changed again
K=stale committed observation
```

---

# 122. Knowledge convergence may lag effect convergence

Provider Effect may be terminal before client learns terminal state.

Thus:

```text
EffectTerminalTime != ObserverKnowledgeTerminalTime
```

---

# 123. Multiple observers can disagree without provider inconsistency

One observer may have stale cache/webhook delay while another reads current primary state.

RF11 evidence scope/freshness rules govern resolution.

---

# 124. Semantic outcome belongs above provider facts

Provider can truthfully report:

```text
order filled
resource created
mail delivered
```

while only domain layer knows whether these satisfy the user's Goal.

This validates current:

```text
semanticCompletionEvaluated=false
```

---

# 125. Runtime should expose mechanism facts, not business judgments

For generic exec:

```text
process started
process exited
stdout captured
result committed
```

are Runtime facts.

“Deployment succeeded for users” may not be.

---

# 126. Host/domain should compose Effect evidence

Host can reason:

```text
Runtime Job result
+ provider order receipt
+ later account state
+ task objective
```

and decide semantic progress.

---

# 127. Provider adapter should own effect-specific reconciliation

Because only it understands:

```text
provider IDs
terminal states
partial states
retention
idempotency
query semantics
```

Generic Runtime cannot infer these from stdout reliably.

---

# 128. Parsing stdout is not equivalent to provider receipt ownership

A CLI may print JSON that came from provider, but without a structured contract Runtime does
not know whether:

```text
it is authoritative
complete
current
correlated to the intended Effect
```

So generic exec remains OPAQUE.

---

# 129. Artifact can carry evidence without changing ownership

Runtime may store stdout/provider receipt bytes as Artifact.

Ownership of the fact remains the provider/domain that authored those bytes.

---

# 130. Signed receipts can strengthen authenticity, not semantics beyond signature scope

A signature can prove source/integrity of a receipt.

It does not make “accepted” mean “completed.”

---

# 131. Exactly-once remains provider/effect scoped

RF10 established the requirement for effect-owner cooperation.

RF15 adds the open-world limitation:

```text
exactly-once order creation
```

would not imply:

```text
exactly-once fill
```

or:

```text
exactly-once business outcome
```

unless those are part of the same owner contract.

---

# 132. One order can intentionally produce many fills

Therefore counting fills and counting order effects are different event cardinalities.

Exactly-once must name what object/event is counted.

---

# 133. HTTP idempotency is intended-effect scoped

HTTP defines idempotent methods by repeated identical requests having the same intended server
effect, even though logging/history side effects can still differ.

This is the same scope discipline Runtime should preserve.

---

# 134. POST can become safely retryable only with extra semantics

Generic POST is not idempotent by method semantics. A specific API can add idempotency-key or
resource-specific guarantees.

Therefore:

```text
TransportMethod != CompleteRetryPolicy
```

---

# 135. Conditional request can reduce lost-update races

HTTP conditional requests/ETags can make mutation contingent on observed resource version.

Again, provider-supported precondition semantics are stronger than client-side hope.

---

# 136. Open-world safety often prioritizes reconciliation before retry

If first request might have committed:

```text
query exact effect/resource identity first
```

is often safer than:

```text
blindly submit again
```

This remains the core RF10→RF15 reliability pattern.

---

# 137. When exact query is impossible, ambiguity can be permanent

Human-world action or provider-retention expiry may leave no authoritative way to decide
historical occurrence.

Systems must sometimes retain:

```text
unknown
```

forever.

---

# 138. Unknown is a legitimate terminal epistemic state

Unknown does not mean execution still running.

It means:

```text
available evidence cannot decide historical external Effect state
```

---

# 139. Terminal epistemic unknown differs from provider pending

```text
provider pending
```

is known nonterminal effect state.

```text
unknown
```

is insufficient observer knowledge.

Do not collapse them.

---

# 140. External Effect facts are temporally indexed

Useful representation:

```text
Effect E
Observation O at time t
State S/version v
Source P
```

because open resources evolve.

---

# 141. Desired-state reconciliation introduces policy liveness

A controller may continue acting as external state drifts.

Whether to keep reconciling is a policy question, not Runtime mechanical execution fact.

---

# 142. Current Host is the natural semantic controller/orchestrator

It can preserve Goal/Task continuity and decide when another Runtime Operation is justified.

Runtime should not independently chase external desired state absent a structured consumer
contract.

---

# 143. Effect observation may require credentials/authority different from action authority

Some systems provide read-only observation credentials separate from write credentials.

This is beneficial separation of:

```text
ObserveAuthority
ActionAuthority
```

---

# 144. Read authority is not semantic authority

An observer can accurately query an order without deciding whether to hold/cancel it.

Again three-authority model remains important.

---

# 145. External provider availability affects reconciliation liveness

If provider query API is down:

```text
Effect may be terminal
but observer remains unable to know
```

So reconciliation liveness depends on provider availability.

---

# 146. Safety and liveness trade off

Refusing blind retry protects against duplicate Effects but can leave work stuck/unknown until
provider evidence returns.

This mirrors RF10 at-most-once design.

---

# 147. Strong external reliability often needs two independent paths

Examples:

```text
write API + read/query API
request response + webhook
provider receipt + audit log
```

This improves reconciliation resilience.

It still does not create global transactionality.

---

# 148. Provider state machine is part of adapter contract

A structured adapter should know legal transitions such as:

```text
live -> partially_filled -> filled
live -> canceled
partially_filled -> canceled
```

rather than accepting arbitrary labels.

---

# 149. State-machine versioning matters

Providers can add/remove states or change API semantics.

Adapter/provider contract identity must account for materially changed semantics.

---

# 150. Current Runtime provider commitment pattern is reusable here

If a future Effect adapter materially depends on external API semantics, adapter/provider
version should be identity-bound rather than assumed stable forever.

---

# 151. External Effect adapter is not necessarily Runtime Core

It may live in Finance/Web/Studio/domain layers and use Runtime for mechanical execution.

The owner should be where domain state machine and receipt semantics are actually understood.

---

# 152. Thin-core conclusion

Runtime Core should continue owning:

```text
Operation/Attempt identity
mechanical dispatch/execution
Artifacts/evidence
structured effects it genuinely owns
```

and not absorb arbitrary domain ontologies prematurely.

---

# 153. Runtime Release is justified because Runtime owns the resource domain

It is self-release: Runtime is the system being deployed and possesses the exact installation
receipt/reconciliation mechanism.

This is qualitatively different from generic cloud/exchange APIs.

---

# 154. Workspace Patch is justified because Runtime owns exact filesystem mutation contract

Again, state equivalence and reconciliation are locally knowable.

---

# 155. Finance adapter should likely remain Finance-owned

Exchange order/fill semantics, venue IDs, trading risk and account reconciliation belong to
Finance/domain truth, even if Runtime executes its process/API calls.

RF15 does not authorize any implementation change; it establishes responsibility logic.

---

# 156. Human Effect adapter is even less suited to generic Runtime

“Human completed task” requires social/contextual semantics far beyond process execution.

Runtime may preserve messages/evidence, not judge human intent or outcome generically.

---

# 157. Web/remote-resource effects vary widely

HTTP alone is insufficient ontology. One endpoint may be:

```text
read
idempotent replace
append event
async job creation
payment
message send
```

Adapter semantics must follow resource/domain contract.

---

# 158. URI is not Effect identity

Repeated requests to one URI can create many resources/events.

Conversely one resource may move/change representations.

Therefore:

```text
EndpointIdentity != EffectIdentity
```

---

# 159. Status code is not Effect identity

`200/202/500` describes response semantics, not the stable external entity/action identity by
itself.

---

# 160. 202-style acceptance illustrates stage separation

Even without relying on one specific API, asynchronous HTTP conventions commonly distinguish
accepted-for-processing from completed processing.

Structured systems should preserve the provider-specific equivalent distinction.

---

# 161. Effect footprint

RF15 defines **Effect Footprint** as the set of externally owned resources/state dimensions
that an action may alter.

Opaque commands can have unknown/unbounded footprints.

---

# 162. OPAQUE means footprint/commit semantics are not generically knowable

It does not mean “no effect information exists anywhere.”

The target/provider may know more; Runtime generic layer does not.

---

# 163. Structured means footprint/state model is explicit enough for stronger claims

Patch and Release are examples.

The threshold is semantic ownership, not merely having JSON output.

---

# 164. Effect authority closure parallels RF7 dependency closure

For a provider Effect, stronger automation requires knowing:

```text
what resource can be changed
under which credential
with which preconditions
how occurrence is identified
how state is observed
```

Unbounded authority makes recovery riskier.

---

# 165. Credential scope is part of Effect blast radius

A write credential that can mutate an entire cloud account creates a larger external Effect
surface than one scoped resource token.

RF13 authority/blast-radius theory extends into open systems.

---

# 166. Least privilege improves external recoverability too

Narrow authority limits the set of plausible unintended Effects after ambiguous execution,
which can make incident reconciliation more tractable.

---

# 167. External transactionality is rare across heterogeneous providers

RF12 already established cross-owner transaction requirements.

RF15 emphasizes that most real external APIs expose request/receipt/reconciliation rather
than prepare/commit participation.

Therefore orchestration/Saga remains the truthful default.

---

# 168. Saga state must preserve provider-local terminal truth

If order partially filled then canceled, compensation logic must see both facts.

Flattening to “failed order” destroys necessary state.

---

# 169. Open-system histories are append-oriented

Later external changes do not erase earlier Effects.

History may be:

```text
created
modified
partially consumed
canceled
compensated
modified again by another actor
```

---

# 170. Current-state snapshots and history are complementary

Current state answers control decisions.

Historical receipts answer audit/causation questions.

A robust domain often needs both.

---

# 171. External Effect proof can be layered

Possible evidence ladder:

```text
local intent only
request bytes created
transport evidence
provider acceptance response
provider operation ID
provider terminal receipt
current resource query
provider audit history
independent downstream observation
semantic/domain confirmation
```

No rung automatically means all above/below facts.

---

# 172. Provider terminal receipt can still be narrower than downstream Reality

A cloud create SUCCESS may not prove application health.

Email mailbox delivery may not prove human read.

Order filled may not prove portfolio objective.

---

# 173. Semantic completion must stay explicit and external

Current Runtime's permanent:

```text
semanticCompletionEvaluated=false
```

for generic job surfaces remains one of its strongest honesty boundaries.

---

# 174. Reconciliation has a stopping rule problem

When should polling stop?

Possible domain answers:

```text
provider terminal state
TTL exceeded
human escalation
receipt retention horizon reached
Goal superseded
```

Runtime cannot choose universally.

---

# 175. Poll frequency is domain/resource-specific

Markets may need subsecond/event-driven updates; cloud operations may poll seconds; human
outcomes may be hours/days.

Therefore generic continuous reconciliation policy belongs above Core.

---

# 176. Freshness requirements are consumer-specific

A 5-minute-old cloud resource state can be acceptable for one report and useless for trading.

So:

```text
FreshEnough(Q) is query/decision-relative.
```

---

# 177. Observation timestamp must not be mistaken for provider transition time

`observedAt` records when we learned a fact.

Provider may expose a separate transition/event time.

RF5 temporal distinctions remain essential.

---

# 178. Callback time is not Effect time

Webhook can be delayed or retried.

Thus:

```text
WebhookReceivedAt != EffectCommittedAt
```

unless provider asserts otherwise.

---

# 179. Poll result time is not necessarily state-transition time

A GET at t2 saying `filled` means filled by t2, not necessarily exactly at t2; provider may
supply fill time separately.

---

# 180. Partial-order causality remains open

Runtime request may cause provider effect, but other actors and asynchronous pipelines create
only partial ordering unless explicit IDs/timestamps/versions link events.

---

# 181. Cross-provider workflow cannot assume global ordering

Email provider event, payment settlement and cloud provisioning may have unrelated clocks and
consistency models.

Host should reason using explicit causal relations, not timestamp sorting alone.

---

# 182. External “success” is a typed word

Always ask:

```text
success of what?
request validation?
provider acceptance?
operation terminality?
resource readiness?
domain objective?
```

Untyped success is dangerous.

---

# 183. External “failure” is equally typed

A provider operation can fail after partial effects.

A business Goal can fail despite successful provider operation.

A transport can fail while provider operation succeeds.

---

# 184. Typed outcome vector

RF15 recommends thinking conceptually as:

```text
Outcome = {
  transport,
  requestAcceptance,
  providerOperation,
  externalResource,
  observationCertainty,
  semanticOutcome
}
```

not one boolean.

---

# 185. Current Runtime already has part of this vector

It exposes:

```text
execution disposition
Runtime delivery disposition
recovery requirement
semantic completion evaluated=false
```

and structured Release adds Effect disposition/receipt.

This is a strong foundation.

---

# 186. A future adapter should extend, not overwrite, these axes

Example finance adapter might add:

```text
providerOrderState
accumulatedFill
providerOrderId
reconciliationFreshness
```

without redefining Runtime execution success.

---

# 187. Domain facts can outlive Runtime artifacts

If provider retains account/order state longer, it can remain authoritative after local log
rotation.

Conversely local receipt may outlive provider query history.

Evidence retention planning matters.

---

# 188. Multiple truth owners are normal

Open systems often require joining:

```text
Runtime mechanical truth
provider resource truth
domain semantic truth
```

No single database should pretend to own all.

---

# 189. Replication of provider facts does not transfer authority

Caching an OKX order state locally creates a replica/projection.

The exchange remains authoritative for venue order/fill truth.

---

# 190. Local projection can be useful despite non-authority

It enables:

```text
history
analytics
fast UI
recovery planning
```

provided freshness/source/version are retained.

---

# 191. Reconciliation updates projection, not past reality

If stale local state says `live` and provider says `filled`, reconciliation updates local
knowledge.

It does not cause the fill retroactively.

---

# 192. External ambiguity should block unsafe automation, not all progress

A workflow can sometimes continue on independent branches while one Effect remains unknown.

Host orchestration can model this; Runtime need not global-lock all work.

---

# 193. Risk sensitivity matters

Duplicate email may be annoying.

Duplicate payment/order may be financially severe.

Thus retry/reconciliation policy must account for Effect consequence, not only technical
possibility.

---

# 194. Cost of false positive versus false negative differs by domain

Examples:

```text
assume order absent when it exists → duplicate position
assume order exists when absent → missed trade
```

Policy should be domain-owned.

---

# 195. External effects need consequence-aware recovery classes

A future domain layer may classify:

```text
safe to retry
query first
manual review
compensate
wait for terminal provider state
```

Generic Runtime should not infer them from process codes.

---

# 196. Human escalation is a legitimate reconciliation outcome

If machine evidence cannot decide a high-consequence Effect, escalation may be the correct
terminal control action.

That is not Runtime failure; it is honest epistemic governance.

---

# 197. Open systems make “exactly once” especially narrow

Even if provider guarantees one resource creation, downstream human/business consequences can
occur many times or not at all.

Exactly-once should remain scoped to the named Effect object.

---

# 198. Effect duplication and observation duplication are distinct

Two webhook deliveries can correspond to one Effect.

Two order fills can be two legitimate subeffects of one order.

Two POST requests can map to one provider object under idempotency.

Count the right entity.

---

# 199. Effect completion and effect settlement can differ

Financial/payment domains may have later settlement/reversal/chargeback phases after initial
completion.

A structured contract should avoid claiming permanent finality beyond provider semantics.

---

# 200. External finality is domain-relative

Blockchains, card networks, markets, SMTP, cloud resources and human actions have different
notions of finality.

Runtime should never expose one generic “final external world state” concept.

---

# 201. RF15 anti-laws

RF15-L1: `ExternalEffect != LocalFunctionCall`.

RF15-L2: `Intent != RequestSent`.

RF15-L3: `BytesSent != ProviderAccepted`.

RF15-L4: `NoResponse != ProviderRejected`.

RF15-L5: `ClientUnknown != ProviderUncommitted`.

RF15-L6: `CallerIdempotencyKey != IdempotentEffectWithoutProviderBinding`.

RF15-L7: `RuntimeHistoricalIdentity != ProviderDeduplicationLifetime`.

RF15-L8: `RequestAccepted != ExternalEffectComplete`.

RF15-L9: `ProviderOperationId != ExternalResourceId`.

RF15-L10: `ResourceIdentityKnown != ResourceReady`.

RF15-L11: `LocalProcessFinished != ExternalEffectTerminal`.

RF15-L12: `RuntimeDeliveryCommitted != ExternalEffectCommitted`.

RF15-L13: `ExitZero != ExternalEffectTerminalSuccess`.

RF15-L14: `ExitNonZero != NoExternalEffect`.

RF15-L15: `AcceptanceReceipt != CompletionReceipt`.

RF15-L16: `ProviderReceipt != DomainGoalCompletion`.

RF15-L17: `CorrectAt(t1) != CurrentAt(t2)`.

RF15-L18: `PostWriteReadMissing != EffectDefinitelyAbsent`.

RF15-L19: `Reconcile != Retry`.

RF15-L20: `HeuristicSearch != IdentityBoundReconciliation`.

RF15-L21: `ForeignReference != ExternalEffectProof`.

RF15-L22: `CurrentState != CompleteCausalHistory`.

RF15-L23: `ProviderAcceptance != DomainTransitionResult`.

RF15-L24: `OrderCanceled != NoTradeOccurred`.

RF15-L25: `CancelRequested != RemainingQuantityDefinitelyUnfilled`.

RF15-L26: `OrderFilled != TradingGoalSucceeded`.

RF15-L27: `HTTPStatusClass != ExternalEffectOccurrence`.

RF15-L28: `Created != Ready`.

RF15-L29: `SMTPDelivered != HumanRead`.

RF15-L30: `Acknowledged != GoalCompleted`.

RF15-L31: `ResponsibilityAccepted != OutcomeSucceeded`.

RF15-L32: `EffectCommittedAt(t1) != ResourceStillEqualAt(t2)`.

RF15-L33: `WebhookDeliveryIdempotency != UnderlyingEffectIdempotency`.

RF15-L34: `NotificationLoss != EffectLoss`.

RF15-L35: `NotificationDuplication != EffectDuplication`.

RF15-L36: `OperationTerminal != ResourceImmutable`.

RF15-L37: `EffectResourceIdentity != StateFingerprint`.

RF15-L38: `EndpointIdentity != EffectIdentity`.

RF15-L39: `WebhookReceivedAt != EffectCommittedAt`.

RF15-L40: `ExternalSuccess != UntypedBooleanSuccess`.

---

# 202. Minimum RF15 grammar

## 202.1 External owner `P`

Provider/domain that authoritatively owns Effect/resource state.

## 202.2 Effect `E`

Externally visible change/obligation/state transition under owner P.

## 202.3 Request `Req`

Concrete action message directed toward P.

## 202.4 Provider operation `Op_P`

Provider-owned asynchronous/synchronous operation identity for Req.

## 202.5 Resource `R_P`

Provider-owned resource/entity affected by E.

## 202.6 Receipt `Rcpt_P(F)`

Provider-authored evidence for fact/frontier F.

## 202.7 Observation `Obs_P(R,S,t,v)`

Provider observation of resource/state S at time t/version v.

## 202.8 Reconciliation `Recon(L,Obs,Rcpt)`

Evidence-based convergence of local knowledge L with provider truth without blind repeat.

## 202.9 Idempotency horizon `H_idem`

Provider guarantee interval for repeated key→same intended Effect/result semantics.

## 202.10 Evidence retention horizon `H_evidence`

Interval over which authoritative historical Effect evidence remains queryable/provable.

## 202.11 Responsibility transfer frontier `RTF`

Point where downstream owner accepts contractual responsibility for continued processing.

## 202.12 Semantic outcome `Sem_Q`

Domain judgment whether Effect/history satisfies Goal/question Q.

---

# 203. Current Runtime open-effect map

| Current mechanism | RF15 interpretation |
|---|---|
| `workspace.exec` | effect-opaque mechanical execution |
| Job/Attempt result | Runtime-owned process/execution truth |
| `deliveryDisposition=committed` | conclusive Runtime physical result committed, not external effect completion |
| `semanticCompletionEvaluated=false` | explicit refusal to judge domain outcome |
| `foreignReferences` | cross-owner correlation/lineage, not foreign fact proof |
| stdout/stderr Artifact | preserved evidence bytes, source authority remains external/target |
| Release effect ID | structured Runtime-owned release Effect identity |
| deployment receipt | release-domain authoritative commit/rollback evidence |
| `release.get` | read-only structured Effect projection/reconciliation |
| Workspace Patch | structured local filesystem Effect contract |
| patch before/after digests | owner-defined physical equivalence relation |
| `workspace.patch.get` | local effect reconciliation without blind repeat |
| `commitState` | Runtime operation commitment only |
| `Lost`/`Unknown` | local/epistemic ambiguity, not proof of no external effect |
| Host | semantic orchestration/completion owner above mechanical Runtime |
| domain/provider adapters | correct future home for external state machines/receipts when justified |

---

# 204. What RF15 strongly supports in current design

## 204.1 Keep generic exec OPAQUE

Runtime cannot infer arbitrary external provider commit/finality from process status/output.

## 204.2 Preserve `semanticCompletionEvaluated=false`

This is the correct boundary between mechanical execution and open-world Goal truth.

## 204.3 Keep Runtime delivery disposition local/mechanical

Do not rename or reinterpret it as domain delivery/effect success.

## 204.4 Require structured provider/domain contracts for stronger Effect semantics

Identity, receipts, state machine, idempotency horizon and reconciliation must come from an
actual owner.

## 204.5 Keep Release/Patch domain-specific

They validate the structured-Effect pattern but do not justify a universal Effect kernel.

## 204.6 Prefer reconciliation over blind retry after ambiguity

Especially for financially or externally consequential Effects.

---

# 205. What RF15 reopens

## 205.1 A reusable Effect-contract vocabulary may be valuable above Core

Common dimensions such as owner/effect ID/receipt/query/freshness can be shared as concepts,
while domain state machines remain specific.

No generic implementation is authorized.

## 205.2 Provider evidence-retention horizons should become explicit in high-risk domains

Finance/payment/cloud adapters need to know how long exact reconciliation remains possible.

## 205.3 Freshness/version should accompany replicated external projections

Especially where Host/domain decisions depend on current provider state.

## 205.4 Action versus observation credentials are useful separation

High-consequence domains may benefit from strong read/query paths independent from write
authority.

---

# 206. Research constitution added by RF15

1. Treat external Reality as independently mutable unless a provider contract proves
   otherwise.
2. Separate Intent, Request, transport, Provider Acceptance, provider operation, resource
   transition, observation/receipt, reconciliation and Semantic Outcome.
3. Never infer provider rejection/non-occurrence from missing response alone.
4. Treat client knowledge state and external Reality state as separate axes.
5. Require provider cooperation for external idempotency, exactly-once, preconditions and
   authoritative reconciliation.
6. Treat idempotency retention/horizon as part of the guarantee.
7. Distinguish provider operation identity, resource identity, client key and Runtime
   Operation identity.
8. Scope receipts to the exact frontier/fact they prove.
9. Distinguish acceptance receipts from terminal/completion receipts.
10. Add freshness/version semantics to external observations where decisions depend on
    currency.
11. Treat eventually consistent absence as insufficient non-occurrence evidence unless the
    provider contract guarantees otherwise.
12. Reconcile before retry when an Effect may already have committed.
13. Prefer exact provider identity lookup over heuristic searches.
14. Preserve partial effects such as fills rather than flattening them into binary failure.
15. Treat cancel/compensation as new external actions that can race with other changes.
16. Do not infer business/Goal success from provider Effect success.
17. Treat current state and historical causal evidence as complementary.
18. Keep Runtime `deliveryDisposition` scoped to physical execution-result certainty.
19. Preserve `semanticCompletionEvaluated=false` for generic Runtime surfaces.
20. Keep generic execution effect-opaque.
21. Allow structured Effect semantics only where an owner contract actually provides
    identity/state/receipt/reconciliation.
22. Do not infer a universal Effect kernel from Release/Patch shared vocabulary.
23. Treat webhooks/callbacks as observations/event deliveries, not the Effect itself.
24. Distinguish notification idempotency from underlying Effect idempotency.
25. Treat provider retention as a hard upper bound on later query-based recovery unless
    authoritative receipts were durably captured elsewhere.
26. Separate Action Authority, Observation Authority and Semantic Authority.
27. Keep provider/domain state machines in the layer that understands their ontology.
28. Use typed multi-axis outcomes instead of a generic external success boolean.
29. Accept permanent epistemic unknown when authoritative historical evidence no longer
    exists.
30. Governing law: `ExternalEffectClaimScope <= ProviderOwnedEvidenceAndControlScope`.
31. Do not enter RF16 until the complete RF0-RF15 model can be attacked across local,
    distributed, adversarial, nondeterministic and open-system cases without collapsing these
    distinctions.

---

# 207. RF15 result — conceptual compression

```text
Runtime can cause a process to issue a request.
That does not mean the provider received it.
Provider receipt/acceptance does not mean the external operation is terminal.
External operation terminality does not mean a mutable resource will stay that way.
Provider technical success does not mean the user's Goal succeeded.

Open-system reliability therefore depends on:
  identity
  owner-specific state machines
  receipts/queries
  freshness/version
  idempotency/retention
  reconciliation
  explicit unknowns
rather than one generic success/failure bit.
```

Strongest Runtime-specific compression:

```text
Generic workspace.exec:
  Runtime mechanical truth only
  external Effect = OPAQUE
  semantic completion = outside Runtime

Structured Effect:
  only stronger when Runtime/domain/provider truly owns
  Effect identity + receipt/query + state/reconciliation contract

Current examples:
  Runtime Release
  Workspace Patch

External domains:
  Finance / cloud / email / human
  need their own Effect ontology and truth owners
```

Central theorem:

```text
ExternalEffectClaimScope <= ProviderOwnedEvidenceAndControlScope
```

---

# 208. RF15 closeout

RF15 closes with thirty-two durable results:

1. **Open systems contain independently changing actors/state outside Runtime authority.**
2. An External Effect is an externally owned visible change/obligation/state transition, not
   merely a local function return.
3. Intent, Request, transport, Provider Acceptance, provider operation, external transition,
   observation/receipt, reconciliation and Semantic Outcome are distinct stages.
4. Missing response cannot prove provider rejection or non-occurrence.
5. Client knowledge and provider Reality can differ; `committed reality + unknown caller`
   is a normal failure state.
6. External idempotency requires provider-side durable binding/equivalence and a retention
   horizon; a caller key alone is not enough.
7. Runtime replay lifetime and provider deduplication/evidence lifetime are distinct.
8. Asynchronous providers separate Request Acceptance from terminal Effect completion through
   provider operation identities/state machines.
9. Provider operation identity, external resource identity, client-assigned effect key and
   Runtime Operation identity are separate namespaces.
10. Resource identity/readiness/current state are distinct facts.
11. **Current generic Runtime correctly keeps local execution terminality separate from
    arbitrary external Effect terminality.**
12. `deliveryDisposition=committed` is Runtime physical-result certainty and must not be
    inflated into external/domain success.
13. Exit zero/nonzero cannot generically prove external Effect success/absence.
14. A receipt must be scoped to the exact provider frontier/fact it proves; acceptance and
    completion receipts are distinct.
15. Provider receipts remain narrower than Semantic Goal completion.
16. External observations require source/authority plus freshness/version; a correct old
    observation can be stale now.
17. Eventual consistency means post-write absence may not prove effect absence.
18. Reconciliation is evidence-based convergence of local knowledge with provider truth and
    is distinct from retry.
19. Identity-bound query is stronger than heuristic search for recovery.
20. Current state does not reconstruct complete causal history when other actors can mutate
    the resource.
21. **Finance order state demonstrates first-class partial Effects: `partially_filled` and
    canceled-with-fills cannot be flattened into no-effect failure.**
22. Cancel/compensation are new Effects that may race existing external activity.
23. Cloud operations demonstrate accepted/in-progress/success/failure/cancel state machines
    that outlive the initiating local request.
24. SMTP demonstrates responsibility acceptance, technical delivery and human semantic
    outcome as separate frontiers.
25. Action Authority, Observation Authority and Semantic Authority may belong to different
    owners.
26. Webhook/callback delivery is an observation channel; duplication/loss of notification is
    distinct from duplication/loss of underlying Effect.
27. Provider version/generation/event IDs can strengthen ordering, conditional mutation and
    reconciliation but remain provider-scoped.
28. **Release and Workspace Patch justify owner-specific structured Effect contracts, not a
    generic Runtime Effect kernel.**
29. Generic `workspace.exec` must remain effect-opaque because Runtime does not own arbitrary
    provider state machines, IDs, receipts or reconciliation semantics.
30. Host/domain remains the natural owner of semantic convergence and policy decisions across
    multiple Runtime Operations/provider observations.
31. Permanent `unknown` can be an honest terminal epistemic state when authoritative evidence
    has expired or never existed.
32. **The governing RF15 law is
    `ExternalEffectClaimScope <= ProviderOwnedEvidenceAndControlScope`.**

The next frontier is:

> **Now attack the complete RF0-RF15 Runtime Foundations as one system. Which distinctions
> survive cross-domain falsification, which assumptions conflict, which current Runtime
> structures are contingent versus foundational, and what should actually be frozen as
> Runtime Foundations v1?**

That is RF16 — Cross-Domain Falsification and Runtime Foundations v1 Reconstruction.
