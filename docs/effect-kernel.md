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

The model must not grow merely to resemble a workflow engine, scheduler, policy platform, or Agent framework.

## Hard commitments

### 1. Proposal is not commitment

Model output, a tool-call draft, or an Agent plan is only a candidate action. A physical dispatch is allowed only after the exact operation has been canonicalized and durably admitted.

### 2. Stable operation identity

The committed operation identity must bind every fact whose change would create a materially different action. Today this includes the authenticated principal, client request identity, Git Workspace source-state digest, executable identity, arguments, explicit request environment, working directory, limits, and budget.

Future facts may enter this identity only when they have enforceable semantics, not because they are useful metadata.

### 3. Request replay does not reinterpret the world

`clientRequestId` identifies the Agent's operation request, not the current Workspace state. A replay of the same request returns the originally committed Job even when that Job or later work has changed or closed the Workspace. Changing arguments, explicit request environment, limits, budget, executable request, or Workspace identity under the same `clientRequestId` is a conflict. Observation preferences and the server's current concurrency policy are not part of the operation request.

The first admission separately captures the world precondition and combines it with request identity to form the Operation identity. This separation is required for response-loss recovery: an effect must not make its own request unreplayable.

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

A non-opaque class is allowed only through a structured operation whose adapter owns and tests the complete Effect Contract. Examples may eventually include a Git operation bound to an expected revision, or an external API operation bound to a provider-supported idempotency key and receipt query.

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
- recoverable Workspace closure.

The source-state commitment is deliberately scoped. It excludes ignored caches and build outputs and does not make the trusted host filesystem immutable. Direct host writes after the Runner check remain outside this guarantee; eliminating that race would require executing from an immutable snapshot or stronger isolation and then resolving how intentional command changes return to the working Workspace.

It does not yet prove:

- the number or identity of external effects performed inside an arbitrary command;
- a general external effect receipt;
- tool-contract continuity between Agent decision and effect execution;
- operation-scoped authority beyond the trusted-local principal;
- external-world preconditions outside the Git Workspace;
- wall-clock time, network responses, ignored Workspace inputs, or other ambient process dependencies; target processes receive a committed configured execution environment plus explicit request overrides rather than inheriting the Runtime service environment;
- effect-aware retry or reconciliation through a structured adapter.

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

## Next implementation gate

The next code change is not a generic `effectClass` field. It must begin with one real structured operation and demonstrate the complete contract:

```text
canonical effect identity
+ enforceable precondition
+ committed authority
+ provider-owned receipt or state proof
+ deterministic reconciliation
+ tested ambiguity behavior
```

Until such an operation is selected, the current execution path remains deliberately opaque and fail-closed.

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
