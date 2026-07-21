# Ordivon Universal Executor M1

Status: supporting architecture evidence, not production activation.

Date: 2026-07-21

Branch: `agent/universal-executor-m1`

Base: `a0074e767944518183167833a17e59df7680729d`

## 1. Purpose

M1 is the first executable vertical slice after the M0 migration contract.
It proves that an LLM can create an isolated workspace, author a program,
start it as a durable Task, leave the initiating process, recover the Task
from a later process, read bounded Artifacts, inspect workspace changes, and
cancel a second Task.

M1 is performance-first. It deliberately uses a filesystem Task registry
instead of waiting for the future SQLite control plane. The filesystem layout
is sufficient to prove the interaction loop and collect the first real
measurements, but it is not accepted as the production registry.

M1 does not expose an MCP execution tool and does not change the production
Bridge, Cloudflare route, system service, or port 8811.
## 2. Agent-facing operations

The feature-gated local adapter exposes eight commands:

```text
workspace-create
workspace-read
workspace-write
workspace-diff
task-start
task-get
task-cancel
artifact-read
```

The adapter reads one strict JSON request from standard input and returns one
structured JSON outcome. Each invocation is a separate OS process. No state is
kept in the adapter process, which makes cross-call recovery an explicit part
of the proof rather than an in-memory illusion.

`task-start` accepts an absolute executable plus an argument vector. It does
not accept a shell command string. A model can still create a Bash, Python, or
other program inside its workspace and then execute the approved interpreter.
This supplies universal composition without making the protocol itself a
stringly typed shell API.

The binaries require the `universal-executor-m1` Cargo feature. Normal builds
and the current MCP server do not include or advertise them.
## 3. Workspace model

A workspace is a detached Git worktree created from an exact resolved commit.
The source repository remains outside the writable workspace. M1 records:

```text
workspace ID
canonical source repository
exact source commit
canonical workspace path
creation time
```

Workspace reads are UTF-8 and byte-bounded. Workspace writes use relative
paths and optional `expectedDigest` optimistic concurrency. Parent directories
are created component by component; existing symlinks and non-directory
parents are rejected before any nested path is created.

`workspace-diff` returns a bounded tracked Git diff and a separate bounded list
of untracked paths. The latter is essential for coding agents because generated
scripts, test outputs, and candidate tools frequently begin as untracked files.

M1 does not automatically commit, push, merge, or remove a user's source
branch. Test worktrees are removed explicitly during cleanup.

## 4. Task persistence

Each Task uses a deterministic directory under the configured store root:

```text
tasks/<task-id>/request.json
tasks/<task-id>/metadata.json
tasks/<task-id>/stdout.log
tasks/<task-id>/stderr.log
tasks/<task-id>/result.json
```
Cancellation also creates `cancel-requested.json`. The runner atomically writes
`result.json`; the terminal result remains authoritative after the transient
systemd unit is garbage-collected.

The systemd unit name is deterministic:

```text
ordivon-m1-<task-id>.service
```

A later CLI process reconstructs the Task from persisted metadata, systemd
observation, and the terminal result. `task-get` supports bounded local long
polling through `waitMs`, so waiting happens inside Ordivon instead of requiring
repeated model or MCP round trips.

M1 has no transactional idempotency table, concurrency reservation, Task list,
retention policy, or garbage collector. Those require the later registry phase.

## 5. Runner behavior

Before starting the target, the runner:

1. loads the persisted request;
2. canonicalizes the workspace and working directory;
3. requires the working directory to remain inside the workspace;
4. verifies that the executable is a non-symlink executable file;
5. recomputes and compares the executable SHA-256;
6. clears the inherited environment and applies only the request environment;
7. starts the target with null standard input and captured output pipes.

Stdout and stderr are always drained. Bytes above each retention limit are
discarded but counted, preventing the target from blocking on a full pipe.
Timeout, non-zero exit, cancellation, and infrastructure failure produce typed
terminal result data.
## 6. Minimum systemd sandbox

M1 retains a small non-negotiable safety kernel while postponing the complete
policy and identity system. The transient unit applies:

```text
NoNewPrivileges=yes
CapabilityBoundingSet=
AmbientCapabilities=
ProtectSystem=strict
PrivateNetwork=yes
PrivateDevices=yes
PrivateIPC=yes
PrivatePIDs=yes
ProtectProc=invisible
ProcSubset=pid
RestrictNamespaces=yes
RestrictAddressFamilies=AF_UNIX
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
ProtectHostname=yes
ProtectClock=yes
RestrictSUIDSGID=yes
LockPersonality=yes
TasksMax=128
MemoryMax=1 GiB
```

Only the Task workspace and Task metadata directory are writable. Common
systemd, D-Bus, Docker, credential, SSH, Cloudflare, AWS, Kubernetes, and Git
credential paths are made inaccessible.

The real test included a model-authored Python program that attempted to write
to `/etc` and invoke systemd control. Both operations were denied while normal
workspace writes succeeded.
## 7. Real vertical-journey evidence

The ignored root/systemd integration test used independent CLI processes. It
created a detached workspace, read and updated one file with digest binding,
wrote a model-authored Python tool, started a durable Task, observed it from a
later CLI process, and long-polled it to completion.

The same journey then read stdout and stderr Artifacts, read the generated
workspace output, inspected the tracked diff and untracked paths, started a
second Task, cancelled it, and removed all temporary units and workspaces.

The governed evidence run observed 2226 milliseconds end to end, 14 independent
CLI invocations, one `COMPLETED` Task, one `CANCELLED` Task, and three terminal
Artifacts. No residual unit, runner process, test store, or host escape file
remained after cleanup.
Before bounded long polling, the same development journey required 38 CLI
calls. Long polling reduced this to 14, approximately 63 percent. This is an
internal M1 polling comparison, not a Legacy Desktop Commander comparison.
Formal Legacy-versus-Ordivon performance measurement remains M2.

The machine-readable evidence is stored at:

`docs/audits/runtime/ordivon-universal-executor-m1-evidence-2026-07-21.json`

## 8. Known limitations

M1 intentionally retains the following debt:

- the Task registry is a directory tree, not SQLite or another transactional
  registry;
- Task creation has no atomic idempotency or global concurrency reservation;
- the runner executes under root UID inside a constrained systemd sandbox;
- workspace and Task write access are constrained, but read access is not yet
  a complete least-privilege filesystem view;
- executable verification narrows but does not fully eliminate path-based
  time-of-check/time-of-use risk;
- network is always disabled; scoped opt-in does not exist;
- binary Artifact reading and structured log summaries are not implemented;
- Task listing, retention, garbage collection, and orphan reconciliation are
  absent;
- cancellation fallback may reconstruct output metadata without the runner's
  exact dropped-byte counters if the runner is terminated before its result;
- no Legacy backend call or automatic fallback is wired;
- no MCP execution tool is exposed.
## 9. Result and next stage

M1 establishes that the new architecture can execute one real agent-authored
program through a durable, recoverable, bounded Task without depending on the
initiating CLI or MCP session. It also proves that large output can remain in
Artifacts rather than the model context.

M1 does not yet prove that Ordivon outperforms the Legacy Desktop Commander
route. The next stage is M2 differential evaluation: execute the same narrowly
specified coding journey through both backends, record M0 benchmark samples,
and compare completion, elapsed time, tool calls, remote round trips, context
bytes, output bytes, recovery, and fallback use.

No production traffic should move before that comparison exists. The M1 CLI and
runner remain feature-gated local evidence tools.
