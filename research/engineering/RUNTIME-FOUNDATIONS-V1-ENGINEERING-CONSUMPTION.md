---
schema_version: 1
id: runtime.foundations.v1.engineering-consumption
status: active
date: 2026-08-17
foundation_baseline: Runtime Foundations v1 (RF0-RF16 frozen)
implementation_baseline: c6e45d9e41d3b4d64b5b3dace01497c53e574026
---

# Runtime Foundations v1 -> Engineering Consumption

## Purpose

This is the first engineering-consumption pass after Runtime Foundations RF0-RF16 were frozen as v1.

It does **not** reopen the foundations research and does **not** treat a conceptual distinction as an automatic refactor request. The engineering question is narrower:

> Where does the current Runtime implementation already realize the frozen foundations, where does it merely expose them poorly, where is complexity contingent on real compatibility/recovery consumers, where is there an actual semantic mismatch, and which research concepts have no present consumer need?

The governing rule is therefore:

```text
frozen foundation
    -> inspect current implementation and live use
    -> classify
    -> require consumer/evidence value before changing code
```

## Baselines and live evidence

### Source baseline

The implementation audited here is commit:

```text
c6e45d9e41d3b4d64b5b3dace01497c53e574026
```

The frozen Foundations documents live in the research Workspace on top of that implementation baseline.

### Deployed baseline

The live Runtime service was healthy during this audit and was receipted from:

```text
5561165077000818585bc959dd4bf87317b12f29
```

The only canonical commits between that deployment and the audited source baseline were documentation/CI maintenance (`6ce1ca9`, `186a897`, `c6e45d9`), not changes to Runtime execution semantics. The deployed behavior is therefore a useful live-consumer witness for this implementation family rather than an obsolete architectural generation.

### Agent-path evidence

The bounded Runtime trace for approximately the preceding 24 hours reported:

- 2,038 protocol Tool calls;
- 2,036 classified outcomes (99.90% outcome coverage);
- 33 failed outcomes (1.62%);
- 0 internal errors;
- 0 unknown-commit outcomes;
- 3 `reconcile_first` outcomes;
- 1 `CONCURRENCY_LIMIT` outcome.

High-volume surfaces included `workspace.exec` (661 calls), `workspace.execPlan` (298), `workspace.mutate` (214), `workspace.read` (181), `workspace.close` (159), `task.observe` (125), and `task.list` (75). `task.get`, despite carrying the richest exact Job inspection, was used only 3 times.

This matters because architectural complexity must be judged against actual consumer friction, not aesthetics alone. The evidence does **not** support a claim that admission, recovery, capacity, or exact-state distinctions are currently creating broad operational friction.

---

## Classification vocabulary

| Class | Meaning in this pass | Default engineering action |
| --- | --- | --- |
| **A — already-correct** | Current implementation faithfully realizes the frozen foundation at the scope it claims. | Preserve; add regression evidence only when useful. |
| **B — poorly-exposed** | Core semantics are correct, but normal consumers cannot easily observe the distinction or identity they need. | Prefer additive projection/documentation improvements over refactors. |
| **C — contingent-complexity** | Complexity comes from a concrete compatibility, rollback, persistence, substrate, or consumer contingency rather than the ontology itself. | Keep while the contingency is live; simplify only after its deletion condition is proven. |
| **D — real-mismatch** | Implementation materially violates or overclaims a frozen foundation and can change a consumer's safe action. | Repair with priority and acceptance evidence. |
| **E — no-consumer-need** | A valid research concept has no present Runtime consumer that requires first-class machinery. | Do not build it. |

---

## Current implementation x frozen foundations map

| Frozen foundation / distinction | Current implementation witness | Classification | Engineering decision |
| --- | --- | --- | --- |
| Proposal != committed Operation | `ExecutionProposal` / `TaskRunProposal`; proposal identity; Runtime resolves only delegated mechanical limits before durable admission | **A** | Preserve. Do not rename/refactor merely to mirror research nouns. |
| Stable Operation identity | `RuntimeJobRecord.operation_digest`; Registry idempotency and replay compare committed Operation identity | **A** in Core, **B** in normal projection before this pass | Preserve Core; expose the same `operationDigest` on ordinary Job projections. |
| Job != Operation ontology | Job is durable Runtime record/handle carrying `operationDigest`; compatibility APIs are explicitly documented as Job control | **A/C** | Preserve `Runtime Job` and `task.*` compatibility names. Rename only if a live consumer demonstrates unresolved ambiguity. |
| Operation != Attempt | one durable Job/Operation record can be inspected separately from `AttemptRecord`; Attempt has its own identity/state/termination evidence | **A** | Preserve. |
| Attempt != Process | Attempt persists independently of systemd/cgroup or Windows process identities; supervisor evidence is subordinate realization evidence | **A** | Preserve. |
| Durable admission before realization | Registry transaction commits Job, Attempt and reservation before dispatch; exact replay does not create second physical dispatch | **A** | Preserve as a core invariant. |
| Replay != re-execution | principal + `clientRequestId` + request/Operation identity reattach to the existing durable Job | **A** | Preserve. Do not add automatic redispatch for ambiguous effects. |
| Authority is local/scoped | `trusted_local`, `contained_local`, Windows limited/elevated authority, input authorities and provider commitments state exact owned scope | **A** | Preserve. |
| Isolation != authority reduction | tool/docs qualify contained execution and Host Dependency continuity; Linux path witness explicitly does not claim target namespace isolation | **A** | Preserve qualified claims; do not market existing containment as hostile-code sandboxing. |
| Workspace != security boundary | Workspace is source/resource context; isolation/authority is supplied by execution profile/provider mechanisms | **A** | Preserve. |
| Dependency closure is generally open | exact executable digest, immutable-input commitments and declared Host Dependencies bind selected prerequisites while explicitly denying complete environment closure | **A** | Preserve partial, named commitments. Do not infer a universal dependency closure. |
| Provider is part of realization authority | `ExecutionProviderSnapshot` is committed beside the Job and revalidated before dispatch | **A** | Preserve. |
| Resource/capacity constrains realization | global reservation plus per-Workspace serialization and machine-readable retry scope | **A/C** | Preserve current gates. Live trace does not justify weakening them: only one `CONCURRENCY_LIMIT` occurred in the measured window. |
| Effect != observation | arbitrary `workspace.exec*` remains effect-opaque; result/evidence is not interpreted as domain truth | **A** | Preserve. |
| Structured effects require owner-specific semantics | Runtime Release has receipt-first reconciliation; Workspace Patch has durable patch receipt and mixed-state `unknown` handling | **A** | Preserve specialized effect owners; do not generalize them into a universal Effect kernel. |
| Observation/Evidence != semantic truth | Artifacts, terminal evidence and exact state projections remain physical/mechanical evidence; `semanticCompletionEvaluated=false` | **A** | Preserve. |
| ClaimScope <= ActualSupportScope | Tool descriptions and machine fields distinguish execution disposition, delivery certainty, recovery and semantic completion | **A** | Preserve; treat any future broader claim as a D-class regression. |
| Lost/Orphaned/Unknown must remain representable | exact Attempt states, `deliveryDisposition`, recovery conditions and evidence-preserving reconciliation | **A** | Preserve. Do not collapse to success/failure. |
| Reconciliation != retry | `task.observe`, startup/maintenance reconciliation and `patch.get` converge owned truth without blindly repeating effects | **A** | Preserve. |
| Projection should not itself mutate unrelated work | `task.get`, `task.list`, `workspace.get/list` are projection-only; `task.observe` explicitly reconciles only its exact Job | **A** | Preserve. |
| Cross-layer references do not transfer truth ownership | bounded `ForeignReference` records Host/Harness identities but Runtime still owns only Runtime truth | **A** | Preserve. |
| Terminal evidence should identify the committed realization it supports | existing evidence carried Job/Attempt/Workspace/provider/input/executable/effect-result details, but previously required a Registry join to recover the Operation/plan identity | **B** | Add `operationDigest`, `executionPlanDigest`, and `executableDigest` directly to terminal evidence. Implemented in this pass. |
| Normal reconnect surfaces should preserve stable Operation identity | `task.get` exposed `operationDigest`, but `workspace.exec*`, `task.observe`, and `task.list` did not | **B** | Add `operationDigest` to `JobProjection`, `TaskObservation`, and `RuntimeJobSummary`. Implemented in this pass. |
| Rich committed-contract inspection | `task.get` exposes Operation identity and bounded history, while terminal evidence gives realization details; no high-frequency consumer currently asks for a full decoded Execution Plan projection | **B/E boundary** | Do **not** add a new full-plan inspection surface yet. Reopen only with a consumer requiring it. |
| Coarse historical `status` | retained beside exact state/disposition fields for existing display/log callers | **C** | Keep until named callers no longer depend on it; do not let it drive control semantics. |
| MCP `2025-11-25` / `2025-06-18`, LocalSessionManager, legacy initialize | retained for named live clients and rollback lifecycle | **C** | Keep until compatibility inventory deletion conditions are satisfied. |
| Legacy request identity derivation and additive persisted defaults | retained so older Jobs/Registries remain replayable after upgrades | **C** | Keep until explicit major-state cutover proves old records no longer require them. |
| v1/v2 proposal identity branches | distinguish historical fully explicit requests from delegated-limit proposal identity and rollback behavior | **C** | Keep while retained Jobs and rollback contracts require the distinction. |
| `workspace.diff` plus bounded `workspace.changes` | richer legacy convenience coexists with scalable paged projection | **C** | Do not collapse them without observed consumer migration. |
| Generic Effect ontology/kernel | Foundations recognizes Effect abstractly, but current effects have different ownership/reconciliation semantics | **E** | Do not build. |
| Universal deterministic replay/hermetic execution | Open-world dependencies and external effects prevent this as a generic Runtime guarantee | **E** | Do not build. |
| Universal hostile-code sandbox | Current consumers and authority model do not justify claiming or implementing universal hostile isolation | **E** | Do not build from theory alone. |
| Cross-provider transactions / distributed exactly-once | no Runtime ownership boundary can currently support the generic claim | **E** | Do not build. |
| Generic Goal/workflow/domain semantics | Host/domain layers own semantic completion and intent continuity | **E** | Keep out of Runtime. |
| Automatic retries after ambiguous external effects | conflicts with evidence-limited effect certainty | **E** | Explicitly do not build as a generic behavior. |

---

## D-class result

### No real mismatch identified in this pass

The audit found **no D-class semantic mismatch** between Runtime Foundations v1 and the current Runtime implementation that warrants a state-machine, ownership, authority, retry, isolation, or effect-model refactor.

This is not a declaration that Runtime is permanently correct. It is a bounded finding for the audited implementation and current consumers.

A future issue becomes D-class when implementation behavior or public claims materially exceed or contradict the frozen support scope, for example:

- treating process exit as semantic Task success;
- collapsing `lost`/`orphaned`/unknown delivery into safe success/failure;
- redispatching an effect-opaque Operation because a response was lost;
- claiming Workspace or current containment is a universal security boundary;
- accepting current provider/dependency bytes as equivalent to the bytes committed at admission;
- allowing an observation surface to silently create a second Operation;
- transferring Host/domain truth ownership into Runtime through a `ForeignReference`.

None of those were observed in the current implementation.

---

## Engineering changes admitted by this pass

### EC1 — Operation identity on ordinary Job projections

**Problem:** Core had the correct Operation identity, and `task.get` exposed it, but the high-frequency `workspace.exec*`, `task.observe`, and `task.list` paths did not. A consumer could therefore interact with a committed Operation repeatedly without ever seeing its stable Operation identity.

**Change:** Add additive `operationDigest` projection through:

```text
RuntimeJobRecord.operation_digest
    -> JobProjection.operation_digest
    -> TaskObservation.operation_digest
    -> RuntimeJobSummary.operation_digest
```

**Non-change:** Job IDs, client request identity, admission, replay, dispatch and Attempt identity are untouched.

### EC2 — Self-describing terminal evidence lineage

**Problem:** terminal evidence was already strongly Job/Attempt/provider/input-bound but required a Registry relationship to identify the committed Operation and resolved Execution Plan it evidenced.

**Change:** terminal evidence now also carries:

```text
operationDigest
executionPlanDigest
executableDigest
```

This makes the evidence lineage explicit without changing what Runtime claims the evidence proves.

**Non-change:** no stronger semantic-completion, hermeticity, isolation, external-effect, or domain-success claim is introduced.

### EC3 — Agent-facing documentation

`docs/agent-ux.md` now names `operationDigest` as the committed Runtime Operation identity and explicitly distinguishes it from Runtime Job and Attempt identity.

---

## Validation

The engineering changes were validated with:

- `cargo fmt --all -- --check`;
- complete `ordivon-runtime-core` unit suite: **221 passed / 0 failed** before the final assertion-only test additions;
- focused Core regressions for observation Operation identity and terminal evidence lineage: **3 passed / 0 failed**;
- complete `ordivon-runtime-mcp` library suite: **41 passed / 0 failed**;
- `python3 scripts/check_docs.py`: **documentation contract valid**.

The first MCP run intentionally surfaced one compile-time fixture omission after `TaskObservation` gained the additive field. The fixture was updated and an explicit JSON serialization assertion for `operationDigest` was added; no Runtime semantic failure was observed.

---

## Deferred questions

These are **not** admitted implementation tasks yet:

1. **Full committed Operation contract projection.** `task.get` might someday benefit from a bounded decoded-plan summary, but current consumption does not justify another public surface.
2. **Compatibility-name cleanup.** `task.*` and coarse `status` should move only when live consumers prove they no longer need them, not because Foundation nouns differ.
3. **Compatibility branch deletion.** Protocol and persisted-state branches already have explicit deletion conditions; no deletion is justified by this Foundations pass alone.
4. **Capacity-policy simplification.** Current live evidence does not show meaningful consumer friction from concurrency gating.
5. **New effect abstractions.** Release and Patch remain intentionally owner-specific until at least one new concrete consumer demonstrates a repeated invariant that a shared abstraction would actually simplify.

---

## Consumption verdict

Runtime Foundations v1 does **not** imply a Runtime rewrite.

The current Runtime is already structurally close to the frozen model in the places that affect safe action: durable Operation commitment, Attempt separation, scoped authority/provider/dependency binding, evidence-limited claims, explicit ambiguity, and reconciliation instead of blind retry.

The first engineering-consumption result is therefore deliberately small:

```text
preserve the semantic core
+ expose Operation identity consistently
+ make terminal evidence more self-describing
- do not rename for ontology aesthetics
- do not remove compatibility before its consumers disappear
- do not build research abstractions without consumer pressure
```

This document is the baseline for subsequent Runtime Foundations -> Engineering Consumption rounds. Future changes should be admitted against this classification rather than re-running RF0-RF16.
