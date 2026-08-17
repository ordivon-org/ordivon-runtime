---
schema_version: 1
id: runtime.foundations.rf15.sources
title: RF15 Sources — External World and Open-System Effects
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
summary: Primary/canonical sources used by RF15 for HTTP idempotency, provider-side idempotency retention, asynchronous cloud operations, exchange order partiality, SMTP responsibility/delivery semantics and current Runtime effect contracts.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf15
---
# RF15 Sources — External World and Open-System Effects

## S1 — HTTP semantics and idempotency

IETF RFC 9110, **HTTP Semantics**.

Canonical source: `https://datatracker.ietf.org/doc/rfc9110/`.

RF15 use:

- HTTP idempotency is defined in terms of the intended effect of repeated identical requests;
- idempotent requests can be retried after communication failure because the intended effect
  remains equivalent even if the original request succeeded;
- clients should not blindly retry non-idempotent methods without stronger knowledge;
- supports `NoResponse != ProviderRejected`, effect-scoped idempotency and the distinction
  between transport behavior and resource-specific semantics.

## S2 — Stripe provider-side idempotency

Stripe official API documentation, **Idempotent requests**.

Canonical source: `https://docs.stripe.com/api/idempotent_requests`.

RF15 use:

- provider-side idempotency stores the first result under an idempotency key and returns it to
  exact retries;
- request parameters are compared for key misuse;
- execution-start/validation distinctions affect whether an idempotent result is stored;
- keys may be pruned after a documented retention horizon, after which reuse can create a new
  request;
- supports `CallerKey != ProviderIdempotencyWithoutBinding` and the concept of Idempotency
  Horizon.

## S3 — AWS Cloud Control asynchronous operation model

AWS official Cloud Control API documentation, **ProgressEvent** and **Managing resource
operation requests**.

Canonical sources:

- `https://docs.aws.amazon.com/cloudcontrolapi/latest/APIReference/API_ProgressEvent.html`
- `https://docs.aws.amazon.com/cloudcontrolapi/latest/userguide/resource-operations-manage-requests.html`
- `https://docs.aws.amazon.com/cloudcontrolapi/latest/userguide/getting-started.html`

RF15 use:

- create/update/delete requests expose a provider-owned `RequestToken`;
- operation states include `PENDING`, `IN_PROGRESS`, `SUCCESS`, `FAILED`,
  `CANCEL_IN_PROGRESS` and `CANCEL_COMPLETE`;
- callers query operation status after initial request completion;
- resource identifier can exist before terminal operation success;
- operation request history itself has a retention horizon;
- supports `RequestAccepted != ExternalEffectComplete`, `ProviderOperationId != ResourceId`
  and asynchronous cancellation/completion semantics.

## S4 — OKX order state and partial effects

OKX official API guide.

Canonical source: `https://www.okx.com/docs-v5/en/`.

RF15 use:

- order states include `live`, `partially_filled`, `filled`, `canceled` and
  `mmp_canceled`;
- accumulated fills can remain non-zero on a terminal canceled order, including IOC partial
  fill then cancel;
- amend request `sCode=0` means the request was accepted by the system server, while actual
  amend result is determined by order-channel push/status query;
- client/provider order identities and trade/fill IDs support effect-specific reconciliation;
- provider history windows constrain later exact reconciliation;
- supports `ProviderAcceptance != DomainTransitionResult`, `OrderCanceled != NoTradeOccurred`
  and partial-effect ontology.

## S5 — SMTP delivery-status semantics

IETF RFC 3461, **SMTP Service Extension for Delivery Status Notifications**, and RFC 3464,
**An Extensible Message Format for Delivery Status Notifications**.

Canonical sources:

- `https://datatracker.ietf.org/doc/rfc3461/`
- `https://datatracker.ietf.org/doc/rfc3464/`

RF15 use:

- positive SMTP acceptance transfers responsibility for delivery/relay or failure reporting;
- delivery status notifications can distinguish success, failure and delay;
- technical local delivery means placement into the recipient mailbox/service rather than
  human retrieval/read/comprehension;
- supports `ResponsibilityAccepted != OutcomeSucceeded`, `SMTPDelivered != HumanRead` and
  multi-frontier communication Effects.

## S6 — Current Ordivon Runtime as empirical source

### Generic execution

Current `workspace.exec` is effect-opaque. Runtime owns Operation/Attempt/process/result truth
and projects `semanticCompletionEvaluated=false`. `RuntimeDeliveryDisposition::Committed`
means Runtime has committed a conclusive physical execution result; it is explicitly not a
Task/domain semantic-completion judgment or arbitrary external-effect receipt.

### Runtime ambiguity

`Lost`, `Orphaned`, `Unknown` and `ReconciliationRequired` preserve mechanical/epistemic
ambiguity after process/provider boundary failures. Current Runtime refuses speculative
redispatch of opaque work after ambiguous physical dispatch.

### Foreign references

`ForeignReference` values preserve cross-owner namespace/type/id/generation/digest correlation
in Operation/evidence. They do not transfer truth ownership or prove the referenced foreign
resource/effect state.

### Runtime Release

Runtime Release is a structured reconcilable Effect with deterministic release Effect identity,
operator-configured authority, deployment receipt, rollback status and `release.get` projection.
Current Tool contract states the deployment receipt, not generic process exit, is authoritative
for release effect commitment.

### Workspace Patch

Workspace Patch is a distinct structured local Effect with durable before/after plans,
prepared/committed/unknown reconciliation and `workspace.patch.get`. Its effect-state semantics
are possible because Runtime owns the exact filesystem equivalence relation for that adapter.

### Semantic boundary

Runtime inspection, task list/observe and Release projection continue to expose
`semanticCompletionEvaluated=false`; semantic Task/domain completion remains outside Runtime.

## Source discipline

Later rounds should preserve:

```text
intent != request
request/transport != provider acceptance
provider acceptance != effect completion
provider operation identity != resource identity
receipt must name its frontier
acceptance receipt != completion receipt
external observation needs freshness/version
reconciliation != retry
partial effect is first-class
cancel/compensation are new effects
current resource state != complete causal history
Runtime delivery committed != external effect committed
foreign reference != foreign fact proof
provider idempotency requires owner binding + retention horizon
webhook notification != underlying effect
provider technical success != semantic goal success
generic exec remains OPAQUE
Release/Patch remain domain-specific structured Effects
ExternalEffectClaimScope <= ProviderOwnedEvidenceAndControlScope
```
