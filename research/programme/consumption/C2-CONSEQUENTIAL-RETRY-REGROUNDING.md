# C2 — Consequential Retry / Open-World Re-grounding Consumption

## Status

`THEORY_CONFIRMED_BY_HISTORICAL_EXTERNAL_EFFECT_TRACE`

No new live consequential action was created for this research wave.

## Consumer / problem

A Runtime-mediated operation attempts to mutate an external authority. The local execution path terminates without success proof. The external effect may or may not have happened.

The question is not:

> Did the client process succeed?

It is:

> What is currently known about the intended external effect, and is consequential repetition admissible?

## Real historical case

C2 reuses the already-completed Research System Branch H publication trace recorded by:

`task:ordivon-research-system-owner-authority-closeout-materialization-20260818@rev3`

The operation was the publication/delivery of Network owner research authority state to the shared `ordivon-research` remote Git main.

The recorded sequence was:

1. an initial HTTPS Git push was attempted;
2. the client attempt timed out and produced **no success proof**;
3. the system did **not** infer delivery success and did **not** immediately repeat the consequential effect;
4. authoritative remote reconciliation showed remote main had not advanced to the intended state;
5. only after that observation was a new SSH push attempted;
6. the SSH push succeeded;
7. final remote verification established `ordivon-research` main at `80bd37393ff46703be1dec6940bd11ea0f7149dc`.

This is a real external-effect trace, not a synthetic timeout example.

## Identity

C2 requires at least the following identities to remain distinct:

```text
Operation O
  = publish the intended Network owner-authority state

Attempt A1
  = HTTPS push episode

Potential external Effect E1
  = remote main advancement caused by A1

Reconciliation observation R1
  = authoritative remote-ref observation after A1 ambiguity

Attempt A2
  = SSH push episode after R1 established retry admissibility

External Effect E2
  = verified remote main advancement to intended commit
```

`A1 timeout` is neither `E1 absent` nor `O failed`.

## Scope

The local attempt can support only a local/process claim:

```text
A1 did not return a verified success result before its local deadline.
```

It cannot support either of these stronger claims:

```text
remote effect definitely happened
remote effect definitely did not happen
```

The external-effect claim requires evidence from the authority that owns the remote ref state.

## Support

C2 distinguishes:

- local process/transport evidence;
- delivery acknowledgement evidence;
- authoritative remote-state evidence;
- final post-effect verification.

The authoritative remote observation after A1 narrowed `effectKnowledge` from `UNKNOWN` to `ABSENT_AT_OBSERVATION_HORIZON`, which made a new attempt admissible.

## Standing / retry admissibility

The C2 state transition is:

```text
Operation admitted
   ↓
A1 HTTPS push
   ↓
local timeout / no success proof
   ↓
EffectKnowledge = UNKNOWN
RetryAdmissibility = NOT_YET
RequiredAction = RECONCILE
   ↓
authoritative remote observation
remote ref still pre-effect
   ↓
EffectKnowledge = ABSENT_AT_OBSERVED_HORIZON
RetryAdmissibility = YES
   ↓
A2 SSH push
   ↓
remote verification
   ↓
EffectKnowledge = PRESENT_AS_INTENDED
Operation delivery claim = SUPPORTED_IN_SCOPE
```

## Falsifier results

### F1 — Duplicate-effect falsifier

**PASS / theory confirmed.**

The system did not turn `timeout` into permission for immediate repetition. It first established that the intended remote effect was absent at the authoritative observation horizon.

### F2 — Lost-acknowledgement falsifier

**PASS / theory confirmed.**

A missing success acknowledgement was represented as uncertainty rather than failure. The design remained safe under the counterfactual in which A1 might have reached the remote.

### F6 — Restore / recovery falsifier

**PASS / theory confirmed.**

Recovery was not `rerun the command`. It was:

```text
recover intent/history
→ observe current external authority
→ re-ground effect knowledge
→ derive admissible continuation
```

## Counterfactual branches required by the theory

If reconciliation after A1 had found the intended remote effect already present:

```text
EffectKnowledge = PRESENT_AS_INTENDED
RetryAdmissibility = NO
Action = ACCEPT_EXISTING_EFFECT / VERIFY
```

If reconciliation had found a conflicting remote advancement:

```text
EffectKnowledge = CONFLICTING_EXTERNAL_STATE
RetryAdmissibility = NO
Action = RECONCILE_CONFLICT
```

Only authoritative evidence of effect absence supports ordinary retry.

## Minimal engineering consequence

A consequential external-operation adapter should never encode retry as a direct function of local attempt failure alone.

At minimum it needs an explicit state comparable to:

```text
AttemptDisposition
EffectKnowledge
EvidenceAuthority
EvidenceHorizon
RetryAdmissibility
ReconciliationRequired
```

The important boundary is:

`RetryAdmissibility = f(current effect knowledge, operation contract)`

not:

`RetryAdmissibility = f(process exit / timeout)`.

## Theory disposition

- Scope Conservation: **confirmed**.
- Identity / Lineage Conservation: **confirmed**.
- Local / Global Non-Lifting: **confirmed**.
- Evidence / Truth Non-Lifting: **confirmed**.
- Open-World Re-grounding: **strongly confirmed**.
- History / Reproduction Non-Identity: **confirmed**.

No R1–R8 Foundation reopen condition is triggered.

## Generalization boundary

C2 does not prove that every consequential API has a usable authoritative reconciliation surface. Git remote refs provided one in this case.

For finance, payments, providers or physical systems, the theory therefore requires a domain-specific answer to:

> What evidence source is authoritative enough to re-ground effect knowledge before repetition?

If no such source exists, ordinary automatic retry may remain inadmissible.
