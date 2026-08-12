---
schema_version: 1
id: runtime.effect-kernel
title: Ordivon Agent Effect Commit Kernel
type: concept
profile: engineering
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - builder
  - researcher
  - agent
updated: 2026-08-04
summary: Canonical concept and admission boundary for turning one Agent proposal into an identifiable, evidenced, recoverable physical commitment.
evidence_status: verified
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.model
  - runtime.operations
  - runtime.authority
---
<!-- cspell:words nonignored unreplayable -->

# Ordivon Agent Effect Commit Kernel

## Problem

An Agent is a nondeterministic controller. It can choose a different action after the same interruption, and the actions it chooses may change repositories, services, accounts, money, messages, or other external state.

Traditional runtimes already own process execution, resource isolation, restart, scheduling, durable workflow state, and ordinary retries. Ordivon Runtime must not reimplement those systems. Its irreducible problem is narrower:

> Convert one Agent-generated candidate action into a stable, authorized, world-bound reality commitment, then preserve what is known and unknown about that commitment across interruption without inventing or repeating effects.

The Runtime does not make Agent reasoning correct. It makes an admitted reality-changing operation identifiable, bounded, evidenced, and recoverable.

## Traditional foundations

Ordivon delegates established facts to their strongest owners:

| Problem | Existing owner |
| --- | --- |
| Process creation, exit, signals, files, permissions | Linux |
| Process-tree ownership and resource budgets | systemd and cgroups |
| Repository history, revisions, diff, rollback | Git |
| Transactional durable records | SQLite |
| Result bytes | Digest-bound files |
| Tool transport and schema exposure | MCP adapter |

A new Runtime abstraction is justified only when these owners cannot express the cross-boundary commitment needed by a nondeterministic Agent.

## Model

The kernel separates proposal, durable operation identity, one physical dispatch generation, identity-bound evidence, and reconciliation. Effect classes are enforceable recovery contracts owned by structured adapters, never caller assertions attached to arbitrary execution.

## Core objects

The minimal conceptual model is:

```text
Operation
→ Dispatch
→ Evidence
→ Reconciliation
```

Current implementation mapping:

| Kernel object | Current Runtime object |
| --- | --- |
| Operation | Job plus committed Execution Plan and immutable bundle |
| Dispatch generation | Attempt plus launch token and systemd unit |
| Physical ownership | cgroup-owned process tree |
| Execution evidence | Runner Result and Artifacts |
| Recovery | Registry, Runner evidence, and systemd/cgroup reconciliation |
| Structured effects | Runtime self-release uses a release-bound Job plus deterministic deployment receipt; `workspace.patch` uses a direct atomic filesystem transaction plus prepared/committed/unknown Patch receipt without a Job/Runner |

The model must not grow merely to resemble a workflow engine, scheduler, policy platform, or Agent framework.

## Hard commitments

### 1. Proposal is not commitment

Model output, a tool-call draft, or an Agent plan is only a candidate action. A physical dispatch is allowed only after the exact operation has been canonicalized and durably admitted.

### 2. Stable operation identity

The committed operation identity must bind every enforceable fact whose change would create a materially different action. Today this includes the authenticated principal, client request identity, Git Workspace source-state digest, executable identity, arguments, explicit request environment, working directory, physical budgets, execution-limit intent, the Runtime-owned execution provider that will cross the dispatch boundary, and any Agent-declared exact Host Dependency prerequisite files. For supported mechanical limits, an explicit value and an omitted/delegated value are different request facts: omission is not rewritten into the current operator value before request identity is formed. Host Dependencies are intentionally explicit and partial: Runtime does not claim it can infer a complete dynamic environment closure.

On first admission, Runtime resolves any delegated mechanical limits into one concrete Execution Plan and binds that plan to the physical world snapshot. The durable plan therefore records what actually governed execution even when the Agent intentionally left a limit to Runtime. Future facts may enter identity only when they have enforceable semantics, not because they are useful metadata.

### 3. Request replay does not reinterpret the world

`clientRequestId` identifies the Agent's operation request, not the current Workspace state. A replay of the same request returns the originally committed Job even when that Job or later work has changed or closed the Workspace. Changing arguments, explicit request environment, budget, executable request, Workspace identity, an explicit limit value, or whether a supported limit was delegated under the same `clientRequestId` is a conflict. Observation preferences, concurrency policy, and the operator values later used to resolve delegated limits are not part of the Agent's proposal identity.

Existing-Job lookup precedes current new-admission policy. A later operator-policy change therefore cannot make an already committed request unreplayable or silently recompute its effective plan. The first admission separately resolves delegated mechanical limits, captures the world precondition, and combines the concrete plan with request identity to form the Operation identity. This separation is required for response-loss recovery: an effect must not make its own request unreplayable.

### 4. At-most-once physical dispatch per Attempt

One Attempt may cross the physical dispatch boundary at most once. Ambiguous systemd dispatch is never speculatively repeated.

This is not a claim of exactly-once external effect. A process started once may itself perform zero, one, or many external effects.

### 5. Evidence before resolution

A Runtime resolution must be supported by identity-bound evidence. Process exit is execution evidence; it is not automatically an external-effect receipt or proof of semantic task completion.

```text
Execution Result
≠ External Effect Receipt
≠ Semantic Completion Claim
```

### 6. Uncertainty is preserved

If the Runtime cannot prove that work is running, terminal, or absent, it records uncertainty as `lost`, `orphaned`, or an active recovery condition. It does not translate missing evidence into guessed success, guessed failure, or automatic redispatch.

### 7. Drift invalidates stale commitment

A committed operation may dispatch only against the world and contract facts it binds. If a required revision, resource version, tool contract, authority, or other enforceable precondition changes before dispatch, the old commitment must fail closed and a new operation must be formed.

### 8. Retry is a new dispatch generation

A retry, when eventually supported, must be an explicit new Attempt linked to the previous Attempt and justified by effect semantics. The Runtime must never hide a second physical dispatch inside the first Attempt.

## Effect semantics

Effect-aware recovery requires more than an enum. A valid Effect Contract must jointly define:

```text
identity
preconditions
commit operation
evidence or receipt
reconciliation query
ambiguity policy
retry policy
```

The useful classes are:

| Class | Required property | Ambiguity policy |
| --- | --- | --- |
| `READ_ONLY` | Contract proves no externally visible mutation | May repeat when other preconditions still hold |
| `IDEMPOTENT` | Stable deduplication identity is accepted by the effect owner | Repeat only with the same deduplication identity |
| `RECONCILABLE` | Effect owner exposes a receipt or state query that can prove occurrence | Query before any new dispatch |
| `OPAQUE` | No enforceable deduplication or reconciliation contract | Never automatically repeat after ambiguity |

These classes are recovery contracts, not caller labels.

### Arbitrary execution remains opaque

`workspace.exec` accepts an arbitrary trusted-local executable. The Runtime cannot prove what that executable does internally. Therefore it is implicitly `OPAQUE` regardless of an Agent's description.

The public API must not accept a caller-asserted `effectClass` for arbitrary execution. Doing so would create a false guarantee.

A non-opaque class is allowed only through a structured operation whose implementation owns and tests the complete Effect Contract. Two materially different Runtime effects now meet that bar. Runtime self-release binds an exact Workspace commit, candidate-manifest digest, operator-owned deployment authority, deterministic effect identity, candidate deployer executable, and durable Job; `release.get` joins that Job with the deterministic deployment receipt without dispatching or retrying. `workspace.patch` binds an exact text-mutation request, complete before/after file digests, and a durable Patch operation before direct filesystem mutation; exact replay returns the original committed receipt, while `workspace.patch.get` reconciles prepared state to committed or `unknown` without creating a Job or repeating a mixed mutation. These implementations share a contract shape, not a common execution mechanism.

## Current proof and current gap

Current Ordivon Runtime already proves:

- idempotent admission by principal and client request identity;
- a digest-bound Operation and immutable Attempt bundle;
- at-most-once physical dispatch per Attempt;
- cgroup ownership of the complete command process tree;
- identity-bound Runner start and terminal Result evidence;
- cancellation intent and termination of the owned process tree;
- fail-closed `lost` and `orphaned` outcomes;
- late-result correction when identity-bound evidence appears;
- a Git source-state commitment covering `HEAD`, the semantic Git index, actual tracked source bytes, recursively initialized submodule source state, and nonignored untracked source;
- pre-spawn source-state revalidation and exclusion of Runtime-mediated mutation while a Job is active or held;
- first-admission commitment of the Runtime-owned Linux Runner or Windows launcher contract/digest, with pre-dispatch drift rejection and identity-bound terminal evidence;
- Runtime self-release as a real `RECONCILABLE` external effect: stable principal/client request identity, operation-v5 binding to release side truth, deterministic deployment receipt, exact replay before current-world checks, and projection-only `release.get` reconciliation across Runtime ingress replacement;
- `workspace.patch` as a second materially different structured effect: stable request identity, durable pre-mutation intent plus complete before/after digest plan, atomic filesystem commit, exact replay, and `workspace.patch.get` reconciliation that preserves mixed physical state as `unknown`;
- explicit trusted-local Linux Host Dependency commitments for known host runtime prerequisite files: admission-time digest validation, additive Job side truth, operation-v6 identity, pre-dispatch drift rejection, Runner pre-validation path/topology witnessing through the whole Attempt, explicit `HOST_DEPENDENCY_RUNTIME_DRIFT`, and terminal continuity evidence;
- target executable realization witnessing on local Linux: Runner establishes a path/topology witness before its final executable hash and keeps it through the step, preserving ordinary pathname semantics while converting runtime pathname replacement/write/delete into `EXECUTABLE_RUNTIME_DRIFT`;
- current provider-bound Linux Runner start evidence binds the actual `/proc/self/exe` image digest and, while live, Core independently cross-checks the systemd MainPID image against the committed provider;
- recoverable Workspace closure.

The source-state commitment is deliberately scoped. It excludes ignored caches and build outputs and does not make the trusted host filesystem immutable. Direct host writes after the Runner check remain outside this guarantee; eliminating that race would require executing from an immutable snapshot or stronger isolation and then resolving how intentional command changes return to the working Workspace.

`workspace.execBound` exposes the explicit immutable-input path for bytes outside the Git Workspace without transferring host-path authority to callers. Operator-owned named authorities are resolved only on new admission; Runtime opens only normal relative regular files beneath the authority, materializes and verifies the expected SHA-256 bytes into a Job-owned set, freezes the effective bindings into the plan, re-verifies that set before dispatch, and exposes it read-only only under `contained_local`. Exact replay resolves the existing Job before current authority or policy. This proves only those explicitly bound bytes; it does not turn ambient external state into a Runtime world model or domain truth.

It does not yet prove:

- the number or identity of external effects performed inside an arbitrary command;
- a general external effect receipt across arbitrary operations; release and Workspace Patch each own adapter-specific evidence and do not turn opaque commands into structured effects;
- automatic or complete target environment closure. P2 proved that a dynamically loaded host dependency can change output while target ELF digest and prior operation identity remain unchanged; P3 then proved that pre-spawn revalidation alone still allowed delayed `dlopen()` to consume replacement bytes. Runtime now witnesses declared filesystem paths through execution but still refuses to pretend that this discovers every loader, module, driver, network, service, mount-namespace, or other ambient dependency;
- generic target/tool-contract continuity beyond the Runtime-owned Runner/Windows launcher and the explicitly committed release/Host Dependency contracts;
- operation-scoped authority beyond the trusted-local principal;
- external-world preconditions outside the Git Workspace other than immutable-input bytes and explicitly declared trusted-local Host Dependency prerequisite files;
- wall-clock time, network responses, ignored Workspace inputs, or other ambient process dependencies; target processes receive a committed configured execution environment plus explicit request overrides rather than inheriting the Runtime service environment;
- generic effect-aware retry or reconciliation across arbitrary adapters. Release and Workspace Patch prove a shared contract vocabulary, but they do not justify a generic `EffectAdapter`, caller `effectClass`, or common physical dispatcher.

These are real gaps. They must not be hidden by process success, Artifact presence, tracing, or additional orchestration layers.

## Cost model

Each stronger guarantee has a cost:

| Guarantee | Necessary cost |
| --- | --- |
| Tool-contract continuity | Versioned canonical contract digest |
| World-bound execution | Stale-precondition rejection and re-planning |
| Git source commitment | Two full tracked-source scans per new dispatch and no Runtime-mediated mutation while that Workspace owns active work |
| Idempotent retry | Effect-owner deduplication support |
| Reconciled effect | Provider receipt or state-query adapter |
| Operation-scoped authority | Canonical authority grant and expiry rules |
| Ambiguity quarantine | Lower automatic completion and possible human decision |
| Durable evidence | Storage, privacy, and retention decisions |

Ordivon chooses explicit ambiguity over fabricated completion. Lower apparent liveness is acceptable when the alternative is an unbounded duplicate reality effect.

## Admission rule for future features

A persistent addition to the Effect Kernel must answer all of the following:

1. Which concrete failure or ambiguity does it solve?
2. Why can Linux, systemd, Git, SQLite, the target provider, or an upper Agent layer not own it better?
3. What invariant becomes enforceable?
4. What evidence proves that invariant?
5. What new failure mode and operating cost are introduced?
6. Which existing path does the addition replace or strengthen?

A field with no enforcement, an event with no recovery decision, or an abstraction with no real adapter does not enter the kernel.

## P3 physical-realization result

P3 attacked the remaining gap between “Runtime validated this path” and “the Attempt later consumed the same physical world.” A deterministic delayed-`dlopen()` experiment succeeded after all P2 checks and produced V2 while terminal evidence still carried the committed V1 Host Dependency digest. That falsified the claim that pre-spawn validation was sufficient. Runtime therefore now distinguishes three boundaries instead of forcing them into one abstraction:

- declared Host Dependency paths are watched from before final Runner validation through terminal; ordinary filesystem drift makes the Attempt fail instead of allowing a false successful continuity claim;
- target executable paths are watched from before final Runner hash through each step, while the original pathname remains the executed path so target-visible filename semantics do not change;
- the Linux Runner provider independently reports its actual `/proc/self/exe` digest and Core cross-checks the live systemd MainPID image whenever it is still observable.

Two stronger-looking mechanisms were tested and rejected. A single-file systemd read-only bind did not stabilize future pathname resolution when the parent directory entry was replaced. Sealed memfd execution did provide exact bytes for ELF and shebang targets, but changed `__file__`, `sys.argv[0]`, and `/proc/self/exe`, so making it the default would silently change the target world to satisfy Runtime's evidence model. P3 keeps the weaker but truthful path-witness contract.

P3 also audited `workspace.close`, cache prune, reclaim, and lifecycle operations as possible third structured effects. None graduated. `workspace.close` physically removes the Git worktree before its closed tombstone is published, so it lacks durable pre-mutation intent; cache/reclaim/lifecycle receipts lack stable effect identity plus exact replay/reconciliation semantics. Runtime therefore still has exactly two structured effects—self-release and Workspace Patch—and P3 adds **no** generic Effect framework.

## P4 same-authority view result and effect re-audit

P4 attacked the explicit boundary P3 left open instead of treating the host-path witness as isolation. The production authority probe established that `trusted_local` currently executes as root with mount-namespace authority. A Runtime Job then committed V1 of one declared Host Dependency, created a private mount namespace inside the target, bind-mounted an unchanged alternate V2 file over the same pathname only in that target view, and exited zero after reading V2. The Runtime host pathname remained V1 and no inotify-visible host event occurred. This proves that a host-namespace path witness is not target-namespace isolation and is not proof that the target consumed the committed bytes.

That result does **not** justify silently restricting `trusted_local`. A target with the same authority is deliberately trusted to use that authority; turning Host Dependency declarations into a security sandbox would change the execution contract, and a process-local syscall filter would not create an independent trust boundary against same-authority delegation. P4 therefore keeps execution semantics unchanged and makes the existing proof boundary machine-readable instead: `runtime.describe` and terminal Host Dependency continuity evidence carry scope `runtime_host_namespace_path_witness`. Strong target-view integrity remains a different problem that requires an explicit reduced-authority or external-isolation contract and a real workload before implementation.

P4 also re-audited every state-changing public surface rather than promoting the next convenient mutation into an Effect. `workspace.open` can be reconciled by an explicit Workspace identity but repeating open is not exact effect replay and there is no durable pre-mutation Effect receipt; `workspace.close` still lacks durable pre-mutation intent; `workspace.mutate` intentionally has no durable `clientRequestId` receipt; `task.cancel` durably controls Runtime lifecycle but cannot prove or reconcile external side effects already performed by the target; `workspace.exec`, `workspace.execPlan`, and `workspace.execBound` remain execution contracts rather than external-effect contracts. No third structured Effect graduated. Runtime still has exactly self-release and Workspace Patch, so there is still no evidence for a generic `EffectAdapter`, `EffectRegistry`, caller effect class, common physical dispatcher, or generic effect-aware retry.

## Two-effect result and next implementation gate

Runtime now has two materially different structured effects: self-release and Workspace Patch. Their implementations prove seven shared contract-level invariants without proving that a shared code framework is useful:

```text
stable request/effect identity
+ durable intent and enforceable preconditions before mutation
+ one physical commit owner
+ identity-bound receipt/evidence
+ exact replay before current-world reinterpretation
+ explicit reconciliation query
+ preserved uncertainty instead of guessed completion
```

The mechanisms remain deliberately different: release is an asynchronous Job/Runner operation whose external receipt may become terminal before the generic Job lifecycle converges; Workspace Patch is a direct atomic filesystem transaction with no Job/Runner and may reconcile a prepared receipt to `unknown` when physical files are mixed. P2 therefore records the shared contract but does **not** introduce a generic `EffectAdapter`, `EffectRegistry`, or caller-supplied class. A common code abstraction requires demonstrated duplicated implementation burden, not merely conceptual similarity. Arbitrary execution remains opaque and fail-closed.

## Boundary

The kernel owns only the narrow commitment between admitted operation and physical reality. It does not own Agent reasoning, workflow planning, semantic completion, provider policy, generic scheduling, or guarantees that the target command's external effects are exactly once.

## Related work

[`runtime.md`](runtime.md) defines the complete Runtime architecture, [`operations.md`](operations.md) defines deployment and recovery practice, and [`effect-comparison.md`](effect-comparison.md) preserves the bounded comparison protocol used to evaluate future effect-aware adapters.

## Non-goals

The Effect Kernel does not own:

- Agent planning, prompts, memory, reflection, or model routing;
- semantic correctness or user-goal completion;
- generic workflow DAGs, task queues, or cluster scheduling;
- untrusted-code isolation;
- arbitrary exactly-once external effects;
- user-experience or context experiments;
- a second history, tracing, or policy platform.

Ordivon Runtime is valuable only if this small boundary is harder, clearer, and more trustworthy than the application code it replaces.
