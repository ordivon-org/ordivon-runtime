# Ordivon Capability Scope v0

Status: locally verified evaluator
Authority: supporting architecture evidence
Runtime status: no process execution

## 1. Purpose

P3 turns the P1 capability schema into a real deny-by-default evaluator. It
loads a bounded policy, resolves filesystem identities, evaluates one typed
`JobStartRequest`, and returns a closed `ExecutionPlan`.

The evaluator does not start a process, create a systemd unit, reserve a Job
slot, persist state, or expose an MCP tool.

```text
CapabilityPolicy + JobStartRequest + authority context + concurrency snapshot
→ validate
→ canonicalize
→ verify
→ deny or produce ExecutionPlan
```

Policy presence is not activation. A valid plan is not proof that execution
occurred.

## 2. Public boundary

The request still selects only:

```text
profileId
args[]
cwd
timeoutMs
envOverrides
stdoutRetentionBytes
stderrRetentionBytes
clientRequestId
```

It never selects a shell or executable. The resolved plan contains the exact
canonical executable, full argv, canonical cwd, closed environment, limits,
principal, authority reference, and semantic digests.

## 3. Policy loading

Production-oriented file loading is fail-closed:

- maximum policy payload: 1 MiB;
- policy path must be a non-symlink regular file;
- policy file must not be group- or world-writable;
- file identity is compared before and after open;
- duplicate JSON object keys are rejected at every nesting level;
- unknown typed fields are rejected by Serde;
- schema version, identifiers, roots, profiles, and limits are validated;
- the semantic policy digest uses canonical JSON, not source formatting.

`load_capability_policy_bytes` exists for trusted embedded input and tests. A
production loader should use `load_capability_policy_file` so file-type and
permission checks are applied.

## 4. Filesystem resolution

All allowed roots, profile roots, and request CWD values are canonicalized.
Scope checks occur only after canonicalization.

This blocks a request such as:

```text
allowed/workspace/escape -> /outside/root
```

from passing a lexical-prefix test. Canonical aliases that resolve to the same
root are rejected as duplicate Policy entries.

A profile root must remain inside at least one global Policy root.

## 5. Executable identity

The executable must be:

- an absolute path;
- a non-symlink regular file;
- executable by at least one Unix execute bit;
- not group- or world-writable;
- equal to the Policy-declared SHA-256.

The evaluator opens and hashes the file during Policy resolution. It records
its device, inode, size, modification timestamp, and digest, then repeats the
inspection immediately before producing a plan. Any identity change requires
Policy reload and reevaluation.

The local Cargo profile binds the real toolchain binary:

```text
/root/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/cargo
```

It does not bind `/usr/bin/cargo`, because that path is a rustup symlink and
executing canonical `/usr/bin/rustup` would not preserve cargo proxy semantics.

The future runner must still reopen with no-follow semantics and verify the
planned digest immediately before `exec`. P3 reduces but does not eliminate the
evaluation-to-execution race.

## 6. Argument policy

V0 uses exact request argument vectors. A prefix, subsequence, extra argument,
or reordered vector is denied.

The local profile resolves exactly to:

```text
cargo build --release -p ordivon-mcp
```

The Policy also bounds:

- maximum profiles: 64;
- maximum global/profile concurrency: 64;
- maximum exact argument vectors per profile: 64;
- maximum runtime: 24 hours;
- output retention: the P1 contract ceiling.

The tracked local profile is narrower: one global slot, one profile slot,
900,000 ms runtime, and 16 MiB for each output stream.

## 7. Environment policy

The plan environment begins empty. It is populated only from
`baseEnvironment`, then from exact client overrides allowed by
`environmentRules`. A future runner must call the equivalent of `env_clear()`
before applying this map.

Client requests cannot control profile-owned or execution-shaping variables,
including:

```text
PATH HOME CARGO_HOME RUSTUP_HOME CARGO_TARGET_DIR
LD_PRELOAD LD_LIBRARY_PATH BASH_ENV ENV
PYTHONPATH NODE_OPTIONS RUSTC_WRAPPER
GIT_SSH_COMMAND GIT_CONFIG_* and Cargo target runners
```

High-risk injection variables are also prohibited in the Policy base
environment. Ordinary exact-value variables such as `RUST_LOG=info` may be
allowed by a profile.

## 8. Concurrency

The evaluator receives explicit observed counts:

```text
globalRunning
profileRunning
```

A count at or above its limit produces retryable `CONCURRENCY_LIMIT`.

This is a decision check, not an atomic reservation. P4 must combine Registry
admission and slot reservation in one durable critical section; otherwise two
concurrent callers could evaluate the same stale count.

## 9. Digests

The evaluator emits:

- `policyDigest`: canonical typed Policy JSON;
- `requestDigest`: canonical typed request JSON;
- `executableDigest`: observed executable bytes.

Object key order and whitespace do not alter semantic digests. Array order is
retained because argument and Policy ordering can carry meaning.

The tracked local smoke produced:

```text
policyDigest:
sha256:3622e35e25754ed4a8c0c0600e075f552cddd0d76ea547de99de94346fee60d6

executableDigest:
sha256:841072d1d92f9e841d9ba5b0814182a0adf064acf4527cd120967b7bc49dcb66
```

## 10. Cargo authority meaning

Cargo is not a side-effect-free compiler primitive. It can execute:

- repository build scripts;
- procedural macros;
- configured rustc wrappers and linkers;
- target runners and external tools;
- code selected through Cargo configuration.

P3 blocks inherited and client-controlled execution-shaping environment, but it
does not sandbox repository code. The `cargo.build` profile grants authority
to execute the reviewed workspace build graph under the runner account.

## 11. Local machine-bound Policy

Tracked candidate:

```text
policies/execution/ordivon-local-dev-v1.json
```

It binds the current WSL workspace, Cargo 1.95.0 toolchain path, and executable
digest. A Rust toolchain update or workspace relocation intentionally makes it
fail closed. Updating it requires a new review and evidence record.

The file is not automatically loaded by a service. It does not activate
execution or confer authority merely by existing in Git.

## 12. Verified negative cases

Automated tests cover:

- Policy file symlink and writable Policy rejection;
- duplicate JSON key rejection;
- executable symlink, writable executable, and digest mismatch;
- executable replacement after Policy load;
- canonical root aliases and CWD symlink escape;
- profile root outside global scope;
- exact argument denial;
- unlisted, wrong-value, and execution-shaping environment denial;
- timeout, output, runtime, and concurrency ceilings;
- disabled and unknown profiles;
- canonical digest stability across JSON formatting.

## 13. Remaining boundaries

P3 does not provide:

- authority-token authentication;
- atomic concurrency reservation;
- workspace revision or clean-tree binding;
- TOCTOU-free executable FD handoff;
- runner environment clearing or process creation;
- persistent Job state, output, result, or Operational Receipt append;
- MCP Job tools or production deployment.

A valid plan is therefore an admission candidate, not an execution fact.

## 14. Next bounded action

P4 should implement the durable runner and Registry around this plan:

```text
validated ExecutionPlan
→ atomically reserve concurrency slot
→ create immutable Job request and metadata
→ runner revalidates executable and clears environment
→ transient service owns runner cgroup
→ bounded byte logs and atomic result
→ release slot at durable terminal transition
```

No `job_start` MCP surface should be exposed before that chain is verified.
