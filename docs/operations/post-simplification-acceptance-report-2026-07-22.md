# Post-Simplification Acceptance Report — 2026-07-22

## Status

The post-simplification implementation reached a locally validated acceptance candidate at commit `1253524fa90950cf6dea139bf0fc89ff8dd1b802`.

**Observed:** all commands in the `release` acceptance profile passed on Arch Linux under WSL2 with systemd 261 and cgroup v2.
**Inferred:** the previously identified local execution, MCP journey, restore, static-reference, and acceptance-automation gaps are substantially closed.
**Unknown:** production operation, sustained load, Cloudflare deployment of the Rust server, and independent review remain unproven.

The original retrospective incorrectly counted eight real-system tests. The exact suite contains ten: two systemd-supervisor tests and eight transactional-runtime tests. The retrospective has been corrected.

## Exact evidence

Primary manifest:

```text
/root/.local/share/ordivon-acceptance/release-1253524/manifest.json
```

The manifest records the exact Git head, branch, clean state, command arguments, exit codes, durations, and bounded output tails. It is external execution evidence and is not committed as project truth.

## Acceptance results

| Layer | Result | Evidence boundary |
|---|---:|---|
| `cargo fmt --check` | passed | formatting only |
| `cargo test --workspace` | 88 passed; 10 real tests ignored in the ordinary profile | unit and local non-opt-in tests |
| Ruff | passed | Python static lint only |
| exact tracked-tree Gitleaks | passed | current committed tree only |
| strict Clippy | passed | Rust static analysis with warnings denied |
| transactional feature matrix | 77 passed; 8 real transactional tests ignored in this profile | feature-specific unit coverage |
| isolated feature matrix | 81 passed; 8 real transactional tests ignored in this profile | isolation-enabled unit coverage |
| operational script tests | 4 passed | backup/restore, digest rejection, reference scan, command planning |
| legacy-reference scan | zero active findings | tracked active files; historical summaries exempted |
| real MCP E2E | passed, 17 named checks | public Streamable HTTP, real systemd execution |
| real systemd supervisor | 2/2 passed | cgroup ownership, stop, terminal evidence, unit cleanup |
| real transactional runtime | 8/8 passed | execution, replay, restart, cancel, ambiguous dispatch, orphaning, bundle rebuild, corrupt result, fast failure |
| bounded benchmark | completed | current machine and loopback initialize only |

## MCP journey

The public-protocol journey proved:

1. protocol `2025-06-18` initialization;
2. the exact nine-tool catalog;
3. exact-revision workspace creation;
4. full and sliced reads;
5. digest-guarded WRITE, APPEND, and REPLACE_EXACT mutation;
6. bounded Git diff and untracked-path projection;
7. transactional execution;
8. Job observation and listing;
9. native MCP task listing;
10. Artifact content and SHA-256 verification;
11. cancellation of a running Job;
12. server restart followed by durable observation from SQLite truth.

**Observed:** the first E2E attempt failed because APPEND and REPLACE_EXACT against existing files require `expectedDigest`. The client journey was corrected; the Runtime contract was not weakened.

## Recovery

`backup.py` and `restore.py` now form a bounded external maintenance pair.

**Observed:** the automated drill created SQLite state, took an online backup, copied permitted regular control files, verified manifest digests and byte lengths, restored to a new destination, ran `PRAGMA integrity_check`, and compared restored rows.
**Observed:** digest mismatch is rejected.
**Unknown:** restoring a large live operational Registry while new dispatches continue is not proven and is not claimed.

## Secret-scan resolution

A full-history Gitleaks scan initially reported three `generic-api-key` findings in historical commit `638c763...`, all in static paper-trading boundary tests. Read-only inspection established that they are explicit test placeholders such as test-prefixed public keys and the literal value `secret`; no real credential was identified.

The three exact findings are recorded in `.gitleaksignore` by commit, file, rule, and line. After adding those fingerprints, the complete 593-commit history scan passed with zero findings.

**Observed:** current-tree and complete-history scans both pass.
**Inferred:** rewriting repository history would add cost without reducing a real credential exposure identified by this audit.

## Bounded performance snapshot

At `1253524...` on the recorded WSL machine:

| Metric | Result |
|---|---:|
| release Runner binary | 924,520 bytes |
| release MCP server binary | 10,638,208 bytes |
| Runner release build command | 43,962 ms |
| MCP release build command | 20,134 ms |
| startup plus first initialize | 62.178 ms |
| initialize median, 20 samples | 1.687 ms |
| initialize p95 | 2.343 ms |
| initialize maximum | 2.421 ms |

**Observed:** these measurements describe one machine, one cache state, and loopback initialization only.
**Unknown:** comparison with the pre-simplification tree, execution throughput, model task success, sustained concurrency, and remote latency. The snapshot must not be treated as a general performance claim.

## External consumer and deployment audit

The live public path is currently:

```text
mcp.ordivon.com
→ Cloudflare Tunnel
→ localhost:8811
→ Supergateway 3.4.3
→ Desktop Commander 0.2.46
```

It is not the Rust `ordivon-mcp` server.

**Observed:** port 8811 listens on all interfaces, although Cloudflared reaches it through localhost.
**Observed:** the current ChatGPT management channel depends on this bridge, so it was not restarted or replaced.
**Observed:** no inspected systemd or Cloudflare configuration consumes removed M1/M4/M6/M7 binaries or old feature names.
**Observed:** historical PostgreSQL, NATS, Temporal, and Temporal database containers remain running; the simplified Runtime does not consume them, but ownership by other workflows was not disproved.

Remote Rust deployment remains blocked until a separate rollback-capable maintenance window validates a second local port and test hostname before changing the production route.

## Debt disposition

### Closed in this stage

- repeatable layered acceptance runner;
- exact-head machine-readable manifest;
- real ten-test systemd/cgroup execution;
- complete nine-tool MCP journey;
- restart observation and cancellation;
- backup/restore drill;
- active legacy-reference scan;
- current and full-history secret scanning;
- compatibility and deployment-boundary documentation;
- read-only workstation consumer audit;
- manually dispatchable portable release workflow;
- bounded current-version benchmark snapshot.

### Still open — P0 before mainline adoption

1. **Independent read-only review.** The implementing context cannot independently certify its own work.
2. **Mainline adoption form.** Decide between preserved-history direct adoption and a clean stacked adoption series after independent review of `main..candidate`.
3. **Remote deployment migration.** Move Cloudflare to the Rust server only through a second-port/test-hostname/rollback procedure.
4. **Wildcard listener.** Restrict or retire `*:8811` after an alternate management path is proven.

### Still open — P1

- confirm ownership and remove obsolete PostgreSQL/NATS/Temporal containers if unused;
- investigate repeated Cloudflare stream-cancellation messages during migration testing;
- establish comparable pre/post and execution-workload benchmarks;
- add sustained concurrency, restart stress, and long-running resource-leak tests;
- exercise an operational restore with dispatches deliberately stopped.

### Explicit non-goals

Do not add new Agent loops, Tools, Skills, governance platforms, databases, distributed scheduling, multi-tenant authority, UI, lifecycle GC, or speculative infrastructure during review and adoption.

## Decision

**Continue to independent audit and mainline-adoption design; do not expand product capability.**

The candidate is locally acceptance-complete for the tested environment, but it is not production-certified, remotely migrated, independently reviewed, pushed, or merged.
