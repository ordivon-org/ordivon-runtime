## Summary

What changed and why?

## Runtime boundary

- [ ] No irreversible external side effect was introduced without an explicit control.
- [ ] Idempotency, concurrency ownership, or recovery semantics were updated when applicable.
- [ ] Unrelated user changes were preserved.

## Verification

- [ ] `cargo fmt --check`
- [ ] `cargo test --workspace`
- [ ] `ruff check scripts/`
- [ ] Additional boundary-specific checks were run when needed.
