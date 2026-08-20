# C3 — Fresh-Execution Non-Necessity / Support Reuse Consumption

## Status

`BOUNDED_THEORY_CONFIRMED_GENERIC_REUSE_NOT_IMPLEMENTED`

No Runtime Foundation reopen condition is admitted by this case.

## Question

When a current obligation appears and historical execution support already exists, must Runtime always create a fresh Attempt?

C3 separates two cases that must not be conflated:

1. replay of the **same Operation identity**;
2. a **new Operation** that appears mechanically or semantically equivalent to an earlier one.

## Case A — same Operation identity, exact replay

Workspace source state before the probe:

`sha256:a62795c4c08bc39a85c448eb4bac496cf8e6ed79200e57a57336dc9b3d44179a`

The first request used:

```text
clientRequestId = runtime-c3-support-reuse-same-operation-20260818-v1
command         = /usr/bin/sha256sum research/programme/CONSTITUTION.md
```

Runtime created:

```text
jobId       = job-01a01574-2d59-7d02-92f8-286ce6e2bb35
attemptId   = attempt-01a01574-2d59-7d02-92f8-287a7a7549c9
attemptNo   = 1
operationDigest = sha256:fe2b2f9c3036fb00a257865ab7cc918c1881f0d3abe3acc8c926e346575c9efd
result hash = dd9048ee467cb6f68817dedcca24a630be49bf0cee58043c286481d980726fb1
```

The exact request was replayed with the same clientRequestId.

Runtime returned the **same Job, same Attempt and same artifacts**. `task.get` showed:

```text
attempt count        = 1
dispatches           = 1
duplicate dispatches = 0
```

No fresh Attempt was created.

### Result

For one durable Operation identity whose execution support is already terminal and available:

`CurrentNeedForSameOperationResult != FreshExecutionNeed`.

The existing realization can discharge the replay obligation.

## Case B — new Operation identity, same mechanical operation

C3 then submitted the same executable, arguments and unchanged workspace under a new clientRequestId:

```text
clientRequestId = runtime-c3-support-reuse-new-operation-20260818-v1
```

Runtime produced:

```text
new jobId     = job-01a01574-8019-78d3-a4b1-3eac16793808
new attemptId = attempt-01a01574-8019-78d3-a4b1-3eb1a37ac363
operationDigest = sha256:fe2b2f9c3036fb00a257865ab7cc918c1881f0d3abe3acc8c926e346575c9efd
result hash = dd9048ee467cb6f68817dedcca24a630be49bf0cee58043c286481d980726fb1
```

The `operationDigest` and output were identical to Case A, but Runtime still created and dispatched a fresh Job/Attempt.

### Result

Current Runtime supports identity-preserving replay but does **not** infer generic cross-Operation support reuse merely from mechanical operation equivalence.

This is a crucial limit on C3 generalization.

## Case C — unsafe reuse attack: same command, changed input state

C3 created a temporary workspace input:

```text
research/programme/.c3-support-input.txt = "alpha\n"
```

Observed workspace state:

`sha256:a529466d9f1172cefcb6c78f2030b36abc1a6a9739c585de026c3e3174641e0d`

Hash operation A produced:

```text
input digest     = b6a98d9ce9a2d9149288fa3df42d377c3e42737afdcdaf714e33c0a100b51060
operationDigest  = sha256:4b32033517a7102b0c5516407f4a0e167500ae77a5cc9134f3e5234aeff636ae
jobId            = job-01a01575-35bf-7603-b9c1-679ea44bda35
attemptId        = attempt-01a01575-35bf-7603-b9c1-67a4d373b245
```

The exact same path was then changed to:

```text
"beta\n"
```

Observed workspace state became:

`sha256:6fb2711120096465c43e5f1342633ab94a11c0bea3217e2136b1344065a1d0c5`

The same executable and arguments now produced:

```text
input digest     = f2c82decdd7181cf98945929a62598db7e6b477e11f6e0eb0ae97020eff151ad
operationDigest  = sha256:db74f71dbd927129cfe0114c0823b07b1e0b305f6333204356c073a6c9507486
jobId            = job-01a01575-93f4-7772-bef0-07ddbc62a020
attemptId        = attempt-01a01575-93f4-7772-bef0-07e147e9c62c
```

The operationDigest changed even though the command text did not. Runtime therefore binds operation identity to workspace/source state strongly enough that this input change did not masquerade as the same operation.

## Case D — historical replay after current workspace changed

After the temporary input had changed from alpha to beta, C3 replayed the original alpha clientRequestId.

Runtime returned the original alpha Job/Attempt and original alpha result:

```text
jobId      = job-01a01575-35bf-7603-b9c1-679ea44bda35
attemptId  = attempt-01a01575-35bf-7603-b9c1-67a4d373b245
result     = alpha digest b6a98d9c...
```

It did not re-read beta and did not create another Attempt.

This is correct: replay restores support for the historical Operation whose input/source state was already bound. It does not claim that the alpha result satisfies a new beta obligation.

The temporary fixture was then removed and the workspace returned to its original clean source state.

## F10 — Unsafe Reuse

**PASS / theory confirmed with a strong boundary.**

C3 falsified two unsafe shortcuts:

```text
same command text => reusable support
same operationDigest => automatically reusable across distinct Operation identities
```

The first is rejected mechanically by source-state binding: changed input produced a changed operationDigest.

The second is not currently performed by Runtime: distinct clientRequestIds produced distinct Jobs/Attempts even when operationDigest matched exactly.

## Identity / Scope / Support / Standing interpretation

A reusable support candidate cannot be admitted merely because it looks similar.

At minimum, a support relation for a current obligation must establish:

```text
IdentityCompatibility
BindingValidity
Input/Source Compatibility
EvidenceScopeSufficiency
ResultAvailability
CurrentStanding
```

Only then can historical realization support legitimately discharge a current obligation.

## Refined computational-economy hypothesis

The earlier shorthand:

`ComputeNeed = UnsatisfiedSupportGap`

is too easy to over-read.

C3 refines it to:

```text
AdmissibleSupport(O)
  = support candidates whose identity/binding/input/scope/evidence/standing
    are sufficient for obligation O

FreshExecutionNeed(O)
  = unresolved support gap after AdmissibleSupport(O) is evaluated
```

or compactly:

`FreshExecutionNeed = UnsatisfiedAdmissibleSupportGap`.

This remains a **bounded programme hypothesis**, not a universal Runtime law.

## Engineering consequence

Current production Runtime already provides a valuable safe primitive:

- exact durable replay of the same Operation identity can avoid fresh execution;
- operation/source binding prevents changed workspace state from silently becoming the same operation;
- replay remains historical-operation replay even after current workspace changes.

What Runtime does **not** currently provide is a generic semantic support-discovery/substitution engine across distinct Operations.

Such a mechanism, if ever added, must not use command equality or operationDigest equality alone. It would require an explicit support/discharge contract and standing validation, likely at a Runtime↔SCD / owner-consumer boundary.

## Theory disposition

- Fresh-Execution Non-Necessity: **confirmed in identity-preserving replay scope**.
- Identity / Lineage Conservation: **strongly confirmed**.
- Specification / Realization Non-Identity: **confirmed**.
- Scope Conservation: **confirmed**.
- Standing requirement for reuse: **confirmed conceptually; no generic reuse mechanism exists yet**.
- F10 Unsafe Reuse: **PASS**.
- `ComputeNeed = UnsatisfiedSupportGap`: **narrowed** to `FreshExecutionNeed = UnsatisfiedAdmissibleSupportGap`.

No R1–R8 Foundation reopen condition is triggered.

## Reopen / future pressure

A future generic reuse system would create a genuine research/engineering question if it cannot determine support equivalence without collapsing SCD semantic equivalence, Runtime operation identity and owner-specific standing into one relation.

Until such consumer pressure exists, C3 supports keeping generic semantic reuse out of Runtime production.
