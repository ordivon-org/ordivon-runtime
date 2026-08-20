# C8 — Binding / Authority / Realization Orthogonality

## Status

`THEORY_CONFIRMED_ORTHOGONALITY_STRONGLY_SUPPORTED`

C8 tests whether capability, authority, binding, standing and physical realization remain distinct under real substitution and live Workstation conditions.

No Runtime Foundation reopen condition is admitted.

## Case A — same higher-level obligation, different concrete binding

C7 established one higher-level Network publication obligation spanning distinct Runtime Operations:

```text
O1/A1 = HTTPS Git push
  operationDigest = a3cab3...
  timed_out

R1 = remote reconciliation
  remote main still 0fde170...

O2/A2 = SSH Git push
  operationDigest = 021f6f...
  succeeded
  Git evidence: 0fde170..80bd373 HEAD -> main

R2 = final verification
  remote main = 80bd373...
```

The higher-level publication obligation remained continuous while transport/binding changed HTTPS -> SSH.

The substitution did **not** preserve Runtime Operation identity:

```text
same higher-level obligation
!= same binding
!= same Runtime Operation
!= same Attempt/history
```

This confirms:

`Semantic/Purposive Continuity != Physical Realization Identity`.

A substitute realization path may satisfy the same bounded obligation without becoming the same historical occurrence.

## Case B — live capability identity without current realizability

C8 performed a read-only live Workstation observation of anchor `surf-clash`.

Observed:

```text
status = UNAVAILABLE
generationDigest = sha256:d7e66dcb...
capabilityDigest = sha256:9358ce15...
namespacePresent = false
resolverHealthy = false
transportHealthy = false
requiredTargetsHealthy = false
serviceActive = false
```

The generation/capability identity remains observable and stable enough to be projected, while operational standing is explicitly unavailable.

Therefore:

```text
CapabilityIdentityExists
!= CapabilityCurrentlyUsable
!= RealizationAvailable
```

Capability identity is not a success/availability bit.

## Case C — authority/profile exists without active realization

C8 also performed a read-only live Workstation observation of scoped egress profile `finance-okx`.

Observed:

```text
status = UNKNOWN
profileDigest = sha256:3e993ee4...
authorityDigest = sha256:3e993ee4...
memberCount = 2
listenerReachable = false
serviceActive = false
parentHealthOk = false
activeMember = null
eligibleMembers = []
watchdogDisposition = no-eligible-member
```

Thus a stable profile/authority object and configured member set coexist with no currently established realization.

Therefore:

```text
Authority/Profile Exists
!= Eligible Realizer Exists
!= Active Realization Exists
!= Serviceability Proven
```

Authority is a precondition/constraint on realization, not realization itself.

## Case D — available/discovered capability does not mint admission authority

Current Workstation regression evidence independently preserves the same boundary. Existing tests explicitly model resources/candidates that may be `AVAILABLE` or owner-source-identified while still requiring admission/revalidation and remaining `admissionEligible=false`.

Examples preserved in the current Workstation test corpus include:

```text
overlay candidate:
  status = AVAILABLE
  actionability = requires-admission

resource discovery candidate:
  providerAuthority = UNVERIFIED or OWNER-SOURCE-IDENTIFIED
  admissionEligible = false
  requiresOwnerRevalidation = true

external root request:
  admission status = ADMITTED_NOT_PROVEN
  explicit non-capability includes current-egress-root
```

These are engineering regression contracts rather than live resource-state claims, but they directly reject:

`AvailableCapability => AuthorizedCurrentRealization`.

## Orthogonal axes

C8 therefore requires at least these distinct axes:

```text
ObligationIdentity
Capability / Resource Potential
Authority / Permission / Admission Standing
Binding / Provider / Transport Choice
Physical Attempt / Realization
Evidence / Observation
Current Standing / Serviceability
```

They constrain one another but are not interchangeable.

A useful bounded admission form is:

```text
AdmissibleRealization(O, B)
  requires
    obligation still standing
    + binding compatible
    + authority/admission standing
    + required capability/resources available
    + current realization preconditions satisfied
```

No individual conjunct is sufficient by itself.

## Capability != Authority

**Strongly supported.**

The Workstation regression corpus contains available/discovered candidates that remain non-admissible until owner revalidation/explicit admission.

A system that can physically reach or describe a capability does not thereby possess authority to use it.

## Authority != Realization

**Strongly supported by live evidence.**

`finance-okx` retains a stable authority/profile digest and two configured members while no eligible member/listener/service is active.

Thus authority/configuration standing does not manufacture a physical realization.

## Binding != Realization

**Strongly supported.**

HTTPS and SSH are concrete bindings/transports under one higher-level publication obligation, but each binding required a distinct Runtime Operation/Attempt lineage. Selecting a binding does not mean that realization succeeded.

## Semantic identity != physical realization

**Strongly supported.**

The Network publication obligation survived substitution from HTTPS to SSH while physical Runtime identity/history changed.

The same semantic/purposive obligation may admit multiple candidate realization topologies across time without collapsing their histories.

## Standing != identity

The live `surf-clash` observation supplies a direct counterexample:

```text
generation/capability identity still observable
while standing = UNAVAILABLE
```

Identity continuity does not transport positive standing automatically.

This is consistent with Network Projection & Currentness v1:

`MigrationDoesNotCarryStandingByDefault` and `RecoveryMayPreserveIdentityWithoutPreservingStanding`.

## F3 — Stale / non-standing authority-capability

**PASS / strongly supported.**

Current Workstation regression contracts reject stale observations for execution and preserve cases where identity remains recoverable while admission fails closed.

Physical/capability continuity cannot substitute for current standing.

## F8 — Identity collapse

**PASS / strongly confirmed.**

Transport substitution preserves higher-level obligation continuity while producing distinct Runtime Operation/Attempt identities.

## F10 — Unsafe reuse

**PASS / strongly supported.**

An old binding/generation/result cannot be reused merely because its identity still exists. Current binding, authority and standing must be revalidated.

The Workstation tests explicitly preserve stale identity while rejecting execution, and C7 shows changed binding produces a new Operation lineage.

## Important negative result

C8 rejects both of these compressed models:

```text
Capability = Authority = Availability = Realization
```

and:

```text
same semantic obligation = same physical execution identity
```

Both destroy information needed for safe recovery, substitution and audit.

## Engineering consequence

Runtime/consumer reasoning should treat substitution as a fresh admission question:

```text
Higher-level obligation still stands?
  -> candidate binding/provider exists?
  -> capability sufficient?
  -> authority/admission current?
  -> binding/current generation valid?
  -> realization admitted
  -> Attempt executes
  -> evidence establishes scoped outcome
```

A provider/transport swap should preserve higher-level lineage only where the owning workflow proves that continuity. Runtime should preserve the new Operation/Attempt identity rather than rewriting history as if nothing changed.

This is a research grammar, not a universal production schema requirement.

## Theory disposition

- Orthogonality: **strongly confirmed across substitution + live Workstation evidence**.
- Capability != Authority: **strongly supported**.
- Authority != Realization: **strongly confirmed live**.
- Binding != Realization: **strongly confirmed**.
- Semantic Identity / Physical Realization Non-Identity: **strongly confirmed**.
- Identity continuity != positive standing: **strongly confirmed**.
- F3 / F8 / F10: **PASS**.

No R1-R8 Runtime Foundation reopen condition is triggered.

The existing six-family Runtime architecture already has the required separation; C8 is strong engineering-consumption evidence rather than a theory repair.
