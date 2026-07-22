# Ordivon Deployment and Remote Boundary

## Current observed topology

A read-only workstation audit on 2026-07-22 established this live path:

```text
mcp.ordivon.com
→ Cloudflare Tunnel container using host networking
→ http://localhost:8811
→ supergateway 3.4.3
→ Desktop Commander 0.2.46 over stdio
```

This is the connector used by the current ChatGPT session. It is **not** the new `ordivon-mcp` runtime.

The Supergateway listener is currently bound to `*:8811`, while Cloudflared reaches it through `localhost:8811`. Supergateway 3.4.3 exposes no documented host/bind option in its CLI. The new Rust server independently enforces a loopback bind and bearer token.

## Consequence

The local MCP E2E result proves the Rust server on an ephemeral loopback port. It does not prove the Cloudflare, Access, or production connector path. Replacing the active bridge during the same session could sever the only management channel, so this stage does not modify the live service or Tunnel.

Remote migration remains blocked until a separate maintenance window completes the following sequence:

1. install `ordivon-mcp` and `ordivon-task-runner` at fixed paths;
2. install `packaging/systemd/ordivon-mcp.service` and a root-readable environment file;
3. start the Rust server on `127.0.0.1:8897` without changing port 8811;
4. run initialize, nine-tool E2E, cancellation, restart observation, and backup/restore against 8897;
5. create a separate test hostname or Tunnel route to 8897;
6. verify Cloudflare Access, bearer forwarding, request-size bounds, and credential-free logs;
7. switch the production route only after the old path remains available for rollback;
8. remove or restrict the wildcard 8811 listener after the Rust path is stable.

## Responsibility boundary

Cloudflare Tunnel and Access authenticate and transport remote requests. They do not replace Runtime authorization, idempotency, execution bounds, or Job/Attempt truth. Conversely, Runtime bearer authentication does not prove that Cloudflare logs, routes, and policies are correctly configured.

## Mode selection

- `trusted-local`: the authenticated Agent inherits the Ordivon service user's host authority, including network, credentials, Git, Docker, systemd, and cloud tooling.
- `isolated`: the user or Agent explicitly requests a reduced-authority non-root sandbox.
- remote exposure: authenticate the caller and verify the exact proxy/Tunnel path; do not silently convert trusted authority into a sandbox merely because transport is remote.
