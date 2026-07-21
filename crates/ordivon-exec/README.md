# ordivon-exec

Deterministic local execution primitives for AI agents.

This crate is not an agent harness and does not call a model. It converts typed
requests into bounded local operations. Protocol adapters such as MCP must
remain outside the core contract.

## V0 bootstrap scope

Implemented:

- `read_text`: 1-based line ranges, byte budget, SHA-256 revision digest,
  continuation metadata, UTF-8-only fail-closed behavior.
- `read_many`: bounded batch reads with one total byte budget and independent
  item failures.

Not implemented yet:

- text search;
- repository snapshot;
- patch/write operations;
- short commands or durable jobs;
- MCP or Hermes adapters;
- authorization, receipts, or runtime observation emission.
