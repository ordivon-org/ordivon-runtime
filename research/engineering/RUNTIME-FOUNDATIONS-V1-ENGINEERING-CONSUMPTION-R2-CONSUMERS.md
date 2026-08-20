---
schema_version: 1
id: runtime.foundations.v1.engineering-consumption.r2-consumers
status: completed
date: 2026-08-17
foundation_baseline: Runtime Foundations v1 (RF0-RF16 frozen)
runtime_consumption_baseline: 7a03090945734c92bc43e1bd744109d6552ccfd9
---

# Runtime Foundations v1 -> Engineering Consumption R2: Consumer Semantics

## Purpose

R1 established that the current Runtime core is already substantially aligned with frozen Runtime Foundations v1 and admitted only additive identity/evidence exposure. R2 therefore moves one boundary outward:

> Do downstream consumers preserve the distinctions Runtime already exposes, or do they collapse them into weaker compatibility summaries and thereby change the safe next action?

This round does **not** reopen RF0-RF16 and does **not** add new Runtime-core machinery. It audits concrete consumers first and changes only owner-local translation logic where a real mismatch is demonstrated.

## Core consumption law

Runtime exposes several related but non-equivalent facts:

```text
status                         coarse compatibility/display summary
executionTerminal              physical Runtime Job terminality
executionDisposition           physical execution resolution
executionReasonCode            machine reason for that resolution
deliveryDisposition            certainty/reconciliation class
recoveryRequired               whether mechanical convergence is still required
resultAvailable                 whether durable physical result evidence exists
semanticCompletionEvaluated    always false; Runtime owns no domain completion truth
```

A consumer is unsafe if it uses the coarse `status` field as a substitute for the exact state vector when those exact fields imply a different next action.

The concrete falsifier found in this round is valid in current Runtime semantics:

```json
{
  "status": "succeeded",
  "executionTerminal": true,
  "executionDisposition": "succeeded",
  "deliveryDisposition": "reconciliation_required",
  "recoveryRequired": true,
  "resultAvailable": true,
  "semanticCompletionEvaluated": false
}
```

This state means physical execution resolved successfully, but Runtime has not mechanically converged. A downstream consumer must not infer that it may proceed as though delivery is committed and recovery-free.

---

## Consumer audit result

| Consumer | Pre-round behavior | Classification | R2 decision |
| --- | --- | --- | --- |
| **Host Guarded Mutation** | coarse `status == succeeded` advanced directly to domain verification | **D — real mismatch** | fixed owner-locally; exact Runtime delivery semantics now decide WAIT / VERIFY / BLOCK |
| **Host Code Change** | same coarse-status domain transition | **D — real mismatch** | fixed with same Host-local classifier |
| **Harness SQLite Runtime Bridge** | terminal polling stopped when coarse `status` entered a terminal-looking set | **D — real consumer mismatch** | exact delivery/recovery semantics now control reconcile / observed / unknown |
| **Harness Repository Repair / Tool Program** | inherits SQLite Runtime terminal observation | **A after inherited fix** | no separate abstraction; shared bridge correction propagates naturally |
| **Security Runtime-assigned Actor** | polling stopped on coarse terminal status; worker success admitted from `status=succeeded && exitCode=0` | **D — real mismatch** | exact committed Runtime success is now required before worker success can be consumed |
| **Finance research admission** | already requires exact terminal/disposition/delivery/recovery/result evidence | **A — already-correct** | no change |
| **Studio Runtime demo receipt** | displays execution/delivery evidence; no control decision discovered | **A / no control consumer** | no change |
| Game / World / Web / Computing | no material direct Runtime control consumer found in this scan | **E for this round** | no speculative work |

---

## Host correction

Workspace:

```text
runtime-consumption-host-exact-semantics-20260817
```

Source baseline:

```text
942bf41cb4f3ef6bfc2b70644f269fe6251b10a7
```

Commit:

```text
5f76055 host: consume exact runtime delivery semantics
```

Host now owns one local `classify_runtime_job_observation(...)` translation. It deliberately ignores coarse Runtime `status` for control and validates the exact Runtime state vector.

Host translation is domain-specific:

```text
Runtime in_progress
    -> Host WAITING / reconcile

Runtime reconciliation_required
    -> Host WAITING / reconcile

Runtime committed + executionDisposition=succeeded
    -> Host VERIFYING

Runtime committed + failed/timed_out/cancelled
    -> Host BLOCKED

Runtime delivery unknown / lost
    -> Host BLOCKED
```

The classifier also rejects a Runtime projection that claims `semanticCompletionEvaluated != false`; Host/domain completion remains Host-owned.

### Regression falsifier

Both Guarded Mutation and Code Change now explicitly test:

```text
status = succeeded
executionDisposition = succeeded
deliveryDisposition = reconciliation_required
recoveryRequired = true
```

and require the Host Task to remain `WAITING`, not enter `VERIFYING`. After the fake Runtime converges to `deliveryDisposition=committed` and `recoveryRequired=false`, Host reconciliation may advance to verification.

### Validation

- authoritative full Host test discovery with optional MCP dependency present: **258 passed / 0 failed**;
- Ruff: clean;
- `git diff --check`: clean.

---

## Harness correction

Workspace:

```text
runtime-consumption-harness-exact-semantics-20260817
```

Source baseline:

```text
286985c82874d293308297f66b23152c1ed53369
```

Commit:

```text
e760caa harness: consume exact runtime delivery semantics
```

`SQLiteHarnessRuntimeBridge` no longer treats coarse terminal-looking `status` as sufficient to stop observation. Its Runtime translation is:

```text
in_progress
    -> continue reconciliation / task.observe

reconciliation_required OR recoveryRequired=true
    -> continue reconciliation / task.observe

unknown
    -> preserve Harness unknown; never convert into ordinary observed success

committed terminal result
    -> project ordinary Harness observation
```

An inconsistent Runtime state vector fails closed to a Harness unknown observation instead of being guessed into a safe state.

### Test-model finding

The first full-suite run intentionally revealed a broader stale assumption in Harness test fixtures: several fake Runtime implementations emitted only `status=succeeded` without the exact Runtime semantic fields. Production logic correctly treated those fixtures as insufficient evidence.

R2 did **not** weaken the production gate. Instead, shared fake Runtime projections were upgraded to match the real Runtime contract, including:

```text
executionTerminal
executionDisposition
deliveryDisposition
recoveryRequired
semanticCompletionEvaluated
resultAvailable
```

Specialized fake projections now inherit the shared exact terminal semantics and override only domain payload such as `stdoutTail`.

### Validation

- focused Runtime bridge regression: succeeded coarse status continues observing until delivery converges;
- related repair/tool-program subset after fixture correction: **47 passed / 0 failed**;
- final full Harness suite: **423 passed / 0 failed / 3 skipped**;
- Ruff: clean;
- `git diff --check`: clean.

---

## Security correction

Workspace:

```text
runtime-consumption-security-exact-semantics-20260817
```

Source baseline:

```text
6a7a8f9b22cb4995d436da2968b135248f8f6bb3
```

Commit:

```text
471b892 security: gate runtime success on exact delivery
```

The Runtime-assigned actor now keeps observing while Runtime says in-progress or reconciliation-required. `deliveryDisposition=unknown` is an explicit failure to establish a consumable terminal result rather than an alternate terminal success class.

Even when the Security worker emits its own successful result marker, Security accepts that result only after Runtime independently satisfies all of:

```text
executionDisposition == succeeded
deliveryDisposition == committed
recoveryRequired == false
resultAvailable == true
exitCode == 0
semanticCompletionEvaluated == false
```

This preserves the ownership boundary:

```text
worker/domain result
    != Runtime physical execution truth
```

Both must independently support the admission decision.

### Validation

- focused Runtime-assigned tests: **10 passed / 0 failed**;
- full Security unit suite: **423 passed / 0 failed**;
- changed-file Ruff: clean;
- `git diff --check`: clean.

---

## Finance as the already-correct control sample

No Finance change was admitted. Existing production/research paths already consume the exact Runtime semantics expected by Foundations:

- `adapters/research_domain_bridge.py` requires committed delivery, no recovery, and result availability;
- `kernel/research_result_admission.py` requires terminal successful execution plus committed delivery and no recovery before admitting research evidence;
- associated tests explicitly model `executionDisposition`, `deliveryDisposition`, `recoveryRequired`, `semanticCompletionEvaluated`, and `resultAvailable`.

Finance therefore demonstrates the desired relationship:

```text
Runtime owns mechanical truth
Finance owns research/domain admission
Finance consumes Runtime evidence without letting Runtime decide Finance truth
```

This was classified **A — already-correct** and intentionally left unchanged.

---

## Why R2 does not introduce a shared cross-repository semantics package

Host, Harness, and Security all consume the same Runtime facts, but their safe next actions differ:

```text
Runtime reconciliation_required
    Host     -> Task WAITING
    Harness  -> continue task.observe
    Security -> continue task.observe

Runtime unknown
    Host     -> BLOCKED
    Harness  -> preserve unknown observation
    Security -> fail to admit Runtime-backed worker result
```

The common truth belongs to Runtime; the translation belongs to each consumer/domain owner.

A shared cross-repository adapter would risk moving domain policy back into Runtime-adjacent infrastructure and create a new versioned dependency merely to eliminate a small amount of duplicated validation. R2 therefore keeps the translation local while making the same Runtime invariants explicit in tests.

Reopen a shared adapter only if several consumers repeatedly require the **same translation semantics**, not merely the same source fields.

---

## Operation identity adoption is deliberately deferred

R1 added additive `operationDigest` exposure to ordinary Runtime projections and terminal evidence in Runtime research commit `7a03090`.

R2 does **not** force downstream consumers to persist or gate on `operationDigest` yet because:

1. `jobId` remains the correct Runtime continuation/reconciliation handle;
2. current consumers use it primarily to reattach to one durable Runtime Job, not as proof of semantic Operation sameness;
3. the additive Runtime projection is not yet merged/deployed to the canonical Runtime service;
4. no concrete consumer in this round demonstrated a decision that cannot be made safely with current request identity + Job reattachment + exact execution/delivery evidence.

Therefore downstream Operation identity adoption remains **B/E boundary — useful only when a concrete lineage consumer appears or after the R1 Runtime surface is integrated**.

Do not manufacture migration work merely because the field now exists.

---

## R2 engineering verdict

R2 changes the interpretation of the first consumption pass in one important way:

> Runtime Core can be Foundation-correct while downstream software still consumes its compatibility projection incorrectly.

The engineering boundary is therefore not just:

```text
Foundation -> Runtime implementation
```

but:

```text
Foundation
    -> Runtime-owned exact semantics
    -> consumer-owned translation
    -> domain action
```

The central R2 rule is:

```text
Never let a lossy compatibility summary erase an exact Runtime distinction
when that distinction changes the consumer's safe next action.
```

Concrete result:

```text
Runtime Core            unchanged
Host consumer           corrected
Harness consumer        corrected
Security consumer       corrected
Finance consumer        already correct
Other domains           no demonstrated need
OperationDigest rollout deferred
```

No RF0-RF16 foundation was reopened and no speculative Runtime abstraction was added.

## Integration status

The three consumer corrections are committed in isolated detached Runtime Workspaces but are **not merged into their canonical repositories and are not deployed** as part of this research-consumption round.

This document records the evidence and exact commits so later integration can be performed as an engineering publication step without repeating the Foundations analysis.
