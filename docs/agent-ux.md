# Agent-facing execution UX

Ordivon Runtime keeps four universal execution semantics in the core and leaves product-specific workflows in Skills.

## Core primitives

- `workspace.patch`: one digest-guarded, multi-file transaction with multiple exact text ranges per file. Every file and edit is validated before the first write; conflicts reject the whole request. Later write failures trigger rollback and the response includes a bounded final diff. This is API-atomic during normal process execution, not a cross-file crash-atomic filesystem transaction.
- `workspace.execPlan`: ordered native process steps. Steps stop on the first failure by default and continue only with explicit `continueOnError`. No shell parsing or injected `set -e` is involved.
- `task.observe`: projects elapsed time, retained output byte counts, last output time, progress revision, completed and total steps, current step, and first failed step. `waitUntil=change_or_terminal` waits until output, progress, or terminal state changes.
- typed operation errors: MCP errors carry `origin`, `retryClass`, `commitState`, `traceId`, and, when known, `operationId`. `commitState=unknown` requires reconciliation rather than a new operation identity.

## Workspace recovery

`workspace.open` accepts a caller-provided compatibility ID or generates an immutable `ws-*` handle when `workspaceId` is omitted. Use an explicit ID when response-loss replay identity matters; generated handles are a low-friction convenience for interactive opens. `workspace.list` and `workspace.get` are the reconnection entry points and expose exact source revision, detached-head mode, dirty state, creation time, and active Jobs.

## Structured report adapter

`scripts/ordivon_report.py` normalizes existing machine-readable reports instead of teaching the Runtime to parse every compiler:

```text
python scripts/ordivon_report.py --kind cargo-json --input target/messages.jsonl
python scripts/ordivon_report.py --kind junit --input test-results.xml
python scripts/ordivon_report.py --kind sarif --input results.sarif
python scripts/ordivon_report.py --kind playwright-json --input playwright-report.json
```

The normalized result contains a stable status, counts, and bounded primary issues with file, line, column, rule, and test identity when available.

## Git and GitHub Skills

Git and GitHub semantics remain outside the Runtime kernel:

```text
python scripts/ordivon_git_skill.py status --repo /absolute/repository
python scripts/ordivon_git_skill.py publish --repo /absolute/repository --remote origin --ref refs/heads/topic
python scripts/ordivon_github_skill.py pr-view --repo owner/repository --pr 123
python scripts/ordivon_github_skill.py await-checks --repo owner/repository --pr 123
```

The Git skill makes detached-head publication explicit. The GitHub skill converts PR, check, run, and merge operations into structured JSON and performs CI polling inside one local process. Destructive merge requires `--confirm MERGE`.

These Skills are replaceable adapters. They do not expand the trusted Runtime effect kernel.
