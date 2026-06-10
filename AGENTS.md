# Ordivon Agent Entry

This file is a routing aid only. It is not canonical architecture truth.

## Canonical Documents

Read these documents first and treat them as the source of truth:

1. `docs/ai/ordivon-core-refrozen.md`
2. `docs/architecture/ordivon-governance-control-loop.md`
3. `docs/architecture/semantic-firebreak.md`
4. `docs/architecture/runtime-governance-alignment.md`
5. `docs/architecture/ai-native-project-object-model.md`
6. `docs/architecture/ordivon-core-method-skill-scope.md`
7. `docs/product/ordivon-flywheel-and-pack-roadmap.md`
8. `docs/audits/certification/production-security-readiness.md`

All other documents are downgraded to support, history, or operational notes.
When any old doc conflicts with the documents above, ignore the old doc.

## Operating Rules

- Do not restore old `apps/`, `packs/`, `domains/`, or `checkers/` just because
  archived docs mention them.
- Keep only components with strict core survival logic.
- Prefer mature infrastructure over custom low-level wheels:
  PostgreSQL, NATS JetStream, Temporal, OpenFGA, OPA, OpenTelemetry, and S3.
- No Receipt, No Done.
- No Authority, No Side Effect.
- Text must not directly move state.
- AI_WRITTEN is never SYSTEM_OBSERVED.
- `READY` is never authorization.

## Directory Structure (AI-Native Project Object Model)

The project is being reorganized per `docs/architecture/ai-native-project-object-model.md` (canonical #5).
Key directories:

| Layer | Directory | Status |
|-------|-----------|--------|
| Source | `src/`, `crates/`, `schemas/`, `state/` | Active |
| Agent Constitution | `AGENTS.md`, `docs/ai/` | Active |
| Skill/Method | `skills/` (ordivon-core-method) | Active |
| Prompt/Template | `prompts/` | Active |
| Tool/Script | `scripts/` (→ `tools/` in future) | Active |
| MCP/Connector | `mcp/` | Active |
| Eval/Test | `tests/`, `evals/` | Active |
| Trace/Receipt | `traces/`, `receipts/` | Active |
| Policy | `policies/` (incl. openfga) | Active |
| Registry/Ledger | `docs/governance/*.jsonl` (→ `registries/` in future) | Active |
| Checker | `checkers/` | Active |

Migration status: `receipts/` and `policies/openfga/` migrated. `scripts/` and `docs/governance/` retained
due to 70+ hardcoded path references; `tools/` and `registries/` are target directories with README mappings.

## Verification

Use fresh local evidence:

```bash
PYTHONPATH=.:src .venv/bin/python -m pytest -q
PYTHONPATH=.:src .venv/bin/python scripts/check_document_registry.py
PYTHONPATH=.:src .venv/bin/python scripts/verify_infrastructure.py
PYTHONPATH=.:src .venv/bin/python -m ordivon_verify all --check
cargo test --workspace
ORDIVON_TEST_DATABASE_URL=postgresql://ordivon:ordivon@localhost:5432/ordivon \
  cargo test --workspace --features postgres-integration,policy-http
```
