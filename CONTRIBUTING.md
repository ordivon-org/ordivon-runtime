# Contributing to Ordivon

Read [`AGENTS.md`](AGENTS.md) and begin from the exact source, Git, test, and live state relevant to the change.

A contribution should identify:

1. the observed failure or repeatedly missing operation;
2. the bounded implementation and the existing path it replaces or extends;
3. the tests or live evidence that can falsify the change.

Preserve unrelated user work and avoid parallel execution paths or speculative infrastructure. Run the smallest checks that cover the changed boundary; CI records the repository's default checks.

Security reports follow [`SECURITY.md`](SECURITY.md).
