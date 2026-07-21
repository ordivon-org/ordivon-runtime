# Script Policy

This directory is for operational or development support scripts only.

If a script contains reusable business behavior, prompt logic, workflow control, or durable product capability, it should be promoted into `tools/`, `skills/`, `domains/`, or `orchestrator/` instead of growing here indefinitely.

## Benchmarks

`scripts/benchmarks/` contains development-only differential harnesses and
independent evidence checkers. A benchmark may invoke local experimental
features and an existing Legacy adapter, but it must not activate production
routes or become durable product behavior inside `scripts/`.

Current benchmark:

- `m2_differential.mjs`: Legacy Desktop Commander versus Ordivon M1 journey.
- `check_m2_evidence.py`: independent recomputation of M2 evidence and gates.

## M3 benchmarks

The M3A and M3B differential scripts measure compact local Ordivon journeys
against real Desktop Commander MCP paths. The M3 evidence checker recomputes
medians, semantic equivalence, recovery claims, and performance gates.

These are development evidence tools and do not change production routing.

## M4 benchmarks

The M4A and M4B differential scripts compare the active Legacy MCP endpoint
with the independent experimental Ordivon Streamable HTTP MCP endpoint through
the same JavaScript MCP SDK. They record paired semantic digests, p50 and p95
latency, logical calls, real task HTTP requests, request bytes, response bytes,
context bytes, fallback, and disconnect recovery.

`m4_transport_security.mjs` verifies authentication, Origin, Host, and body-size
rejection. `m4_resilience.mjs` verifies server-restart Task recovery, native MCP
Task cancellation, cgroup cleanup, and residual-process cleanup.

`check_m4_evidence.py` independently recomputes both 20-pair benchmark summaries
and optionally validates Core and HTTP trace uniqueness and credential absence.
These scripts authorize only bounded local Dogfood eligibility; they do not
change port 8811, Cloudflare, production service definitions, or default routing.

## M5 Dogfood harness

`scripts/mcp/stack.py` locks the local MCP SDK identity and controls the full
experimental server lifecycle, including build, transient systemd startup,
health checks, child execution, failure cleanup, worktree cleanup, and residual
state removal.

`m5_wire_contract.mjs` freezes response budgets and concurrent trace identity.
`m5_dogfood.mjs` exercises seven bounded local Agent journeys.
`m5_shadow.mjs` compares four safe journeys with Legacy Desktop Commander.
`check_m5_evidence.py` independently recomputes the M5 evidence and decisions.

The M5 harness does not modify the production 8811 endpoint or authorize
external side effects, credentials, deployment, or automatic Legacy fallback.
