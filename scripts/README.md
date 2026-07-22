# Scripts

Scripts are limited to local development and operational support. Durable runtime behavior belongs in the Rust crates.

The remaining `benchmarks/`, `mcp/`, and `m7/` scripts are M-series historical harnesses. They stay temporarily so Batch 3 can remove the proof machinery as one auditable change.

No script is part of the default CI path except Ruff linting. New scripts should be small, directly invoked, and tied to a current operational need.
