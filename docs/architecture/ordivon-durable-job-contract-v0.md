# Ordivon Durable Job Contract v0

Status: contract freeze only
Authority: supporting architecture evidence
Runtime status: not implemented

## 1. Purpose

This document freezes the first typed contract for durable local jobs in
`ordivon-exec`. It does not implement process creation, systemd integration,
persistent files, authorization evaluation, MCP tools, or production routing.

The contract exists to prevent the implementation from starting with an
unbounded shell primitive and adding safety, recovery, and evidence later.

The target boundary remains:

```text
ChatGPT Host decides what operation is needed
-> MCP adapter validates protocol input
-> Ordivon Core resolves authority and capability
-> OS supervisor owns the process lifetime
-> Ordivon returns bounded state and evidence
```

Ordivon remains an execution plane, not a second agent or planning harness.

## 2. Non-negotiable invariants

1. Public job requests never contain a shell command string.
2. Public job requests never select an executable path.
3. A request selects a versioned `profileId` and supplies structured `args`.
4. An execution profile binds an absolute executable path and SHA-256 digest.
5. Argument vectors are exact allowlisted vectors in v0.
6. Environment overrides are deny-by-default and exact-value allowlisted.
7. CWD values and allowed roots are absolute; canonical scope checks remain a
   later enforcement concern.
8. Job state is persisted outside the MCP session.
9. Public state is derived from internal state and cannot drift independently.
10. Terminal jobs never transition back to a non-terminal state.
11. PID alone is not process identity; PID and process-start identity are bound.
12. Output retention and per-read output are independently bounded.
13. Raw process output is byte-oriented; callers choose UTF-8 lossy or Base64.
14. Operational receipt events are not comparator governance receipts.
15. `AI_WRITTEN` cannot be deserialized as an operational observation origin.
16. Process exit and recovery observations require `SYSTEM_OBSERVED` origin.
17. Contract existence does not authorize execution or prove runtime safety.

## 3. Public start request

`JobStartRequest` contains:

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

It intentionally excludes:

```text
command
shell
executable
PATH
stdin script
arbitrary environment inheritance
```

Unknown JSON fields are rejected. Shape validation enforces bounded argument,
environment, output, identifier, and absolute-path fields. Policy evaluation is
not part of P1.

## 4. Capability policy

`CapabilityPolicy` is versioned and deny-by-default. It contains global roots,
a global concurrency ceiling, and one or more `ExecutionProfile` objects.

Each execution profile binds:

```text
profileId
enabled
absolute executable
executable SHA-256
fixed argument prefix
exact allowed argument vectors
allowed CWD roots
exact environment rules
maximum runtime
stdout/stderr retention ceilings
profile concurrency ceiling
output-limit termination behavior
```

V0 deliberately uses exact argument vectors instead of regexes or arbitrary
value slots. The first intended profile can therefore express only:

```text
cargo build --release -p ordivon-mcp
```

without accidentally authorizing arbitrary Cargo subcommands.

`CapabilityPolicy::validate_shape` only validates the policy document. It does
not canonicalize paths, verify the executable currently matches its digest, or
evaluate a request. Those enforcement operations belong to Capability Scope
implementation after the supervisor spike.

## 5. Resolved execution plan

`ExecutionPlan` is an internal, fully resolved object. It binds:

- policy ID, version, and digest;
- profile ID;
- executable path and digest;
- final argv, CWD, and environment;
- runtime and output ceilings;
- client request ID and request digest;
- authenticated principal and authority reference.

Only a policy evaluator may construct a valid execution plan. MCP request data
must not be treated as an execution plan.

## 6. State model

### 6.1 Internal states

```text
accepted
starting
running
stopping
recovering
succeeded
failed
timed_out
cancelled
lost
orphaned
```

### 6.2 Public states

```text
queued
running
succeeded
failed
timed_out
cancelled
lost
orphaned
```

Projection:

| Internal | Public |
|---|---|
| accepted, starting | queued |
| running, stopping, recovering | running |
| succeeded | succeeded |
| failed | failed |
| timed_out | timed_out |
| cancelled | cancelled |
| lost | lost |
| orphaned | orphaned |

### 6.3 Allowed transitions

```text
accepted  -> starting | recovering | failed | lost
starting  -> running | recovering | failed | lost | orphaned
running   -> stopping | recovering | succeeded | failed | timed_out | lost | orphaned
stopping  -> recovering | cancelled | failed | lost | orphaned
recovering -> starting | running | stopping | succeeded | failed |
              timed_out | cancelled | lost | orphaned
```

All terminal states reject every later transition. Retry creates a new Job ID.

Definitions:

- `lost`: Ordivon cannot prove whether the expected process still exists.
- `orphaned`: the process or supervisor object exists, but Ordivon cannot prove
  that it remains bound to the recorded authority and job identity.

## 7. Job record

`JobRecord` binds identity, authorization evidence, lifecycle state, supervisor
identity, process identity, terminal result, and both output streams.

Important validation rules:

- a terminal job requires `finishedAt` and `terminationReason`;
- a non-terminal job cannot contain terminal metadata;
- running/stopping require `startedAt` and `unitName`;
- `runnerPid` and `processStartIdentity` appear together;
- succeeded requires `exitCode = 0`;
- dropped output bytes require `truncated = true`;
- request, policy, executable, output, and detail digests use `sha256:<64 hex>`.

P1 does not select a Job ID generator. P4 is expected to use UUIDv7 or an
equivalent time-ordered opaque identifier.

## 8. Output and cursor contract

Each output stream has:

```text
generation
retainedBytes
droppedBytes
truncated
digest
```

A `JobOutputCursor` is a structured stable cursor bound to:

```text
schemaVersion
jobId
stream
generation
byteOffset
```

It is not a bare byte offset and cannot be reused across jobs or streams.

`JobReadRequest` limits one response to at most 1 MiB. Total retained stdout and
stderr are each capped by the contract at 64 MiB. A later profile may choose a
smaller ceiling.

When retention is exhausted, the intended runtime behavior is:

```text
continue draining the child pipe
-> stop growing the retained log
-> count dropped bytes
-> mark truncated
-> either continue or terminate according to the profile
```

The child must never block indefinitely because Ordivon stopped consuming its
output.

## 9. List contract

`JobListRequest` is bounded to 200 records and uses a structured cursor bound to
`createdAt + jobId`. It supports public-state and time filters. A runtime must
never return an unbounded historical job list.

## 10. Error taxonomy

Stable wire error codes are SCREAMING_SNAKE_CASE:

```text
INVALID_REQUEST
POLICY_INVALID
PROFILE_NOT_FOUND
PROFILE_DISABLED
ARGUMENT_POLICY_DENIED
PATH_SCOPE_DENIED
INVALID_CWD
ENVIRONMENT_DENIED
TIMEOUT_EXCEEDS_POLICY
OUTPUT_LIMIT_EXCEEDS_POLICY
CONCURRENCY_LIMIT
JOB_NOT_FOUND
JOB_ALREADY_TERMINAL
JOB_STATE_CONFLICT
SUPERVISOR_UNAVAILABLE
RUNNER_START_FAILED
JOB_METADATA_CORRUPT
CURSOR_INVALID
OUTPUT_NOT_RETAINED
IDEMPOTENCY_CONFLICT
```

Runtime layers may map these errors into MCP `isError=true`, but P1 does not add
MCP job tools.

## 11. Operational receipt events

`OperationalReceiptEvent` is append-only execution evidence. It is intentionally
separate from `ordivon-kernel::Receipt`, which is comparator-issued governance
evidence.

Operational event types are:

```text
REQUEST_RECEIVED
AUTHORIZATION_ALLOWED
AUTHORIZATION_DENIED
JOB_RECORD_CREATED
RUNNER_UNIT_STARTED
RUNNER_STARTED
PROCESS_STARTED
OUTPUT_LIMIT_REACHED
TIMEOUT_TRIGGERED
STOP_REQUESTED
PROCESS_EXITED
JOB_TERMINAL
RECOVERY_OBSERVED
RECOVERY_FAILED
```

Origins are restricted to:

```text
SYSTEM_OBSERVED
SYSTEM_DERIVED
```

There is no `AI_WRITTEN` variant. Runtime observations such as process exit must
be `SYSTEM_OBSERVED`. Derived state projections may be `SYSTEM_DERIVED` if they
reference the observed evidence from which they were calculated.

An authorization denial may be recorded without creating a Job ID. Events after
job-record creation require a Job ID.

Operational events do not:

- authorize an operation;
- prove a requested business outcome;
- replace an `ObservedEvent` emitted by a trusted adapter;
- replace comparator evaluation;
- close governance debt;
- claim the user objective was achieved.

## 12. Threat model frozen for implementation

The following cases must be addressed before `job_start` is exposed:

1. relative path and symlink escape from allowed roots;
2. executable replacement after policy validation;
3. PATH, `LD_PRELOAD`, wrapper, toolchain, and environment injection;
4. dangerous subcommand or argument escape inside an otherwise allowed program;
5. build scripts, proc macros, hooks, and other child execution;
6. descendants calling `setsid` or otherwise detaching from a parent process;
7. child and grandchild process leakage after stop;
8. unbounded stdout/stderr and pipe backpressure;
9. partial metadata writes and crash consistency;
10. PID reuse;
11. missing supervisor unit with a still-running process;
12. duplicate client request IDs and double dispatch;
13. concurrent stop requests;
14. concurrent log append and cursor reads;
15. Core restart during accepted, starting, running, and stopping;
16. WSL restart and supervisor-state reconstruction;
17. forged runtime observations;
18. a trace or operational event being treated as a governance receipt.

## 13. P1 implementation evidence

P1 adds pure data types, validation, serialization contracts, JSON Schema
derivation, state-transition logic, and adversarial unit tests to
`ordivon-exec`.

P1 explicitly does not add:

- `std::process::Command` use for jobs;
- systemd or cgroup calls;
- `/var/lib/ordivon/jobs` writes;
- persistent registry behavior;
- authorization evaluation;
- Capability Policy activation;
- `job_start`, `job_stop`, `job_status`, `job_read`, or `job_list` MCP tools;
- production service changes;
- release builds through a durable job.

## 14. Exit criteria and next phase

P1 is complete when:

1. all public request types reject unknown fields;
2. start requests cannot carry a shell command or executable;
3. policy profiles bind exact executable and argument authority;
4. state transitions are explicit and terminal reopening is impossible;
5. records bind process identity beyond PID;
6. read/list outputs are bounded and cursor-scoped;
7. AI-written operational observation cannot deserialize;
8. unit, workspace, Clippy, format, and diff checks pass;
9. document governance introduces zero new violations relative to the exact base;
10. an evidence-bound draft engineering receipt records remaining debt.

The next phase is a supervisor spike. It must prove systemd/cgroup lifecycle
semantics before the persistent Job Registry is implemented.
