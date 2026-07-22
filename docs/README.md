# Ordivon Documents

The active document surface is intentionally small. Read it in this order:

1. [Current state](current-state.md) — what exists now, what is live, and the only active milestone.
2. [Problem statement](ai/problem-statement.md) — the user and engineering problem Ordivon solves.
3. [Runtime architecture](architecture/runtime.md) — the current code and persistent-state model.
4. [Operating boundary](architecture/operating-boundary.md) — when to allow, isolate, block, add, or delete structure.
5. [Local operations](operations/runbook.md) — build, run, inspect, back up, restore, and recover.
6. [Deployment boundary](operations/deployment-boundary.md) — current remote topology and rollback-safe migration.

These six documents are the only active textual authority. Code, tests, Git identity, and live process state remain the primary evidence when text and reality disagree.

## Reference records

The following files record a dated compatibility result, acceptance event, closure, or retrospective. They may explain a decision but must not be used as current architecture or roadmap:

- [Compatibility record](operations/compatibility.md)
- [Acceptance design](architecture/post-simplification-acceptance-design.md)
- [External consumer audit](operations/external-consumer-audit-2026-07-22.md)
- [Acceptance report](operations/post-simplification-acceptance-report-2026-07-22.md)
- [M-series closure](governance/ordivon-m-series-closure-2026-07-22.md)
- [M0–M7 retrospective](postmortems/ordivon-m0-m7-stage-retrospective-2026-07-22.md)
- [Post-simplification retrospective](postmortems/ordivon-post-m-series-simplification-retrospective-2026-07-22.md)
