---
schema_version: 1
id: runtime.foundations.rf10.continuation
title: RF10 Continuation — Enter RF11
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
summary: Cross-conversation continuation checkpoint after RF10, preserving failure/recovery/idempotency/exactly-once distinctions and the exact RF11 frontier on Observation, Evidence, Proof and Truth Surfaces.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf10
  - runtime.foundations.rf10.sources
---
# RF10 Continuation — Enter RF11

## Completed

RF10 — Failure, Recovery, Retry, Idempotency and Exactly-Once.

Primary record:

`research/foundations/RF10-FAILURE-RECOVERY-RETRY-IDEMPOTENCY-EXACTLY-ONCE.md`

Source map:

`research/foundations/RF10-SOURCES.md`

## Durable results to carry forward

1. `Failure != Unknown/Ambiguous`; timeout/disconnect does not prove an Effect did not occur.
2. Delivery/transport outcome, Runtime execution outcome, external Effect outcome and
   semantic/Goal outcome are orthogonal axes.
3. Recovery, Reconciliation, Retry, Replay/Reattachment, Resume and Compensation are
   distinct operations.
4. Current exact request replay is durable reattachment to the same Job, not execution retry.
5. `(principal, clientRequestId)` plus request identity provides durable admission
   idempotency; changed meaning under the same key fails `IDEMPOTENCY_CONFLICT`.
6. Response-loss after admission should recover through same-identity reattachment rather
   than a new key/new Operation.
7. Idempotency is effect-equivalence under a named contract; it is not read-only or
   side-effect-free.
8. `RequestIdempotency != ExternalEffectIdempotency`.
9. Current Attempt dispatch is at-most-once: dispatch intent is committed before provider
   crossing and ambiguous dispatch is never speculatively repeated.
10. At-most-once permits zero realizations; current Runtime can therefore converge to Lost/
    ambiguity instead of guaranteeing physical occurrence.
11. Current arbitrary execution is neither at-least-once nor exactly-once physical/effect
    execution; it is effect-opaque.
12. Process exit 0/!=0 cannot prove external Effect occurrence/absence or semantic
    completion.
13. `Lost` does not mean definitely never ran; `Orphaned` represents unresolved ownership/
    identity continuity and conservatively holds capacity.
14. Strong late identity-bound evidence may supersede prior conservative uncertainty while
    append-oriented history preserves both observations.
15. Reconciliation converges Registry/provider/process/effect-receipt truth and does not
    retry ambiguous opaque work.
16. Doctor/repair correctly refuses to guess when evidence is missing and requires audited
    explicit repair/finalization.
17. At-most-once, at-least-once and exactly-once must be scoped to a specific event/effect,
    owner, identity and failure model.
18. Reliable transport alone cannot produce exactly-once external Effect semantics.
19. Strong exactly-once-like semantics become possible when one owner/domain can bind stable
    identity, deduplication, transaction/preconditioned commit, receipts/state query and
    retention.
20. Kafka is a useful competing model: idempotent/transactional processing is strong within
    Kafka's integrated log/offset/state transaction domain but does not automatically cover
    unrelated external APIs.
21. Runtime Release is a structured RECONCILABLE Effect; exact replay reattaches and
    `release.get` reconciles canonical deployment receipt without redispatch.
22. Workspace Patch is a separate structured Effect with prepared/committed/unknown state;
    mixed physical state remains unknown rather than being blindly repeated.
23. Two structured adapters prove a common Effect-contract vocabulary but do not justify a
    generic EffectAdapter or caller-declared effect class.
24. Compensation is a new domain Effect rather than rollback; checkpoint Resume is distinct
    from retry-from-start.
25. Current generic Runtime reliability shape is durable idempotent admission + at-most-once
    dispatch + evidence-first reconciliation + explicit ambiguity, not generic exactly-once.

## RF10 compact grammar

```text
Fail(Q,evidence)              named contract Q proven unmet
Unknown(Q)                    evidence cannot distinguish material outcomes
Recover(S,evidence)           restore/converge valid explainable state
Recon(model,owners)           compare truth owners and converge
Retry(A)                      another presentation/attempt of A
Replay(id)                    reattach/reconstruct existing committed history
Resume(checkpoint)            continue same realization from saved state
Compensate(effect)            new offsetting/inverse domain Effect
Idem(A,≈E)                    repeated A has one-equivalent intended effect
AtMostOnce(A)                 count(A) <= 1
AtLeastOnce(A)                count(A) >= 1 under stated liveness assumptions
ExactlyOnce(A)                count(A) == 1 under stated domain/failure model
Receipt                       identity-bound evidence from execution/effect owner
```

## Current implementation interpretation

```text
clientRequestId + digest        admission idempotency identity
exact replay                    same-Job reattachment
IDEMPOTENCY_CONFLICT            same key/different meaning protection
Accepted                        admitted pre-dispatch
Starting / dispatch_issued      durable at-most-once dispatch frontier
Running                         local realization identity
Runner Result                   local execution terminal evidence
Lost                            explicit uncertain/adverse Runtime conclusion
Orphaned                        unresolved ownership/identity; capacity held
reconciliation                  evidence convergence without retry
Doctor/repair                   explicit evidence-gated repair path
CONCURRENCY_LIMIT/not_started   safe re-admission class
workspace.exec                  OPAQUE external Effect contract
Runtime Release                 structured reconcilable Effect + receipt
Workspace Patch                 structured prepared/committed/unknown Effect
semanticCompletionEvaluated=false explicit semantic boundary
```

## RF11 exact frontier

Enter:

# RF11 — Observation, Evidence, Proof and Truth Surfaces

Core questions:

```text
What is an observation versus evidence versus proof versus state versus truth?
Who owns a fact and under what scope/lifetime?
What makes evidence identity-bound, authoritative, direct, derived, corroborating or weak?
How should independent truth sources conflict or supersede one another?
What is a receipt and when is it stronger than process state?
What is negative evidence: process absent, file absent, timeout, no event?
When does absence prove non-occurrence and when is it merely unobserved?
What is confidence versus correctness versus completeness?
What is a projection and when does projection truncation change truth claims?
How should Runtime expose evidence without constructing a second global truth database?
What is forensic evidence versus operational control evidence?
How do timestamps, digests, signatures, row versions and generations establish different
kinds of proof?
```

Mandatory falsifiers:

```text
exit code 0 but semantic task failed
receipt says Effect committed while launching process state is lost
inotify saw no change but target mount namespace consumed different bytes
process absent under kill-on-job-close versus process absent on ordinary Linux
truncated holder list mistaken for complete set
stale snapshot plus valid digest
log line without identity binding
Runner Result with wrong invocation ID
same bytes with wrong provenance
late stronger evidence superseding orphan conclusion
absence of result file before timeout versus after guaranteed atomic rename protocol
conflicting Registry and external provider receipt
```

Do not enter RF12 Composition/Transactions across Operations until RF11 establishes how
truth/evidence from independent owners may be combined without pretending one observation
surface is globally authoritative.
