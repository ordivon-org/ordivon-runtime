# ordivon-mcp

`ordivon-mcp` is the single current MCP entry over `ordivon-exec`.

It serves Streamable HTTP on a loopback address, requires a random bearer token, bounds request bodies, and exposes:

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

The adapter owns protocol schemas, MCP Task projection, transport authentication, and conversion between tool results and Runtime results. It binds the server-owned Principal and global concurrency limit; no policy or profile objects are part of the execution contract. Filesystem, Git, execution, idempotency, cancellation, result, and reconciliation semantics remain in `ordivon-exec`.

`trusted-local` is the default and inherits the service user's host authority. `isolated` explicitly activates the reduced-authority non-root worker boundary.

For a Cloudflare Access protected remote hostname, set `ORDIVON_TRUST_CF_ACCESS=true`. The loopback service then accepts either its fixed local Bearer token or a non-empty `Cf-Access-Jwt-Assertion` inserted by Access.
