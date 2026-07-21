# Ordivon Capability Scope v0 Evidence

Status: supporting evidence
Evidence origin: AI-written summary of local tool output
Branch: `agent/capability-scope-v0`
Base: `facebd4a7f8a7e0e24413c881a8bf1d176e1c775`

## Observed local inputs

Tracked Policy:

```text
policies/execution/ordivon-local-dev-v1.json
raw SHA-256:
41e4f789ec0e8875d4a540f75c22d812d6005432787337401ed17157967d2621
mode: 0644
```

Resolved executable:

```text
/root/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/cargo
cargo 1.95.0 (f2d3ce0bd 2026-03-21)
SHA-256:
841072d1d92f9e841d9ba5b0814182a0adf064acf4527cd120967b7bc49dcb66
mode: 0755
symlink: false
```

## Generated plan

The evaluator loaded the tracked Policy and generated the governed plan file:

```text
docs/audits/runtime/ordivon-capability-scope-v0-plan-2026-07-21.json
```

Plan file SHA-256:

```text
47ed10241373d894c984f7c7bfd715b114225749c040fc4d40358cd221005e11
```

Semantic Policy digest:

```text
sha256:3622e35e25754ed4a8c0c0600e075f552cddd0d76ea547de99de94346fee60d6
```

Request digest:

```text
sha256:c5336ebf702c442025a33398811fe9f6643a1fa41d98d20a27f70806c9daad50
```

Resolved plan fields:

```text
profile: cargo.build
argument count: 4
working directory: /root/projects/ordivon-structured-exec-v0
environment keys: CARGO_HOME, HOME, PATH, RUSTUP_HOME
timeout: 900000 ms
stdout retention: 16777216 bytes
stderr retention: 16777216 bytes
```

The probe performed Policy loading, evaluation, and JSON serialization only.
No target build process was observed.

## Verified denial matrix

Automated tests cover Policy symlinks, writable Policy files, duplicate JSON
keys, executable symlinks, writable executables, digest mismatch, executable
replacement after load, canonical root aliases, CWD symlink escape, profile
root escape, non-exact arguments, environment denial, timeout/output excess,
concurrency limits, and disabled or unknown profiles.

## Limitations

This evidence does not establish authority authentication, atomic concurrency
reservation, workspace revision binding, sandboxing, process creation,
persistent Job state, output capture, terminal results, or MCP exposure.

The evaluator rechecks executable identity before plan creation, but the future
runner must still reopen with no-follow semantics and verify the digest
immediately before process creation.

The concurrency snapshot is an input observation, not a reserved slot. P4 must
make admission and reservation atomic.

Cargo may execute repository-controlled build scripts, procedural macros,
linkers, and configured tools. The profile grants authority to run the reviewed
workspace build graph; it is not a side-effect-free compilation claim.
