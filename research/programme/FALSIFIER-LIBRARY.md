# Runtime Falsifier Library v1

These are reusable attacks against Runtime designs and operational claims. They do not create new Foundations.

## F1 — Duplicate-effect falsifier

Create response loss or ambiguous completion after an external effect may already have occurred. A safe design must not infer `timeout => no effect`.

## F2 — Lost-acknowledgement falsifier

Allow realization/effect completion while acknowledgement or local persistence is lost. Recovery must distinguish effect knowledge from transport/result delivery.

## F3 — Stale-authority falsifier

Let an old lease/generation/credential remain physically usable after authority has been superseded. Physical possibility must not imply current operational standing.

## F4 — Local/global lifting falsifier

Produce a local success signal while the wider operation remains unresolved. The design must not promote local completion into global completion without a supported lifting contract.

## F5 — Evidence/truth lifting falsifier

Provide strong evidence for a bounded claim while the associated world-level proposition is false or unresolved. Evidence authenticity must not manufacture broader truth.

## F6 — Restore/recovery falsifier

Restore local durable state after the external world has independently changed. Correct recovery requires reconciliation/re-grounding, not local restoration alone.

## F7 — Historical/currentness falsifier

Preserve a historically valid claim after a later generation, authority or semantic owner supersedes it. History must remain visible while current standing changes.

## F8 — Identity-collapse falsifier

Retry or fan out one Operation across multiple Attempts/realizations. Any model that requires one shared identity for request, Operation, Attempt and realization fails.

## F9 — Weak-composition-upgrade falsifier

Compose mechanisms with individually weak guarantees and test whether the system silently claims a stronger end-to-end property that was never established.

## F10 — Unsafe-reuse falsifier

Reuse historical/cached/shared realization after its semantic binding, evidence freshness or validity domain has changed. Fresh-execution non-necessity applies only when current support is sufficient and standing.

## Required result vocabulary

Each falsifier run should end as one of:

- theory confirmed;
- theory scope narrowed;
- engineering bug found;
- owner-boundary conflict found;
- theory falsified / reopen candidate;
- inconclusive.
