---
schema_version: 1
id: runtime.foundations.rf14.sources
title: RF14 Sources — Determinism, Nondeterminism and Agent Action
type: reference
profile: research
lifecycle: completed
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - researcher
  - builder
  - agent
updated: 2026-08-17
summary: Primary/canonical sources used by RF14 for reproducible builds, hermeticity, randomness and current Runtime replay/input/provider/environment boundaries.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf14
---
# RF14 Sources — Determinism, Nondeterminism and Agent Action

## S1 — Reproducible Builds definition

Reproducible Builds project, **Definitions**.

Canonical source: `https://reproducible-builds.org/docs/definition/`.

RF14 use:

- reproducible build requires same source code, build environment and build instructions to
  recreate bit-by-bit identical specified artifacts;
- relevant environment/input attributes and specified artifact set must be declared;
- supports `Reproducibility != Determinism` and purpose-relative reproduction contracts.

## S2 — Bazel hermeticity

Bazel official documentation, **Hermeticity**.

Canonical source: `https://bazel.build/basics/hermeticity`.

RF14 use:

- hermetic builds isolate actions from host-installed software and manage exact tools and
  dependencies;
- timestamps/build IDs, system binaries and source-tree mutation are named sources of
  non-hermeticity/nondeterministic build behavior;
- supports `Hermetic != Deterministic`, explicit dependency closure, and the principle that
  host/environment leakage undermines reproducibility.

## S3 — Bazel BUILD/Starlark hermeticity

Bazel official documentation, **BUILD files**.

Canonical source: `https://bazel.build/concepts/build-files`.

RF14 use:

- BUILD/Starlark evaluation is restricted from arbitrary I/O so it depends on a known set of
  inputs;
- demonstrates that restricting reachable dependency channels can engineer stronger
  hermeticity rather than trying to enumerate every ambient dependency after the fact.

## S4 — NixOS reproducible builds

NixOS, **Reproducible Builds**.

Canonical source: `https://reproducible.nixos.org/`.

RF14 use:

- Nix derivation/dependency identity and sandboxing provide a strong basis for reproducible
  builds;
- NixOS explicitly notes this is not sufficient because timestamps and other nondeterminism
  can still leak into outputs;
- supports `DependencyClosure + Sandbox != AutomaticReproducibility`.

## S5 — Linux randomness

Linux man-pages, **getrandom(2)** and **random(7)**.

Canonical public renderings:

- `https://man7.org/linux/man-pages/man2/getrandom.2.html`
- `https://man7.org/linux/man-pages/man7/random.7.html`

RF14 use:

- `getrandom()` obtains bytes from Linux kernel random sources/CSPRNG state;
- kernel randomness is target-observable state outside ordinary source/input identity unless
  explicitly controlled or virtualized;
- supports `Contained != Deterministic` and the treatment of randomness as a state/input
  dimension rather than generic error.

## S6 — Current Ordivon Runtime as empirical source

### Request replay

Current `(principal, clientRequestId)` plus request/proposal semantic fingerprint identifies
one durable logical request. Exact replay returns the original durable Job/Attempt/result and
does not redispatch a completed or already-admitted Operation. This is historical request
continuity, not physical deterministic replay.

### Operation/request identity

Current proposal/operation identity binds executable path, args, cwd, explicit env, limits,
steps, budget, execution profile, target, Windows authority where material and selected
Runtime-resolved facts. Runtime also binds source-state and execution-provider commitments in
the admitted Operation identity.

### Source identity boundary

`sourceStateDigest` includes semantic Git/source state while deliberately excluding ignored
cache/build outputs. Tests prove changing ignored `*.cache` bytes does not change the source
digest. Current docs explicitly state ignored files, wall-clock time, network state and
ambient external services are not part of source-state commitment.

### Executable/provider/input commitments

Runtime binds primary executable bytes and Runtime-owned Linux Runner/Windows launcher
provider identities. Explicit Host Dependencies and immutable input materializations close
selected dependency classes more strongly. None of these mechanisms claims automatic
transitive toolchain, kernel, CPU or external-service closure.

### Environment

Request environment participates in proposal/operation identity. Runtime constructs a bounded
execution environment rather than exposing the Runtime service's arbitrary ambient environment
as semantic target state. Contained payloads receive explicit HOME/XDG_CACHE_HOME/TMPDIR and
reduced environment semantics. These are reproducibility improvements, not full world
closure.

### Caches

Current source identity intentionally ignores caches/build outputs. Trusted-local Cargo target
bytes are Workspace-private while a stable target pathname supports cross-Workspace compiler
cache reuse; contained-local uses narrower Workspace-scoped caches. Cache/persistence state
therefore remains an explicit execution-world dimension outside source identity.

### Time and scheduling

Runner records wall-clock start/finish evidence and uses monotonic `Instant` for deadlines.
Neither mechanism virtualizes target-visible wall clock or internal process/thread scheduling.

### execPlan

Runner executes a committed ordered step list and records per-step results. Step order/control
is explicit; each step remains ordinary process execution and may observe time, randomness,
filesystem state or external Effects. `execPlan` is therefore deterministic control structure
without deterministic constituent outcomes.

### Structured Effects

Runtime Release owns deterministic effect identity but deployment outcome remains dependent on
host/service state. Workspace Patch owns deterministic before/after planning under exact state
preconditions but physical realization may fail/race and reconcile to unknown. These are
current examples of `DeterministicIdentity/Plan != DeterministicOutcome/Realization`.

## Source discipline

Later rounds should preserve:

```text
same Operation identity != same result
repeatability != determinism
reproducibility != determinism
hermeticity != determinism
exact replay != re-execution
source identity != complete execution-state identity
ignored state may remain causally relevant
executable/provider identity != full toolchain/kernel closure
wall clock and kernel RNG remain open unless controlled
seed controls one randomness dimension only
step-order determinism != internal execution determinism
network request sameness != response sameness
reproducible != correct
determinism != auditability
more frozen state != always better
Agent/model policy remains outside Runtime determinism authority
DeterminismClaimScope <= ControlledStateBoundaryScope
```
