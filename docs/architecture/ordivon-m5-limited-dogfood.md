# Ordivon M5 Limited Dogfood and Shadow Routing

Date: 2026-07-22
Phase: ORDIVON-MIGRATION-M5-2026-07-22
Branch: agent/m5-limited-dogfood
Base: 97b609e65d024c881f85521b8812aaa634175758
Measured implementation: ff57d258c0697dba1335bf8a86ccb32ca85e9128

## Status

M5 passes bounded local Dogfood and Shadow gates. It does not change the
production 8811 MCP route, expose Cloudflare, authorize credentials, or permit
external side effects.

The correct decision is limited local Dogfood eligibility, not production
cutover. The filesystem Task registry, privileged worker identity, retention,
reconciliation, and broader workload coverage remain structural debt.

## M5.0 Harness stabilization

M5.0 converts the temporary M4 test setup into a reproducible lifecycle:

- a locked MCP SDK version and package.json digest;
- repository root derived from the script location rather than process.cwd();
- one stack controller for build, transient systemd startup, health, child
  execution, stop, worktree cleanup, and residual-state cleanup;
- a 0600 EnvironmentFile so the local bearer is not placed in the unit command;
- source-repository and source-revision admission for Dogfood workspaces;
- response-size budgets and no duplicate success content;
- 64 concurrent reads with unique Core, HTTP, and client-visible trace IDs;
- cleanup after both successful and deliberately failing child commands.

The wire contract observed successful responses between 94 and 299 bytes for
the seven exercised tools. The run produced 71 Core trace rows and 77 HTTP
trace rows without duplicate identifiers.

## M5.1 Limited Dogfood policy

The Agentive surface remains the eight M4 tools. M5 adds a classical admission
layer instead of adding new execution authority.

The experimental server accepts workspaces only when both values are allowed:

- canonical source repository path;
- exact source revision string.

The bounded Dogfood policy permits isolated reads, isolated mutations,
sandboxed local execution, Task observation and cancellation, and bounded
Artifact reads. It explicitly forbids push, merge, production-worktree
mutation, service or Cloudflare changes, credential access, deployment, and
automatic Legacy fallback.

The policy does not claim to replace authentication or a transactional
Authority system. It is a local stage boundary for M5 only.

## M5.1 Dogfood journeys

Seven journeys were executed through the real local MCP adapter:

1. bounded read-only audit with ripgrep and a clean diff;
2. one-file guarded edit and verification;
3. two-file Python module and unittest execution;
4. failing test, failure observation, digest refresh, exact repair, and rerun;
5. a real targeted Rust test for the M5 source admission policy;
6. large stdout truncation with bounded Artifact retrieval;
7. Native Task disconnect recovery and cgroup-owned cancellation.

All seven completed. The run used 33 model-facing calls, returned 15,090 bytes
of tool context, consumed 4,335 output bytes, performed one repair round, and
used zero Legacy fallback. Disconnect recovery and cancellation cleanup both
passed.

## M5.2 Shadow comparison

Four safely comparable journeys were each run as three alternating pairs:
read-only audit, one-file edit, multi-file test, and failure-repair loop.
Every pair produced the same backend-neutral semantic digest.

Overall medians across twelve samples:

| Metric | Legacy | Ordivon M5 |
|---|---:|---:|
| elapsed time | 603 ms | 477 ms |
| tool calls | 5 | 5 |
| context bytes | 1,722 | 1,721 |
| HTTP requests | 5 | 5 |
| repair rounds | 0 | 0 |
| fallback | 0 | 0 |

The overall bounded Shadow gates pass. This aggregate must not hide the
per-journey differences described below.

## Per-journey findings

The read-only audit was slower on Ordivon: 536 ms versus 465 ms. Its Context
was lower: 1,280 bytes versus 1,738 bytes. The likely cause is fixed workspace
and durable-execution setup dominating a short read-only journey.

The one-file edit was faster on Ordivon: 344 ms versus 539 ms. Context was
slightly higher: 3,376 versus 3,333 bytes.

The multi-file test was faster on Ordivon: 402 ms versus 665 ms. Context was
higher: 976 versus 776 bytes.

The failure-repair loop was faster on Ordivon: 578 ms versus 711 ms, with the
same single repair round. Context was higher: 2,162 versus 1,706 bytes.

M5 therefore proves bounded overall fitness, not uniform dominance on every
metric and journey. Read-only routing and mutation-result projection remain
specific optimization targets.

## Engineering findings from Dogfood

The repair journey initially attempted an exact replacement without refreshing
the file digest. The executor correctly rejected it with REVISION_MISMATCH.
The accepted flow is observe failure, reread current identity, then mutate.

The repaired Python source initially appeared to remain broken because a same-
size edit occurred within the timestamp granularity used by bytecode caching.
The second interpreter process reused stale bytecode. Dogfood now disables
Python bytecode generation for rapid repair verification. This is a harness
correctness fix, not an executor relaxation.

The standardized stack removed earlier M4 failure modes involving arbitrary
npx SDK selection, current-working-directory dependence, transient server
lifetime, scattered environment injection, and leaked smoke worktrees.

## Decision

M5 authorizes continued bounded local Dogfood on the experimental MCP adapter.
It does not authorize production routing, remote exposure, credentials,
external side effects, or automatic fallback.

The correct next structural stage remains a transactional Job and Attempt
control plane. Before broader routing, the project also needs a dedicated
non-privileged worker, retention and reconciliation, concurrent workload
coverage, and route-specific budgets for read-only and repair journeys.

## Evidence

- docs/audits/runtime/ordivon-m5-wire-contract-evidence-2026-07-22.json
- docs/audits/runtime/ordivon-m5-limited-dogfood-evidence-2026-07-22.json
- docs/audits/runtime/ordivon-m5-shadow-comparison-evidence-2026-07-22.json
- scripts/mcp/check_m5_evidence.py

The independent checker recomputes all Dogfood totals, Shadow medians,
semantic equivalence, and gates. A modified Shadow median is rejected.
