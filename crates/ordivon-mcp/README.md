# ordivon-mcp

`ordivon-mcp` is the single MCP entry over `ordivon-exec`.

It serves Streamable HTTP on loopback, requires a random bearer token, bounds request bodies, and exposes:

```text
workspace.open
workspace.close
workspace.read
workspace.mutate
workspace.diff
workspace.exec
task.observe
task.cancel
task.list
artifact.read
```

The adapter owns protocol schemas, MCP Task projection, authentication, and conversion between MCP and Runtime results. Filesystem, Git, execution, idempotency, cancellation, result, capacity, and reconciliation semantics remain in `ordivon-exec`.

`workspace.open` isolates Git state, not host authority. `workspace.exec` inherits the installed service user's trusted-local authority.

For a Cloudflare Access protected hostname, set `ORDIVON_TRUST_CF_ACCESS=true`. The loopback origin then accepts either its fixed local Bearer token or the Access JWT assertion.
