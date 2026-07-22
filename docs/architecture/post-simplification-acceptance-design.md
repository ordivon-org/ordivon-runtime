# Post-Simplification Acceptance and Mainline Adoption Design

Status: accepted for local implementation  
Date: 2026-07-22  
Evidence branch: `agent/post-m-series-simplification`  
Design input: `docs/postmortems/ordivon-post-m-series-simplification-retrospective-2026-07-22.md`

## 1. Decision

The next stage is an **integration and acceptance stage**, not another architecture or feature stage.

No new Agent loop, Tool, Skill, remote multi-tenant layer, governance platform, scheduler, database, UI, or speculative isolation mechanism may enter this stage. Work is admitted only when it closes a documented acceptance, compatibility, recovery, or deployment-boundary gap.

## 2. Correct branch topology

The repository topology is:

```text
main@1e6a219
  -> 45 M-series capability and closure commits
  -> 10 simplification commits
  -> retrospective and acceptance work
```

`main` is an ancestor of the M-series closure point and of this branch. There is no newer-main rebase problem. The adoption problem is that the complete surviving runtime has not entered main.

The primary review surface is therefore the **net diff `main..candidate-head`**, supported by the logical history. Reviewers must not evaluate only the deletion commits, because the runtime and MCP implementation are also new relative to main.

## 3. Goals

1. Convert previously ignored real-system tests into an explicit, repeatable acceptance profile.
2. Prove a real MCP journey rather than only protocol initialization.
3. Verify online SQLite backup and non-destructive restore into a new destination.
4. Inventory repository and workstation consumers of removed binaries, features, paths, and environment variables.
5. Define trusted-local, isolated, and remotely proxied transport boundaries.
6. Produce a machine-readable exact-head acceptance manifest.
7. Preserve fast ordinary development while making merge acceptance explicit.
8. Prepare the branch for an independent read-only review without pushing or merging it.

## 4. Non-goals

- Production-readiness certification.
- Sustained load, multi-host, or multi-tenant claims.
- Restoring historical governance, receipts, lifecycle, or M-series proof frameworks.
- Benchmarking every workload.
- Automatically modifying live systemd or Cloudflare configuration.
- Squashing or rewriting the existing evidence history.
- Publishing, opening a PR, or merging without a separate decision.

## 5. Verification model

### 5.1 Fast profile

Runs on ordinary changes:

```text
cargo fmt --check
cargo test --workspace
ruff check scripts/
tracked-source secret scan
```

This profile optimizes feedback speed. It is not merge acceptance.

### 5.2 Merge profile

Runs before review completion or merge:

```text
fast profile
strict Clippy
transactional-runtime feature matrix
isolated-execution feature matrix
script unit tests
MCP transport and full tool journey
backup/restore drill
legacy-reference scan
```

### 5.3 Release profile

Runs only on an environment with root, systemd, cgroup v2, and a built Runner:

```text
merge profile
all ignored systemd supervisor tests
all ignored transactional runtime tests
server restart and durable task observation
remote-boundary checks when a proxy or Tunnel is in scope
bounded benchmark snapshot
```

A release-profile result applies only to the recorded machine, revision, configuration, and workload.

## 6. Acceptance runner

Add `scripts/acceptance.py` with these properties:

- profiles: `fast`, `merge`, `release`;
- dry command plan via `--list`;
- exact Git revision and dirty-state recording;
- environment preflight for release-only requirements;
- fail-fast command execution;
- JSON manifest with command, exit code, duration, and bounded output tails;
- output directory chosen explicitly by the caller;
- no source mutation other than normal build output;
- release profile requires an explicit opt-in flag.

The manifest is evidence, not a governance authority. It records what ran; it does not infer production maturity.

## 7. MCP end-to-end journey

Add `scripts/mcp_e2e.py`. It must launch or target the current `ordivon-mcp` and use the public Streamable HTTP protocol. The minimum success journey is:

1. initialize;
2. tools/list and exact nine-tool catalog check;
3. create a temporary Git source repository;
4. `workspace.open` at an exact revision;
5. `workspace.read` in FULL and SLICE mode;
6. `workspace.mutate` using WRITE, APPEND, and REPLACE_EXACT across distinct paths;
7. `workspace.diff` and untracked-file verification;
8. `workspace.exec` with a bounded command;
9. `task.observe` until terminal;
10. `task.list` and native MCP task listing;
11. `artifact.read` with digest verification;
12. submit a long-running task and `task.cancel` it;
13. restart the MCP server and observe the previously completed Job from SQLite truth.

The script must clean only its own temporary source, store, registry, and server processes. It must use persistent paths rather than `/tmp` for state passed into systemd `PrivateTmp`.

## 8. Backup and restore

Keep `backup.py` as the online-copy producer and add `restore.py` with conservative semantics:

- source must be a completed snapshot containing `manifest.json` and `registry.sqlite3`;
- every manifest digest and byte length is verified before restore;
- destination must not exist;
- symlinks and unsupported file types are rejected;
- the restored SQLite database must pass `PRAGMA integrity_check`;
- restore writes to a sibling staging directory and atomically renames it;
- no in-place overwrite or live service stop/start is performed.

Add a bounded drill that creates Registry state, backs it up, restores it to a new root, and compares integrity and selected rows.

## 9. Compatibility and external-consumer audit

The removed public surface includes historical stdio/M4/M6/M7 binaries, stage-numbered features, Python governance commands, governance services, and old environment/path names.

The audit has two parts:

1. **Repository scan:** fail merge acceptance when active files outside historical closure documents contain removed entry-point names.
2. **Workstation scan:** read-only inspection of systemd units, service environments, Cloudflare/Tunnel configuration, bridge commands, and active processes. Findings are recorded in a dated audit document; the script must not rewrite live configuration.

Every finding receives one disposition: current, migrate, remove, historical, or unknown owner.

## 10. Execution and network boundary

### trusted-local

Use only for user-owned, Git-backed repositories and reversible local commands. It retains runtime bounds, cgroup ownership, Job/Attempt truth, idempotency, and reconciliation but does not provide a separate Unix identity or private filesystem view.

### isolated

Use for downloaded repositories, unknown scripts, dependency experiments, parser/fuzzer inputs, and security work. It requires the static worker identity and isolated roots.

### remote proxy or Tunnel

Loopback binding and bearer authentication remain necessary but are not sufficient evidence once a remote proxy is in scope. Before remote exposure:

- identify the trusted proxy hop;
- prevent direct non-loopback listener exposure;
- define allowed Host/Origin behavior appropriate to the actual client;
- confirm bearer-token storage and rotation;
- bound request size and connection lifetime;
- verify that proxy logs do not record credentials;
- document Cloudflare Access and Tunnel responsibility separately from Runtime authorization.

Remote exposure is blocked until the topology audit is complete.

## 11. CI design

Keep `.github/workflows/ci.yml` as the fast profile. Add a manually dispatchable and reusable `release-acceptance.yml` that runs merge-safe checks on GitHub and documents that real systemd tests require the local WSL runner.

The workflow may enforce strict Clippy, feature matrices, Python tests, reference scans, and backup/restore drills. It must not pretend GitHub-hosted runners can prove the local WSL/systemd boundary.

## 12. Mainline adoption strategy

Do not create an intermediate PR that first adds all historical M-series implementations and then a second PR that deletes most of them. That would expose transient architecture as if it were a product migration path.

The existing branch remains the evidence and development history. After acceptance and independent review, choose one of two explicit integration forms:

- **Preserved-history PR:** direct branch to main, reviewed by final net diff plus logical commits.
- **Clean-adoption PR series:** reconstruct the final net state from main into a small number of stacked branches: runtime/MCP adoption, legacy removal, and operations/acceptance.

The second form improves review ergonomics but must preserve the evidence branch and closure tag. The choice is deferred until the independent audit reports whether the final net diff can be reviewed safely as one PR.

## 13. Required deliverables

- corrected retrospective topology;
- this design document;
- acceptance runner and tests;
- full MCP E2E runner;
- restore utility and backup/restore drill;
- compatibility-reference scanner;
- remote/deployment boundary document;
- dated workstation consumer audit;
- merge/release workflow;
- exact-head acceptance manifest;
- final gap closure report.

## 14. Exit criteria

This stage is complete only when:

1. the branch is clean and exact head is recorded;
2. fast and merge profiles pass;
3. all real systemd tests execute, not merely compile or remain ignored;
4. the MCP journey passes through all nine tools, cancellation, restart, and artifact verification;
5. backup/restore drill passes;
6. repository legacy-reference scan passes;
7. workstation external-consumer findings have dispositions;
8. the network boundary is documented and remote exposure is either verified or explicitly blocked;
9. no new feature scope entered the branch;
10. an independent reviewer can reproduce the commands and inspect bounded evidence.
