# ordivon-mcp

Thin MCP adapter over `ordivon-exec`.

The adapter owns protocol schema, tool annotations, and conversion between MCP
results and core results. It must not duplicate filesystem, search, Git, patch,
or job behavior.

V0 exposes four read-only stdio tools: `read_text`, `read_many`, `search_text`,
and `repo_snapshot`. Network transport is intentionally deferred so the first
comparison can reuse the same Supergateway transport as Desktop Commander.
