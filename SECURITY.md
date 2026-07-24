# Security Policy

## Reporting

Do not open a public vulnerability issue. Contact the repository owner with `[SECURITY]`, the affected component, reproduction steps, and impact.

## Supported version

Only current `main` is supported. There are no backports or LTS branches.

## Boundary

Ordivon is a trusted-local execution Runtime and intentionally inherits the installed service user's filesystem, process, network, credential, Git, Docker, systemd, cloud-CLI, and executable authority. Do not submit untrusted repositories or scripts; use a separate VM, container host, or equivalent external isolation boundary.

The MCP origin must remain loopback-bound. Direct local clients require the configured Bearer token; remote access must reach that origin through the authenticated tunnel boundary configured by the operator.
