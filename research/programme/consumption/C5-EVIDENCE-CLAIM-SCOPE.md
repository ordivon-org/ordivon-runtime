# C5 — Evidence / Claim Scope Heterogeneity

## Status

`THEORY_CONFIRMED_TYPED_SUPPORT_CLARIFIED`

C5 tests Scope Conservation and Evidence / Truth Non-Lifting against heterogeneous real Ordivon evidence roles.

No Runtime Foundation reopen condition is admitted.

## Core question

What exactly may a consumer claim from a piece of evidence, and what stronger claim remains unsupported?

Canonical law:

`ClaimScope <= ActualSupportScope`.

C5 shows that `ActualSupportScope` is not well modeled as one scalar evidence-strength value. It is typed by identity, authority, observation horizon, owner role and semantic domain.

## Case A — Runtime terminal execution evidence

C3 Runtime Job:

`job-01a01574-2d59-7d02-92f8-286ce6e2bb35`

The durable projection establishes:

```text
Attempt state = succeeded
exitCode = 0
mechanicallyConverged = true
resultAvailable = true
dispatches = 1
semanticCompletionEvaluated = false
```

This supports claims such as:

```text
the bound process exited zero
this Attempt reached Runtime terminal success
retained result artifacts exist
Runtime mechanically converged this Job
```

It does **not** support, without an additional owner bridge:

```text
external world effect occurred as intended
Host Goal succeeded
owner semantic objective was satisfied
world proposition is true
```

Runtime's own projection explicitly preserves `semanticCompletionEvaluated=false`, so the boundary is operational rather than merely documentary.

## Case B — external Git remote observation

C2 reused the real Branch-H delivery trace:

```text
A1 HTTPS push -> timeout / no success proof
remote-ref observation -> intended remote effect absent at observation horizon
A2 SSH push -> success
final remote observation -> intended remote ref present
```

The authoritative remote observation supports:

```text
remote ref had value X at observation horizon H
```

It does not, by itself, prove:

```text
which local Attempt caused X
why X changed
that a higher-level research Goal succeeded
that every downstream replica already observed X
```

Thus authoritative external-state evidence can be stronger than local process evidence while still remaining scoped.

## Case C — Host continuity evidence

Host resume/checkpoint surfaces explicitly preserve exact Task revision and semantic-working continuity while disclaiming Runtime/Git/domain validation.

A Host checkpoint supports:

```text
this Task had this recorded objective/frontier/established state at revision R
this continuity object is the current Host continuity revision if revalidated as such
```

It does not support:

```text
Git main currently equals the remembered commit
an owner publication remains current
an external effect occurred
World truth follows from the checkpoint
```

C1/C4 supplied real examples where old Host statements remained historically valid while later Git/owner topology and authority changed.

## Case D — owner publication evidence

An owner `CURRENT` + immutable owner publication supports owner-qualified semantic claims inside its declared AuthorityVersionRef and source fence.

It does not become:

```text
all-world truth
a Host Goal-completion fact
proof of post-source-fence changes
proof that every downstream Atlas projection is fresh
```

The owner publication is stronger than a generic record because it carries semantic authority, but that authority remains owner- and scope-qualified.

## Case E — Atlas projection evidence

Atlas consumes owner publications and produces source-fenced recovery/currentness projections.

Atlas evidence can support:

```text
Atlas observed AuthorityVersionRef V from owner source S
projection health was CURRENT_TO_SOURCE / SOURCE_ADVANCED_STALE / ...
this recovery locator was projected from the verified publication
```

It cannot support:

```text
Atlas independently owns the projected domain truth
Atlas freshness creates owner currentness
Atlas result implies World truth or Host Goal success
```

`AtlasProjection != OwnerTruth` is therefore an instance of Scope Conservation rather than an Atlas-only convention.

## Typed support lattice

The cases do not form one total order from weak to strong evidence.

For example:

- Host continuity can be stronger than Runtime evidence for historical Task state but irrelevant to process exit;
- Runtime terminal evidence can be exact for Attempt mechanics but irrelevant to owner semantic currentness;
- owner publication can be authoritative for owner claims but not for World causation;
- remote ref observation can be authoritative for one external Git state but not for Host Goal success.

Therefore C5 models support as a typed set:

```text
Support(E) = {
  identity scope,
  authority scope,
  semantic-domain scope,
  observation/source horizon,
  effect/control scope,
  freshness/standing scope
}
```

A claim is admissible only if the required support dimensions are covered by evidence or explicit bridges.

## Explicit support bridges

Strong claims require typed bridges. Examples:

```text
Runtime Attempt -> external effect
  requires effect observation / provider contract / reconciliation evidence

external effect -> Host Goal success
  requires Host/goal semantic evaluation

Git bytes -> owner current semantic authority
  requires owner CURRENT / AuthorityVersion relation

owner publication -> Atlas current projection
  requires verified projection/currentness evaluation

historical evidence -> current reusable support
  requires standing/currentness validation
```

C5 therefore strengthens the operational rule:

> Every semantic lift requires an explicit support bridge owned by the relevant role.

## Falsifier results

### F4 — Local / global lifting

**PASS / strongly confirmed.**

Runtime terminal success remains local/mechanical. It does not imply higher-level or external semantic completion.

### F5 — Evidence / truth lifting

**PASS / strongly confirmed.**

Authenticated, immutable or authoritative evidence remains scoped. Strong evidence does not manufacture a broader proposition.

### F9 — Weak-composition upgrade

**PASS / clarified.**

Combining Runtime success + Host continuity + Git observation + Atlas projection does not automatically create Goal success or World truth. Evidence composition requires a claim-specific composition argument; unrelated strong evidence is not semantic upgrading.

## Important negative result

The following model is rejected:

`ClaimConfidence = f(number_or_strength_of_evidence_objects)`.

C5 evidence supports instead:

`ClaimAdmissible(C) = RequiredSupport(C) subset-of ProvenTypedSupport`.

Confidence may matter inside a domain, but it cannot substitute for missing authority/scope/identity support.

## Operational Claim Theory disposition

C5 materially strengthens the candidate compression:

`OperationalClaim = Identity + Scope + Support + Standing`.

But C5 does **not** justify splitting Operational Claim Theory into an independent project yet. The current evidence still fits naturally as the epistemic discipline of Operational Realization Theory / Runtime consumption.

A future split would require autonomous domains to need the claim grammar without the realization problem that currently motivates it.

## Engineering consequence

Runtime/consumer APIs should avoid overloaded status words such as `success`, `verified` or `current` without a typed subject and scope.

A consumer-facing claim should be reconstructible as approximately:

```text
Claim {
  subject identity
  predicate
  scope/domain
  support refs
  evidence authority
  observation/source horizon
  standing/currentness
}
```

This is a research grammar, not a production schema mandate.

## Theory disposition

- Scope Conservation: **strongly confirmed and clarified as typed/multidimensional**.
- Evidence / Truth Non-Lifting: **strongly confirmed**.
- Local / Global Non-Lifting: **confirmed**.
- Non-Upgrading Composition: **confirmed**.
- Identity / Lineage Conservation: **required by claim attribution**.
- Operational Claim Theory: **strengthened, not promoted**.

No R1-R8 Runtime Foundation reopen condition is triggered.
