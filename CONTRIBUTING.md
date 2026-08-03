# Contributing to Ordivon Runtime

Ordivon Runtime accepts changes that strengthen its physical execution, durable state, process ownership, evidence, cancellation, reconciliation, recovery, Artifact, Workspace, or MCP projection boundaries. Goal semantics, Provider loops, product policy, hostile multi-tenant isolation, and domain completion belong elsewhere unless a concrete Runtime contract requires a narrow change.

## Start from exact state

Read [`AGENTS.md`](AGENTS.md), confirm the exact Git state, and inspect the relevant source, tests, and live process state. Do not preload every document or infer live behavior from prose.

```bash
git status --short --branch
git rev-parse HEAD
```

The repository fixes Rust through `rust-toolchain.toml`. Python scripts support Python 3.11 or newer.

## Development checks

Run the smallest checks that can falsify the changed boundary, then run the complete portable contract before submitting:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo test -p ordivon-runtime-core --no-default-features --features transactional-runtime
python3 -m unittest discover -s scripts/tests -v
python3 scripts/check_docs.py
scripts/local-acceptance check
```

Run the real system path when changing dispatch, systemd/cgroup supervision, Runner behavior, cancellation, recovery, authority profiles, resource budgets, or the public MCP journey:

```bash
sudo scripts/local-acceptance run
```

A hosted CI runner cannot substitute for this systemd/cgroup evidence.

## Change standard

A contribution must identify:

1. the observed failure or repeatedly missing operation;
2. the bounded implementation and the existing path it replaces or extends;
3. the tests or live evidence that can falsify the change;
4. compatibility, migration, rollback, security, privacy, and retention impact;
5. documentation whose authority or public claim changes.

Every persistent addition must justify its continued existence. Preserve unrelated user work, avoid parallel execution paths, and do not introduce speculative infrastructure.

Effect semantics must be enforceable by a structured adapter. Never trust a caller-declared effect class for arbitrary execution. Prefer mature host CLIs through `workspace.exec`; arbitrary execution remains opaque.

## Source organization

Mechanical file splitting is welcome when it reduces navigation cost without adding a second abstraction, state owner, service, or execution path. Do not introduce generic repository, scheduler, policy-engine, or plugin layers without a demonstrated workload failure.

## Documentation

Canonical documents and generated Tool reference are enforced by `scripts/check_docs.py`.

When changing Tools or schemas:

```bash
python3 scripts/check_docs.py --write
python3 scripts/check_docs.py
```

Update `CHANGELOG.md` for user-visible changes and follow [`docs/releases.md`](docs/releases.md).

## Pull requests

Use the repository pull-request template. Keep the change reviewable and include exact commands and evidence. A clean PR should not contain unrelated formatting, generated caches, deployment state, secrets, or local Workspace records.

Issue forms are available for bugs and bounded feature proposals. Security reports follow [`SECURITY.md`](SECURITY.md) and must remain private.

## Releases

Do not manually copy candidate binaries into production. A release uses portable CI, real-system acceptance when relevant, a clean Changelog entry, a generated Tool reference, a digest-bound candidate manifest, and receipted deployment and rollback. See [`docs/releases.md`](docs/releases.md) and [`docs/operations.md`](docs/operations.md).
