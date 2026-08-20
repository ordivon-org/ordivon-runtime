---
schema_version: 1
id: runtime.foundations.rf7.sources
title: RF7 Sources — Boundary, Environment, Context and Dependency
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
summary: Primary/canonical sources used by RF7 to separate namespace and VM boundaries, environment inheritance, dynamic dependencies, locale/device dependencies and current Runtime partial dependency-closure mechanisms.
evidence_status: verified
readiness: READY
related:
  - runtime.foundations.rf7
---
# RF7 Sources — Boundary, Environment, Context and Dependency

## S1 — Linux namespaces and mount namespaces

Linux man-pages project, **namespaces(7)** and **mount_namespaces(7)**.

Canonical upstream: Linux man-pages / kernel.org; public rendering: man7.org.

RF7 use:

- namespace kinds isolate particular global-resource views rather than forming one universal
  isolation boundary;
- mount namespaces provide distinct mount hierarchies, proving pathname resolution is
  namespace-relative;
- supports `NamespaceIsolation != CompleteIsolation` and
  `SamePathString != SameResolvedObject`.

## S2 — OCI Runtime Specification, Linux configuration

Open Container Initiative, **Runtime Specification — Linux Container Configuration** and
**config.md**.

Canonical source: opencontainers/runtime-spec.

RF7 use:

- Linux containers compose root filesystem, mounts, namespaces, cgroups, capabilities and
  other host-kernel mechanisms;
- omitted namespace types inherit the runtime's namespace for that dimension;
- reinforces that “container” is a boundary composition and remains host-kernel dependent;
- supports `ContainerBoundary != CompleteHostIndependence`.

## S3 — POSIX/Linux exec inheritance

The Open Group POSIX exec family and Linux man-pages **execve(2)** / POSIX **exec(3p)**.

RF7 use:

- a new process image receives an environment and ordinarily retains open file descriptors
  unless close-on-exec is set;
- proves execution context includes inherited resource references beyond executable bytes,
  argv and environment-variable text;
- supports `EnvironmentVariableMap != ExecutionEnvironment`.

## S4 — Linux dynamic linker

Linux man-pages project, **ld.so(8)**.

RF7 use:

- the dynamic linker finds/loads shared objects based on ELF metadata, runtime search paths,
  environment, cache and related rules;
- establishes shared libraries/runtime loader state as execution dependencies distinct from
  main executable bytes;
- current Ordivon P2/P3 provide direct physical confirmation of this dependency class.

## S5 — POSIX locale environment

The Open Group, **POSIX.1-2024 Base Definitions — Environment Variables** and utility
specifications.

RF7 use:

- `LANG`, `LC_ALL` and category-specific locale variables alter character handling,
  collation, messages, numeric/time formatting and related behavior;
- supports treating current fixed `LANG/LC_ALL` as real environment-variability reduction,
  while rejecting process-env closure as complete execution closure.

## S6 — KVM API

Linux kernel documentation, **The Definitive KVM API Documentation**.

RF7 use:

- KVM provides explicit VM, vCPU and device abstractions through the host kernel;
- VM creation/capabilities remain host KVM/CPU dependent;
- supports treating VMs as stronger/different guest-kernel boundaries without calling them
  closed universes.

## S7 — NVIDIA CUDA compatibility

NVIDIA, **CUDA Compatibility** documentation.

RF7 use:

- CUDA application execution depends on compatible GPU, driver, runtime/toolkit and, for
  dynamically linked programs, relevant libraries;
- provides a concrete hardware/software dependency closure example;
- supports `ApplicationBytes != GPUExecutionClosure`.

## S8 — Current Ordivon Runtime as empirical source

RF7 audits current Core, docs and prior physical experiments.

### P2 dynamic dependency

Current canonical Runtime research physically changed one runtime-loaded shared library
while preserving the target executable digest and prior executable identity; target output
changed from V1 to V2. This directly proves executable-byte identity is not execution
closure.

### P3 delayed consumption

A delayed `dlopen()` target passed pre-spawn Host Dependency validation, waited, then
resolved the path after it had changed. This disproved pre-spawn validation as a use-time
continuity guarantee and motivated Attempt-lifetime path/topology witnesses.

### P4 namespace-relative target view

A trusted-local same-authority target created a private mount namespace and made the same
textual Host Dependency pathname resolve alternate V2 bytes while the Runtime host
namespace continued to expose committed V1. The host witness observed no host-path drift.

This is RF7's strongest local falsifier for:

```text
host path witness = target namespace integrity
```

### Immutable inputs

Current input-bound execution resolves bytes under operator-owned `InputAuthority`, copies
them into Job-owned materialization, verifies exact inventory/digests and presents a
read-only target view under the declared target authority.

RF7 classifies this as strong explicit-input byte closure rather than execution closure.

### Execution provider commitment

Current `ExecutionProviderSnapshot` binds Runtime's own Linux Runner or Windows launcher
contract/digest and revalidates before dispatch. It explicitly excludes shared libraries,
kernel semantics, toolchain/package closure, network, wall clock and arbitrary target
provider semantics.

### Explicit minimal process environment

Runtime does not inherit the full host process environment. It constructs a bounded map
with fixed `PATH`, `HOME`, `LANG=C.UTF-8`, `LC_ALL=C.UTF-8`, temporary/cache/build paths
and request overlay according to profile/platform rules.

RF7 interprets this as constructed process-environment state, not full environment closure.

### `contained_local`

Current profile composes private network, capability reduction, no-new-privileges,
namespace/address-family restrictions, read-only/hidden host filesystem regions and exact
bind-back paths. Its canonical documentation explicitly rejects hostile-code/kernel-grade
isolation, complete confidentiality, controlled egress and disposable-machine claims.

## Source discipline

Later rounds should preserve:

```text
boundary claims are dimension-specific
namespace/container/VM names do not substitute for threat/isolation contracts
paths and names require resolution context
process env map is a projection, not complete execution environment
explicit inputs are not complete dependencies
dynamic dependencies defeat naive executable closure
freeze/materialize/bind/witness/virtualize/query/open are different strengths
Host Dependency scope remains runtime_host_namespace_path_witness
immutable inputs are strong byte closure but not whole execution closure
kernel/network/device/time/provider state remain open unless separately contracted
```
