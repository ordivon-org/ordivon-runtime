# C7 — Identity / Lineage under Retry-like Multi-Realization Continuation

## Status

`THEORY_CONFIRMED_IDENTITY_BOUNDARY_CLARIFIED`

C7 attacks identity collapse and causal over-attribution using the real Branch-H / C2 Network publication trace.

No Runtime Foundation reopen condition is admitted.

## Important correction to the informal C2 shorthand

C2 described one higher-level publication Operation `O` with A1/A2 for explanatory convenience.

The exact durable Runtime ledger shows a more precise structure. The HTTPS push, reconciliation read, SSH push and final verification are **distinct Runtime Jobs / operationDigests**, each with its own Attempt.

Therefore C7 distinguishes:

```text
O_pub
  = higher-level semantic/operational obligation:
    publish the intended Network owner-authority state

O1 / A1
  = Runtime Job/Attempt for HTTPS push

OR / AR
  = Runtime Job/Attempt for authoritative remote reconciliation

O2 / A2
  = Runtime Job/Attempt for SSH push

OV / AV
  = Runtime Job/Attempt for final remote verification
```

This matters: a retry-like continuation is not necessarily another Attempt of the same Runtime Operation. If binding/transport/admission changes, the continuation may be a new Runtime Operation under the same higher-level obligation lineage.

## Exact durable identities

### O1 / A1 — HTTPS push

```text
clientRequestId = atlas-h-network-push-main-20260818-v1
jobId           = job-01a014e6-ed5e-7161-8039-18e377b6bedc
operationDigest = sha256:a3cab3e9e3ab217f00006fcaea0e2b893abac0b2ff1f75f6e0c805f7815eaec3
attemptId       = attempt-01a014e6-ed5e-7161-8039-18ff01ceb6f9
attemptNumber   = 1
state           = timed_out
reason          = DEADLINE_EXCEEDED
```

The Attempt had one dispatch and no duplicate dispatch. It produced no Git delivery proof in retained stdout/stderr.

Supported claim:

`A1 reached the Runtime deadline without verified process/delivery success.`

Unsupported at this horizon:

```text
A1 caused remote advancement
A1 caused no remote advancement
O_pub failed
```

### OR / AR — reconciliation observation

```text
clientRequestId = atlas-h-network-reconcile-push-20260818-v1
jobId           = job-01a014e8-892e-77c2-be83-052292b66306
operationDigest = sha256:ce1addf6f10de0ad6864b051249cd8bcf2c66cc8025c80298f3f9dedeeaf52ba
attemptId       = attempt-01a014e8-892e-77c2-be83-053e18f03da6
state           = succeeded
observed remote = 0fde1702e18cbaa12896497e13f400f64c7d1756 refs/heads/main
```

This observation establishes that the intended advancement to `80bd373...` was absent at the R1 observation horizon.

It narrows knowledge about A1's externally durable effect without rewriting A1's identity or terminal state.

### O2 / A2 — SSH push

```text
clientRequestId = atlas-h-network-push-main-ssh-20260818-v1
jobId           = job-01a014e8-b0cb-76b2-b542-09a624885c82
operationDigest = sha256:021f6f605afaaa82faa73913e6ff912a5787d7628199de6e92ebba1b394f775d
attemptId       = attempt-01a014e8-b0cb-76b2-b542-09bbf1387eca
attemptNumber   = 1
state           = succeeded
```

Retained Git transport output reports:

`0fde170..80bd373 HEAD -> main`.

O2 is not O1. The transport binding changed from the timed-out HTTPS episode to an SSH push, and Runtime assigned a distinct request/job/operationDigest/Attempt lineage.

### OV / AV — final verification

```text
clientRequestId = atlas-h-final-remote-fences-20260818-v1
jobId           = job-01a014e9-f1cc-7340-a52b-624f0534f9d3
operationDigest = sha256:89efab4677a5b3c57b11239b2f44863f3ff0a154adf24df0454ad55006e50940
attemptId       = attempt-01a014e9-f1cc-7340-a52b-62587086ed40
state           = succeeded
observed Network remote = 80bd37393ff46703be1dec6940bd11ea0f7149dc
```

Again, verification evidence is a distinct Operation/Attempt, not the external effect itself.

## Identity graph

The safe lineage is:

```text
Higher-level obligation O_pub
    |
    +-- admits O1 --realized-by--> A1
    |                 |
    |                 +-- local terminal evidence: timeout
    |
    +-- admits OR --realized-by--> AR
    |                 |
    |                 +-- evidence: remote target effect absent at R1
    |
    +-- admits O2 --realized-by--> A2
    |                 |
    |                 +-- transport evidence: ref advancement reported
    |
    +-- admits OV --realized-by--> AV
                      |
                      +-- evidence: remote ref = 80bd373 at R2
```

None of these arrows is identity.

## Retry-like continuation vs same-Operation retry

C7 therefore refines the programme vocabulary:

```text
Retry-like continuation
  = a later realization attempt/operation intended to continue satisfying
    an unresolved higher-level obligation after prior uncertainty/failure
```

It may be represented as:

1. another Attempt under the same Runtime Operation, **or**
2. a new Runtime Operation under the same higher-level obligation lineage.

The Branch-H case is type 2.

This prevents the word `retry` from silently collapsing semantic obligation identity, Runtime Operation identity and Attempt identity.

## Causal attribution

C7 separates three claim strengths.

### Level 0 — terminal/process fact

From A2 Runtime success alone:

`A2 process exited successfully.`

This does not prove the external effect.

### Level 1 — post-attempt association

From R2 alone:

`After A2, the authoritative observation saw remote main = 80bd373...`.

This supports temporal association, not complete causal attribution by itself.

### Level 2 — bounded causal attribution

The combined chain provides stronger support:

```text
R1: before O2/A2, authoritative remote observation = 0fde...
A2: Git transport reports 0fde... -> 80bd... HEAD -> main
R2: after A2, authoritative remote observation = 80bd...
```

Within the tested Git-ref scope, this is sufficient for a bounded operational claim that O2/A2 is the supported delivery realization associated with the observed ref advancement.

It does **not** justify stronger claims such as:

```text
A2 is the sole metaphysical cause of every downstream consequence
A2 caused owner semantic acceptance beyond the publication contract
A2 caused Atlas refresh/convergence
A2 satisfied the higher-level Goal by itself
```

Causal attribution is therefore itself a scoped claim requiring a bridge/evidence chain.

## What happened to A1?

R1 proves the target external ref advancement was absent at the observation horizon after A1.

The safe claim is:

`No intended durable remote-ref effect from O1/A1 was present at R1.`

This is stronger and more precise than rewriting history as:

`A1 never did anything.`

A1 remains a real timed-out Attempt with its own dispatch/process history.

Thus:

`NoStandingTargetEffectAtR1 != NoHistoricalAttemptOccurrence`.

## F8 — Identity Collapse

**PASS / strongly confirmed with representation clarification.**

The real trace requires separate identities for:

- higher-level obligation;
- O1/A1 HTTPS execution lineage;
- reconciliation O_R/A_R;
- O2/A2 SSH execution lineage;
- verification O_V/A_V;
- external remote-ref states/effects;
- evidence artifacts/observations.

Any model that calls all of these `the retry` or reuses one status/identity loses recovery and causation information.

## F1 — Duplicate Effect

**PASS / confirmed.**

A second delivery Operation was not admitted until R1 established target-effect absence at the authoritative observation horizon.

## F2 — Lost Acknowledgement / ambiguity

**PASS / confirmed.**

A1 timeout remained a local terminal fact while external effect knowledge stayed unresolved until R1.

## New lineage clarification

C7 adds an important distinction to Runtime engineering consumption:

```text
Same higher-level obligation
  != same Runtime Operation
  != same Attempt
```

A changed binding/provider/transport may require a new admitted Runtime Operation even when the purposive obligation is continuous.

Which owner maintains the higher-level obligation identity depends on the system boundary. In Ordivon, Host or an owner-specific workflow may maintain purposive continuity while Runtime maintains each admitted Operation/Attempt lineage.

## Engineering consequence

Recovery/observability surfaces should preserve enough identifiers to answer separately:

```text
Which higher-level obligation was being pursued?
Which Runtime Operation was admitted?
Which Attempt physically ran?
Which external effect was observed?
Which evidence object supports that effect claim?
Which later Operation/Attempt was admitted after reconciliation?
What causal attribution is actually supported?
```

A single `retryCount` or `lastAttemptStatus` cannot answer these questions.

This is a research grammar, not a mandate for one production schema.

## Theory disposition

- Identity / Lineage Conservation: **strongly confirmed**.
- AttemptState != Operation/obligation state: **strongly confirmed**.
- Effect != Evidence: **strongly confirmed**.
- Causal attribution requires typed support bridges: **clarified**.
- Retry-like continuation may cross Runtime Operation identity: **clarified**.
- F8 Identity Collapse: **PASS STRONG**.
- F1 Duplicate Effect: **PASS**.
- F2 Lost Acknowledgement: **PASS**.

No R1-R8 Runtime Foundation reopen condition is triggered.

The clarification is representational/consumption-level: current Runtime theory already permits distinct Operation/Attempt/realization identities and owner boundaries; the real trace shows why higher-level obligation lineage must not be inferred from Runtime Job identity alone.
