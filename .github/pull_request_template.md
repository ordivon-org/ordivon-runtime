## State change

What observable current state changes, and which user or Runtime failure does it address?

## Surface change

- What persistent component, state, tool, document, or dependency was added?
- What existing structure does it replace or remove?
- Why is the current Runtime, Git, tests, or operations path insufficient without it?

## Runtime boundary

- [ ] No irreversible external effect was introduced without an explicit control.
- [ ] Idempotency, concurrency, process ownership, persistent truth, or recovery semantics were updated when applicable.
- [ ] Unrelated user changes were preserved.
- [ ] Active documents were updated only when current truth changed.

## Verification

- [ ] `cargo fmt --check`
- [ ] `cargo test --workspace`
- [ ] `ruff check scripts/`
- [ ] Additional checks were selected because they can invalidate this change.
