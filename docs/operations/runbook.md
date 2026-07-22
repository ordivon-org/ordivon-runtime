# Ordivon Local Operations

Ordivon no longer requires PostgreSQL, NATS, Temporal, OpenFGA, OPA, Redis, DuckDB, or a Python governance service.

## Build and verify

```bash
cargo build --workspace
cargo fmt --check
cargo test --workspace
```

## Run the current stdio MCP server

```bash
cargo run -p ordivon-mcp
```

The versioned local HTTP servers remain transitional until the entry points are unified.

## Inspect repository state

```bash
git status --short --branch
git rev-parse HEAD
git diff --stat
```

## Recover code or documentation

Use ordinary Git recovery. The complete pre-simplification M-series tree is tagged as:

```text
m-series-closed-2026-07-22
```

## Persistent runtime state

SQLite Registry and result files are the execution truth. Before manual maintenance, stop new dispatches and copy the control root with the SQLite backup API or `sqlite3 .backup`. Runtime cleanup and backup helpers will be reduced to small external scripts in a later batch.

## Strong isolation

The static `ordivon-worker` identity and systemd directory definitions remain available under `packaging/systemd/`. Strong isolation is an explicit mode for untrusted repositories or scripts; it is not the default cost for trusted, Git-backed local work.
