---
schema_version: 1
id: runtime.foundations.rf15.continuation
title: RF15 Continuation — Enter RF16
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
summary: Cross-conversation checkpoint after RF15, preserving the open-system Effect model and exact RF16 frontier for cross-domain falsification and Runtime Foundations v1 reconstruction.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf15
  - runtime.foundations.rf15.sources
---
# RF15 Continuation — Enter RF16

## Completed

RF15 — External World and Open-System Effects.

Primary record:

`research/foundations/RF15-EXTERNAL-WORLD-OPEN-SYSTEM-EFFECTS.md`

Source map:

`research/foundations/RF15-SOURCES.md`

## Durable results to carry forward

1. Open systems contain independently changing actors/state outside Runtime authority.
2. External Effect means externally owned visible change/obligation/state transition, not a
   local function return.
3. Intent, Request, transport, Provider Acceptance, provider operation, external transition,
   observation/receipt, reconciliation and Semantic Outcome are distinct stages.
4. Missing response does not prove provider rejection/non-occurrence.
5. Caller knowledge and provider Reality are independent axes; committed provider state can
   coexist with caller `unknown`.
6. External idempotency requires Effect-owner durable binding/equivalence and retention; a
   caller key alone is insufficient.
7. Runtime historical replay lifetime and provider deduplication/evidence lifetime differ.
8. Asynchronous providers expose provider operation identities/state machines that outlive the
   initiating request.
9. Provider operation ID, external resource ID, client Effect key and Runtime Operation ID are
   distinct namespaces.
10. Resource identity/readiness/current state are distinct.
11. Generic Runtime execution terminality remains separate from arbitrary external Effect
    terminality.
12. `deliveryDisposition=committed` is Runtime physical-result certainty, not external/domain
    completion.
13. Process exit zero/nonzero cannot generically prove external Effect success/absence.
14. Receipts must be scoped to exact frontiers; acceptance and terminal/completion receipts
    are distinct.
15. Provider receipt remains narrower than Semantic Goal completion.
16. External observations require authority + freshness/version; correct historical truth can
    be stale now.
17. Eventual consistency can make post-write absence insufficient evidence of non-occurrence.
18. Reconciliation is evidence-based convergence with provider truth and is distinct from
    retry.
19. Identity-bound provider query is stronger than heuristic search for recovery.
20. Current external resource state does not reconstruct complete causal history when other
    actors can mutate it.
21. Finance order state demonstrates first-class partial Effects: partial fills and
    canceled-with-fills are economically real committed subeffects.
22. Cancel and compensation are new external actions that can race existing effects.
23. Cloud providers demonstrate accepted/in-progress/success/failure/cancel state machines
    separate from initiating network-call completion.
24. SMTP demonstrates responsibility acceptance, technical delivery and human semantic outcome
    as distinct frontiers.
25. Action Authority, Observation Authority and Semantic Authority may belong to different
    owners.
26. Webhook/callback delivery is an observation/event channel; notification duplication/loss
    differs from underlying Effect duplication/loss.
27. Provider versions/generations/event IDs strengthen ordering, preconditions and
    reconciliation but remain provider-scoped.
28. Release and Workspace Patch justify owner-specific structured Effect contracts rather than
    a universal Runtime Effect kernel.
29. Generic `workspace.exec` must remain effect-opaque because Runtime does not own arbitrary
    provider state machines/IDs/receipts/reconciliation semantics.
30. Host/domain remains semantic convergence/policy owner across Runtime Operations and
    provider observations.
31. Permanent epistemic `unknown` is legitimate when authoritative historical evidence has
    expired or never existed.
32. Governing law: `ExternalEffectClaimScope <= ProviderOwnedEvidenceAndControlScope`.

## RF15 compact grammar

```text
P                          external Effect owner/provider
E                          external Effect
Req                        request toward P
Op_P                       provider operation identity
R_P                        provider resource identity
Rcpt_P(F)                  provider receipt proving scoped frontier F
Obs_P(R,S,t,v)             provider observation with state/time/version
Recon(L,Obs,Rcpt)          evidence-based local/provider convergence
H_idem                     provider idempotency horizon
H_evidence                 provider evidence retention horizon
RTF                        responsibility transfer frontier
Sem_Q                      semantic Goal outcome under question Q
```

## Current implementation interpretation

```text
workspace.exec               mechanical OPAQUE execution
Job/Attempt Result            Runtime truth only
Runtime delivery committed   physical-result certainty only
semanticCompletion=false     explicit domain boundary
foreignReferences            correlation, not proof
Release Effect ID            structured Runtime-owned effect identity
release receipt              release-domain evidence
release.get                  read-only effect projection/reconciliation
Workspace Patch              structured local filesystem effect
patch.get                    before/after state reconciliation
Lost/Unknown                 uncertainty, never generic no-effect proof
Host                          semantic orchestration/convergence owner
```

## RF16 exact frontier

Enter:

# RF16 — Cross-Domain Falsification and Runtime Foundations v1 Reconstruction

Goal:

Attack RF0-RF15 as one theory rather than continuing to add concepts. Determine which laws are
foundational, which are contingent implementation choices, which conflict, and what minimal
Runtime Foundations v1 should actually freeze.

Mandatory cross-domain attacks:

```text
pure deterministic local computation
nondeterministic multithreaded computation
immutable input + mutable host dependency
trusted_local same-authority namespace attack
contained_local hostile kernel assumption
Windows limited/elevated execution
response-loss after external POST commit
finance partial fill + cancel race
async cloud create + stale read
SMTP accepted + later bounce + human nonresponse
multi-step execPlan with partial external effects
Runtime Release self-replacement during response loss
Workspace Patch mixed filesystem state
Registry COMMIT ambiguity
Host semantic Goal continuing after Runtime terminal execution
provider idempotency retention expires before local replay retention
multiple truth owners disagree due freshness/namespace scope
```

Questions:

```text
What is the smallest root ontology that explains all rounds?
Is Operation still the correct Runtime root entity?
Which entities are intrinsic: Operation, Attempt, Effect, Evidence, Authority, Resource?
Which are projections/implementation artifacts: Job, Workspace, cgroup/unit, receipt tables?
What must Runtime own versus merely correlate?
Which scope laws can be unified into a deeper theorem?
Where is current Runtime overfit to local/systemd/Git assumptions?
Where did engineering intuition already discover sound foundational principles before theory?
Which research conclusions should change implementation now, later, or never?
What explicit FoundationReopenConditions should v1 declare?
```

Do not proceed to broad Host/Harness unification until RF16 freezes Runtime Foundations v1 or
finds a concrete contradiction requiring a prior RF round to reopen.
