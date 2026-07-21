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
