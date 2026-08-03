---
schema_version: 1
id: runtime.quickstart
title: Runtime Quick Start
type: quickstart
profile: engineering
lifecycle: active
source_role: canonical
visibility: public
owners:
  - ordivon-runtime
audience:
  - user
  - builder
  - operator
updated: 2026-08-04
summary: Minimal path from a clean checkout to portable checks, real system acceptance, and receipted installation.
evidence_status: verified
readiness: READY
applies_to:
  - ordivon-runtime
related:
  - runtime.start
  - runtime.status
  - runtime.operations
  - runtime.data-privacy
---
<!-- cspell:words rustc clippy toplevel -->
# Runtime Quick Start

## Goal

Use the portable path to inspect or contribute from any supported Linux development environment. Use the real-system path only on an owner-trusted machine with systemd, cgroup v2, and root authority. Use the production path when installing the long-running service.

## Prerequisites

```bash
git clone https://github.com/zycxfyh/ordivon-runtime.git
cd ordivon-runtime
git status --short --branch
rustc --version
python3 --version
```

The repository fixes Rust 1.95.0 through `rust-toolchain.toml`. Python operational scripts support Python 3.11 or newer.

## Steps

Follow the portable, real-system, configuration, deployment, and first-use steps below in order.

## 2. Run portable checks

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo test -p ordivon-runtime-core --no-default-features --features transactional-runtime
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_docs.py
scripts/local-acceptance check
```

These checks validate source formatting, Rust and Python behavior, migrations, Registry invariants, protocol schemas, deployment and rollback scripts, lifecycle policy, documentation ownership, local links, and the generated Tool reference. They do not execute the ignored systemd/cgroup tests.

## 3. Run real system acceptance

The complete acceptance path requires:

```bash
cat /proc/1/comm
cat /sys/fs/cgroup/cgroup.controllers
sudo -n true
```

Then run:

```bash
sudo scripts/local-acceptance run
```

The acceptance command:

1. builds the Runner and Runtime;
2. runs the ignored systemd supervisor tests serially;
3. runs the ignored transactional Runtime tests serially;
4. starts a temporary loopback MCP instance;
5. performs the public protocol journey;
6. emits a digest-bound JSON receipt when an output path is supplied by the underlying acceptance script.

Do not run this path on a machine that contains untrusted repositories or credentials you are unwilling to expose to a root-owned trusted-local execution service.

## 4. Configure the installed service

Copy and edit the example outside the repository:

```bash
sudo install -d -m 0700 /etc/ordivon
sudo install -m 0600 packaging/systemd/ordivon-runtime.env.example \
  /etc/ordivon/ordivon-runtime.env
sudo editor /etc/ordivon/ordivon-runtime.env
```

At minimum:

- replace `ORDIVON_BEARER_TOKEN` with at least 32 random characters;
- keep `ORDIVON_BIND` on loopback;
- confirm `ORDIVON_EXEC_PATH` and `ORDIVON_EXEC_HOME` expose only intended trusted toolchains;
- set concurrency, runtime, output, and cache limits for the host;
- enable Cloudflare Access trust only when the loopback origin is reachable exclusively through the operator-owned authenticated tunnel.

## 5. Deploy with a receipt

Do not copy binaries manually. Build, plan, apply, and verify through the canonical deployment script:

```bash
repo=$(git rev-parse --show-toplevel)
commit=$(git rev-parse HEAD)
manifest="$repo/target/release/ordivon-deployment-manifest.json"
cargo=$(command -v cargo)

scripts/ordivon-runtime-deploy prepare \
  --source-repo "$repo" \
  --commit "$commit" \
  --candidate-dir "$repo/target/release" \
  --candidate-manifest "$manifest" \
  --cargo "$cargo"
```

Continue with the exact `plan` and `apply` commands in [`operations.md`](operations.md). The deployment receipt binds the source commit, build toolchain identity, binary digests, installed digests, protocol lifecycle, and Tool catalog digest.

## Verification

Verification combines the portable contract, the explicit real-system acceptance path, and the installed service checks below.

## 6. Verify the live service

```bash
scripts/ordivon-runtime-status --health --json
scripts/ordivon-runtime-status --diagnose --json \
  --expected-commit "$(git rev-parse HEAD)"
```

Health answers whether the current deployment is safe to operate. Diagnose adds bounded maintenance, storage, Workspace, and protocol-compatibility evidence.

## 7. Use the public Tool surface

Connect an MCP client to the configured loopback or authenticated tunnel endpoint. Begin with:

```text
workspace.open
workspace.read
workspace.patch
workspace.execPlan
task.observe
artifact.read
workspace.diff
workspace.close
```

The exact Tool names and descriptions are generated in [`reference/tools.md`](reference/tools.md). A complete executable client implementation is available in `scripts/mcp_e2e.py`.

## 8. Recover or remove

- Reconnect to existing work through `workspace.list`, `workspace.get`, `task.list`, and `task.observe`.
- Back up and restore through `scripts/backup.py` and `scripts/restore.py`.
- Repair only through the documented doctor/repair sequence in [`recovery.md`](recovery.md).
- Release Workspaces through `workspace.close` or the receipted lifecycle tools, never by deleting directories directly.
- For instance retirement, export required state, stop the service, preserve any mandated evidence, then delete the operator-owned state roots described in [`data-and-privacy.md`](data-and-privacy.md).
