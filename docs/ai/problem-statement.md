# Ordivon Problem Statement

## The problem

Capable agents can edit repositories and run commands quickly, but ordinary tool chains do not preserve enough execution truth to make long-running local work reliable.

A task can fail because:

- the agent reads stale or contradictory project context;
- several documents or Issues describe different current architectures;
- an execution result disappears when a process, client, or machine restarts;
- a command is dispatched but its actual outcome becomes ambiguous;
- duplicate requests consume the same external effect twice;
- cancellation stops one process but leaves descendants running;
- output or Artifacts change after the Runtime recorded them;
- blanket isolation and governance slow reversible work without reducing these risks.

The problem is therefore not a lack of generated plans, reports, Receipts, or policy objects. It is the absence of one clear current path and a small amount of durable execution truth.

## First-principles objective

Ordivon should let an agent transform user intent into a real result while preserving only what reasoning cannot reconstruct:

```text
current repository truth
→ bounded action
→ owned process
→ observed result
→ persistent identity
→ recovery or continuation
```

## What evidence means here

AI output is not evidence merely because it is fluent or complete. Ordivon does not solve that problem by governing vocabulary or requiring a parallel document bureaucracy.

Evidence comes from the mechanism that produced the fact:

- Git identifies repository state;
- SQLite transactions identify admission, idempotency, and concurrency ownership;
- systemd and cgroups identify the running process tree;
- digests bind requests, plans, outputs, results, and Artifacts;
- reconciliation compares persistent state with observable machine state;
- exact-head acceptance records the commands actually executed.

A summary may explain these facts, but it does not replace them.

## Design consequence

The user is the Principal and the Agent is the delegated Operator. In `trusted-local`, Ordivon inherits the host user's authority instead of deciding that network, credentials, Git, Docker, systemd, cloud CLIs, or other host capabilities are categorically forbidden.

The Runtime still protects facts that authority alone cannot reconstruct:

- duplicate client retries remain idempotent;
- ambiguous dispatch is never guessed or automatically repeated;
- real concurrency ownership is transactional;
- the complete process tree remains observable and cancellable;
- results and Artifacts remain identity-bound;
- restart reconciliation preserves what actually happened.

Reduced authority is an explicit `isolated` choice, not the default product philosophy.

## Non-goals

Ordivon does not need to model every claim, document, policy, approval, project, or future capability. It does not attempt to eliminate trust or prevent all mistakes before execution.

Its responsibility is narrower:

1. make the current execution path obvious;
2. execute within explicit local boundaries;
3. preserve non-reconstructable facts;
4. expose uncertainty instead of guessing;
5. recover from failure without duplicating side effects.

The smallest system that satisfies these responsibilities is preferred over a broader governance platform.
