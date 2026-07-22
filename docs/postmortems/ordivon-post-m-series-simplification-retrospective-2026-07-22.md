# Ordivon Post-M-Series Simplification Retrospective

Date: 2026-07-22  
Baseline: `8249c474362257973a717d891ac465e37b9e7418`  
Reviewed head: `3351338bfd0d39d169278031da0204ca71de5a76`

## Evidence labels

- **Observed**: directly established by the execution trace, repository state, commands, or test output.
- **Inferred**: a conclusion derived from multiple observed facts.
- **Proposed**: a decision or action for a later stage.
- **Unknown**: current evidence is insufficient.

Evidence strength is ordered as follows: formal benchmark, real integration or smoke test, unit test, static review, architecture inference. Passing tests are not treated as production maturity, and local measurements are not extrapolated to all workloads.

## 1. Task overview and goal review

The task was a post-M-series structural reduction, not a feature expansion. Its goals were to remove governance platforms, proof machinery, parallel runtime generations, and infrastructure that no longer served the current execution path; retain the minimum execution truth required after crashes and races; converge on one MCP server and one runtime vocabulary; make trusted local execution the low-friction default; and preserve rollback and the user's existing work.

Success required an isolated worktree and recovery point, logically separated commits, preservation of Job and Attempt truth, idempotency, concurrency reservations, systemd and cgroup ownership, bounded execution, cancellation, artifacts, reconciliation, and ambiguous-dispatch fail-closed behavior. It also required a buildable and testable final tree without pushing or merging it.

**Observed:** the task reduced the tracked tree from 461 files and 100,331 lines to 88 files and 18,927 lines. The final diff contains 2,422 additions and 83,845 deletions across 436 files, for a net reduction of 81,423 lines.  
**Inferred:** the structural simplification goal was highly achieved.  
**Unknown:** full production readiness and workload-wide performance improvement were not established.

Overall completion at the reviewed simplification head was assessed at approximately 88%: architecture reduction and rollback safety were strong, while mainline adoption, real systemd acceptance, full MCP end-to-end validation, compatibility review, and formal benchmarks remained incomplete at that time.

## 2. Execution process review

### 2.1 Material stages

1. **Recovery and isolation.** The current repository was on `main@1e6a219...` and contained an existing modification to `.vscode/settings.json`. A recovery tag was created at the M-series closure commit, then branch `agent/post-m-series-simplification` and worktree `/root/projects/ordivon-simplify` were created. **Observed:** the user's modification remained untouched.
2. **Default-friction removal.** Governance-oriented CI and default entry gates were removed before deleting their implementations. **Inferred:** cutting the entry path first reduced interference from obsolete checks during the following batches.
3. **Python governance and infrastructure removal.** The Python governance platform and its PostgreSQL, NATS, Temporal, OpenFGA, OPA, Redis, and DuckDB support were removed after consumer inspection. **Observed:** this batch removed about 175 files and 22,827 lines.
4. **M-series proof machinery removal.** Stage evidence, receipts, benchmark, shadow, dogfood, wire, reboot, and historical harnesses were removed while retaining the one currently working HTTP implementation.
5. **Lifecycle reduction.** Quota, hold, GC, tombstone, core backup/restore, lifecycle migration, and the separate remediation ledger were removed. Job/Attempt terminal truth, reservations, reconciliation, and ambiguous-dispatch behavior remained.
6. **Execution-mode simplification.** `trusted-local` became the default and `isolated` remained explicit for untrusted inputs. **Observed:** the isolation-specific worker and filesystem boundary were retained.
7. **MCP convergence.** Stdio, M4, M6, and M7 server variants were replaced by one `ordivon-mcp` Streamable HTTP server exposing nine tools.
8. **Second consumer audit.** Unused Rust governance crates, the migration comparator, file task registry, M1 CLI, old executor matrices, and versioned M-series runtime names were removed.
9. **Operational minimum.** Bounded smoke, conservative cleanup, and SQLite-consistent backup helpers were added. A real smoke test exposed a `PrivateTmp` path-boundary issue when control state was placed under `/tmp`; persistent paths resolved it.
10. **Final active-reference cleanup.** Current README, runbook, architecture, and policy references were aligned with the single runtime and server.

Mechanical file removal, repeated search, formatting, and ordinary test invocations are intentionally aggregated here rather than listed command by command.

### 2.2 Important decisions

- **Observed:** the work used an isolated worktree rather than modifying a dirty current-main checkout. **Inferred:** this was the strongest risk-control decision in the task.
- **Observed:** systems were removed after checking current consumers rather than by directory name alone. **Inferred:** this prevented obvious removal of live runtime dependencies.
- **Observed:** reliability mechanisms tied to real execution ambiguity and process ownership were retained. **Inferred:** simplification did not collapse into an unsafe stateless shell wrapper.
- **Observed:** phase names were removed from long-lived binaries, modules, features, and schemas. **Inferred:** this reduced historical project structure leaking into permanent architecture.
- **Observed:** no push, PR, or merge occurred. **Inferred:** the branch is suitable for independent review but not yet integrated.

### 2.3 Failures and handling

- A type-name collision appeared when MCP and execution request types were unified. **Observed:** explicit aliases resolved it without reintroducing old stage names.
- Removing task and migration layers exposed residual constants and helpers. **Observed:** compilation failures were used to continue shrinking the obsolete surface.
- MCP smoke initially failed because systemd `PrivateTmp` hid paths under `/tmp`. **Observed:** using a persistent root visible to both service boundaries fixed startup. **Inferred:** this was a real integration finding not discoverable from unit tests alone.
- Gitleaks initially flagged a stale `target/` metadata file from a deleted cryptographic dependency. **Observed:** after clearing build artifacts and scanning the current source, no leak remained. **Proposed:** source secret scans should explicitly target tracked content.
- Active README and execution-policy references were stale until the final review. **Observed:** they were corrected before the branch was declared clean. **Inferred:** compile success alone is insufficient after a broad architecture rename.

### 2.4 Resource and method efficiency

**Observed:** the work produced ten logical commits, used Git isolation, consumer searches, Cargo feature matrices, Ruff, strict Clippy, Gitleaks, and a real MCP initialize smoke.  
**Inferred:** the method was effective but produced repetitive verification output and relied on compiler failures to reveal some residual dependencies.  
**Proposed:** a single acceptance command and a pre-removal dependency inventory would reduce repeated work and late discoveries.

## 3. Results and metrics

| Metric | Before | After | Change |
|---|---:|---:|---:|
| Tracked files | 461 | 88 | -373 (-80.9%) |
| Tracked lines | 100,331 | 18,927 | -81,404 (-81.1%) |
| Diff | - | 436 files | +2,422 / -83,845 |
| Logical commits | - | 10 | reviewable sequence |
| Default tests | - | 88 passed | 10 real-system tests ignored (2 supervisor + 8 transactional) |
| Transactional runtime | - | 77 passed | 8 ignored |
| Isolated execution | - | 81 passed | 8 ignored |
| Strict Clippy | - | passed | `-D warnings` |
| Ruff | - | passed | scripts only |
| Secret scan | - | no current-source leak | after removing stale build output |
| MCP initialize smoke | - | passed | protocol `2025-06-18` |

**Observed:** the workspace now contains only `ordivon-exec` and `ordivon-mcp`.  
**Observed:** the public server exposes `workspace.open`, `workspace.read`, `workspace.mutate`, `workspace.diff`, `workspace.exec`, `task.observe`, `task.cancel`, `task.list`, and `artifact.read`.  
**Inferred:** the active architecture is materially easier to understand and maintain.  
**Unknown:** performance, long-running stability, complete external compatibility, and production safety.

## 4. Comparison analysis

### 4.1 Initial plan versus actual path

The execution followed the planned direction: establish recovery, remove default gates, remove governance infrastructure, remove proof machinery, simplify runtime policy, converge servers, and finish with minimal operations and documentation.

Material deviations were:

- **Observed:** deletion expanded beyond the initial Python and proof layers to unused Rust governance crates and old runtime layers. **Inferred:** this was justified by consumer evidence but increased review scope.
- **Observed:** implementation was based on the M-series closure commit rather than current `main`. **Inferred:** this improved isolation but deferred integration risk.
- **Observed:** only MCP initialization was smoke-tested; the complete tool and task journey was not executed on the final head.
- **Observed:** no formal before/after benchmark was performed. **Unknown:** the claim of higher runtime performance remains unproven.

### 4.2 Alternatives

Keeping obsolete systems behind feature flags would have reduced deletion risk but preserved dependencies, maintenance burden, and accidental reactivation paths. A clean rewrite would have looked smaller but discarded previously proven execution semantics. A long sequence of small PRs would have reduced review size but risked leaving the repository in a prolonged dual-system state.

**Inferred:** retaining the proven execution core while deleting inactive platforms was preferable to both feature-flag preservation and a clean rewrite. The main weakness is that a 436-file branch remains expensive to review even with ten logical commits.

### 4.3 Mature-practice comparison

Strong practices included isolated worktrees, a fixed recovery point, logical commits, consumer-based deletion, module convergence, and preservation of crash/race truth. Weak or incomplete practices include current-main integration, external-caller inventory, exact-head remote CI, independent review, complete real E2E, formal performance benchmarking, recovery drills, and an updated remote-exposure threat model.

## 5. Success factors and lessons

### 5.1 Strong results

1. **Worktree isolation.** **Observed:** user changes were preserved. **Inferred:** this made a high-deletion task reversible. **Proposed rule:** any core change exceeding 50 files must use a recovery reference and isolated worktree.
2. **Consumer-driven removal.** **Observed:** both Python and Rust governance systems were checked for current consumers. **Inferred:** deletion was evidence-led rather than aesthetic. **Proposed rule:** inventory entry points, state, deployment consumers, tests, and documents before removing a subsystem.
3. **Retention of execution truth.** **Observed:** Job/Attempt, reservations, process ownership, cancellation, artifacts, and reconciliation remained. **Inferred:** these are operational safety mechanisms rather than governance ceremony.
4. **Integration failure treated as evidence.** **Observed:** the `/tmp` smoke failure produced a durable `PrivateTmp` deployment rule.
5. **Active-reference scan.** **Observed:** stale README and policy paths were corrected. **Proposed rule:** every architecture rename requires a full-tree scan over active docs, environment variables, policies, and packaging.

### 5.2 Start / Stop / Continue

**Start**

- Build one release-acceptance command covering static checks, feature matrices, real systemd tests, MCP E2E, and backup/restore.
- Maintain a public entry-point and external-caller inventory.
- Require current-main replay before integration of work developed from a historical closure point.
- Benchmark build time, binary size, initialization, and execution journey rather than inferring performance from deleted lines.

**Stop**

- Treating green unit tests as production maturity.
- Using stage numbers in permanent modules, binaries, features, and schemas.
- Scanning untracked build products as if they were source.
- Removing public entry points without an external-consumer check.
- Preserving inactive systems only because they may be useful later.

**Continue**

- Isolated worktrees and logical commits.
- Consumer-graph reasoning.
- Fail-closed treatment of ambiguous external execution.
- Separate trusted-local and isolated modes.
- Git tags as the archive for deleted implementation history.

### 5.3 Root causes

The real systemd suite remained ignored because it had been organized as stage proof rather than a stable long-term acceptance target. Mainline adoption remained unresolved because the candidate contained a 45-commit capability chain beyond `main` before the simplification tail, requiring review of the complete net architecture rather than an ordinary replay. Documentation drift appeared late because active operational references were not maintained as an explicit inventory.

## 6. Limitations, risks, and improvements

| ID | Finding | Evidence | Risk |
|---|---|---|---|
| R1 | Final head did not execute all real systemd/cgroup tests | Observed | process, recovery, cancellation, and cleanup regressions may remain |
| R2 | No complete MCP tool/task journey | Observed | initialize success may hide tool-chain failures |
| R3 | The branch contains a 45-commit not-yet-main capability chain before simplification | Observed | reviewers may evaluate only the deletion tail and miss the complete net adoption |
| R4 | No external API/caller inventory | Unknown | old binaries or feature names may still be consumed |
| R5 | `trusted-local` broadens the default trust boundary | Observed | untrusted code may have greater host impact |
| R6 | Host/Origin checks are not default gates | Observed | a future proxy or non-loopback deployment may violate assumptions |
| R7 | Default CI omits strict Clippy, real E2E, and dependency checks | Observed | merge-time regressions can escape fast CI |
| R8 | No formal benchmark | Observed | performance claims cannot be made |
| R9 | Backup and cleanup have only bounded self-tests | Observed | restore and concurrent-operation behavior remain unproven |
| R10 | No independent reviewer | Observed | the implementation is primarily self-validated |
| R11 | Very large total diff | Observed | semantic review is difficult |
| R12 | No production or sustained-load evidence | Unknown | production maturity remains unknown |

Actionable improvements are to create a layered acceptance workflow, replay the commits onto current main, execute real systemd and MCP journeys, audit external consumers, document the trust and network boundaries, perform a real backup/restore drill, and establish benchmark baselines.

## 7. Reproducibility, documentation, and maturity

Code-level reproducibility is strong because the baseline, tag, branch, worktree, and ten commits are known. Environment-level reproducibility is weaker because real tests depend on Arch Linux/WSL2, root, systemd, cgroup v2, runner binaries, identities, and persistent paths visible across `PrivateTmp`.

Current documentation covers the runtime, operating boundary, runbook, MCP surface, operational scripts, and historical closure. Missing documentation includes a simplification ADR, a breaking-changes and compatibility guide, a current deployment topology, a complete acceptance report, and a tested restore procedure.

**Observed:** modularity, version-control isolation, bounded execution, typed state, and error handling are strong.  
**Observed:** current-main replay, external compatibility, real final-head integration, and independent review are incomplete.  
**Unknown:** production stability and workload-wide performance.

The current state should be described as a reviewable simplified-runtime candidate, not a production-ready runtime.

## 8. Overall assessment

Overall score: **8.0/10**.

Architecture convergence, deletion discipline, recovery safety, and code quality were strong. The score is reduced by the historical baseline, incomplete real E2E, absent compatibility audit, lack of formal benchmarks, and absence of an independent reviewer.

The project's active boundary is now substantially clearer:

```text
Ordivon = local agent execution runtime + MCP adapter
```

rather than a combined governance, proof, infrastructure, migration, and execution platform. **Inferred:** this materially improves the feasibility of future dogfooding and runtime iteration. **Unknown:** direct task-success and performance gains require measurement.

## 9. Follow-up actions and retained knowledge

### 9.1 Debt classification

**Should have been completed in this stage**

- Final-head real systemd/cgroup suite.
- Complete MCP end-to-end journey.
- Breaking-change and compatibility inventory.
- Mainline-adoption decision for the complete `main..HEAD` net change.
- Independent review.
- Machine-readable final acceptance manifest.
- Real Registry backup/restore drill.

**Newly discovered and required for the next stage**

- `PrivateTmp` path visibility constraints.
- Trusted-local versus isolated selection rules.
- Host/Origin behavior under the Cloudflare/Tunnel topology.
- Repository-external service and bridge consumers.
- Separation of fast CI from merge/release acceptance.
- Mainline adoption of a 56-commit descendant chain without reviewing transient historical architecture as current design.

**Long-term structural debt**

- Manual opt-in for real systemd acceptance.
- Linux/systemd/path coupling.
- No sustained-load or restart stress suite.
- No versioned public compatibility policy.
- No continuous performance baseline.
- No formal remote-exposure threat model.

**Explicit non-goals**

Do not restore the Python governance platform, governance databases and brokers, wording or document gates, receipts and stage proof machinery, lifecycle quota/hold/GC/tombstone systems, parallel MCP servers, M8 naming, Kubernetes, distributed scheduling, multi-tenant authority infrastructure, or speculative subsystems without a current consumer and demonstrated need.

### 9.2 Priority assessment

| ID | Severity | Probability | Cost | Stage |
|---|---|---|---|---|
| R1 | High | Medium | Medium | next-stage P0 before merge |
| R2 | High | High | Medium | next-stage P0 |
| R3 | High | High | Medium | next-stage P0 |
| R4 | High | Medium | Medium | next-stage P0 |
| R5 | High | Medium | Medium | next-stage P1 |
| R6 | High | Low-Medium | Medium | P1, and P0 before remote exposure |
| R7 | Medium | Medium | Low-Medium | next-stage P1 |
| R8 | Medium | High | Medium | next-stage P1 |
| R9 | Medium | Medium | Low | next-stage P1 |
| R10 | High | Medium | Medium | next-stage P0 |
| R11 | Medium | High | Medium | current review stage |
| R12 | High | High | High | long-term; does not block local candidate status |

### 9.3 Decision and action output

**Decision: continue into an integration and acceptance stage; do not continue feature expansion.**

P0 actions:

1. Treat `main..HEAD` as the adoption surface, document that `main` is an ancestor, and review the final net architecture plus the logical commit history before opening any merge path.
2. Execute all real systemd/cgroup integration tests on the replayed exact head and retain raw logs.
3. Execute a complete MCP journey covering all nine tools, durable task observation, restart recovery, artifact digest, cancellation, and listing.
4. Audit repository-external callers: systemd units, Cloudflare Tunnel, MCP bridge, connector configuration, scripts, binaries, and environment variables.
5. Perform an independent read-only review by a context separate from the implementing agent.

P1 actions:

- Add a release-acceptance command and manifest.
- Add a trusted-local/isolated decision matrix.
- Document the remote network threat boundary.
- Run a real backup/restore drill.
- Establish build, binary-size, initialization, and journey benchmarks.
- Add merge-time strict Clippy, feature matrices, dependency checks, and E2E without restoring high-friction default development gates.

Do not add agent loops, new tools, Hermes integration, remote multi-tenancy, distributed registries, Kubernetes, a new governance framework, UI, lifecycle platforms, or unrelated performance refactors during this stage.

Retained reusable rules:

1. A dirty workspace or historical target baseline requires a recovery tag, isolated branch, and isolated worktree before broad deletion.
2. A subsystem may be deleted only after current entry points, state dependencies, deployment consumers, tests, and historical recovery have been identified.
3. Unit tests do not prove systemd execution; MCP initialization does not prove a tool journey; static scans do not prove runtime safety; deleted lines do not prove performance.
4. With systemd `PrivateTmp`, registry, store, workspace, and attempt bundles must not depend on `/tmp` paths created outside the runner namespace.
5. Completed project-stage identifiers must not remain in permanent runtime, binary, feature, or schema names.
6. Verification should be layered into fast development CI, merge acceptance, and release validation.

## 10. One-sentence summary

The task removed roughly 81% of the active engineering footprint while preserving the minimum execution truth required for reliable local agent work; the next stage must validate and independently review the complete `main..HEAD` adoption surface, then close real systemd, complete MCP journey, compatibility, restore, and deployment-boundary gaps before any new capability is added.
