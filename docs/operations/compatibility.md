# Post-Simplification Compatibility

## Current public entry

The only active server binary is `ordivon-mcp`, using Streamable HTTP and the nine tools documented in `crates/ordivon-mcp/README.md`.

## Removed surface

| Removed surface | Disposition |
|---|---|
| stdio server and M4/M6/M7 HTTP binaries | removed; use `ordivon-mcp` |
| M1 CLI | removed; use MCP tools or the Rust library |
| `m6-transactional-runtime` and `m7-runtime-hardening` feature names | removed; use `transactional-runtime` and `isolated-execution` |
| Python governance and `ordivon_verify` commands | removed; no runtime replacement |
| PostgreSQL/NATS/Temporal/OpenFGA/OPA governance services | removed from Runtime requirements |
| lifecycle quota, hold, GC, tombstone, and core restore protocols | removed; use external bounded maintenance |
| stage evidence, receipts, dogfood, and shadow harnesses | historical at tag `m-series-closed-2026-07-22` |

The repository scanner `scripts/legacy_reference_scan.py` rejects active tracked references to removed entry points. Historical closure and retrospective documents are explicitly exempt.

## Branch adoption

Current `main` is an ancestor of the M-series closure chain. The candidate branch contains the complete runtime introduction and later simplification. Review must therefore use the final `main..candidate` net diff and not treat the simplification tail as an independently deployable patch.
