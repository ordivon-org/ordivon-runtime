# Runtime Compatibility Inventory

Runtime retains compatibility only when a named live consumer, persisted state, or receipt-bound recovery path still depends on it. Compatibility is not an independent product objective.

The current machine-readable inventory is projected by:

```text
scripts/ordivon-runtime-status --diagnose --json
```

Its `compatibility` section combines the current deployment receipt, a bounded tail of protocol observations, the immediately previous receipted deployment, and a default seven-day observation window. Missing, truncated, or temporally insufficient observations block deletion conclusions but do not create an operational incident. The window can be changed explicitly with `--protocol-retention-hours`; status never shortens it implicitly.

## Protocol lifecycles

| Contract | Current role | Protected failure | Deletion trigger |
| --- | --- | --- | --- |
| MCP `2026-07-28` | Canonical deployment and acceptance lifecycle | New candidates silently falling back to an older lifecycle | A later canonical lifecycle is deployed, accepted, and leaves its rollback window |
| MCP `2025-11-25` | Compatibility for named initialized clients, currently including the connected OpenAI Host | Current clients losing the Runtime endpoint after an adapter upgrade | A complete observation window contains no live client and no receipt-bound rollback depends on it |
| MCP `2025-06-18` | Compatibility for the immediately previous receipted Runtime binary | A failed deployment cannot prove that the previous service has recovered | The previous binary and its rollback window expire and no live client is observed |

`LocalSessionManager` and legacy `initialize` handling exist only for the two compatibility lifecycles. Modern continuity remains Workspace, Job, Attempt, cancellation, reconciliation, and Artifact truth in Runtime Core.

## Persisted Runtime state

| Contract | Current consumer | Protected failure | Deletion trigger |
| --- | --- | --- | --- |
| Registry migrations v1–v4 and checksum validation | The production Registry and fresh installation or upgrade paths | Existing Job, Attempt, event, reservation, and repair history becomes unreadable or ambiguous | One explicit major-state cutover produces a new accepted baseline, archives the old Registry with a verified receipt, and removes every rollback dependency |
| Legacy request-identity derivation from the stored plan | Jobs admitted before explicit `requestIdentityDigest` | A fresh Host cannot reattach to an older idempotent request after response loss | No retained Registry contains a Job that requires derivation, or a major-state cutover intentionally retires those Jobs |
| Client-request lookup index without a schema-version advance | Current recovery callers and the receipt-bound previous binary | Response-loss recovery cannot locate the original Job, or rollback cannot open the Registry | The previous-binary rollback window expires and the next intentional schema baseline incorporates the query contract |
| Workspace Patch receipt table without a schema-version advance | Harness durable Patch recovery and the receipt-bound previous binary | A lost response causes duplicate file effects, or rollback cannot open the Registry | The previous-binary rollback window expires and a later intentional schema baseline incorporates the isolated Patch receipt contract |
| Additive defaults for budgets, execution profiles, foreign references, and observation preferences | Requests admitted before those optional fields existed | Replaying the same operation changes identity or old records fail to decode | A major-state cutover proves no retained request or Execution Plan requires the default |
| v2 execution-proposal request identity and frozen effective limits | Jobs admitted from omitted/delegated `workspace.exec` or `workspace.execPlan` limits | Policy drift changes an already committed operation, or response-loss recovery creates duplicate physical work | Current Runtime replays the v2 proposal before policy evaluation. The immediately previous v1-only binary may not accept the omitted request shape after rollback, but it can still locate the retained Job with `task.list(clientRequestId)` and reconcile it with `task.observe(jobId)`; exact omitted-shape replay is restored when the current binary is redeployed. This compatibility branch can be reconsidered after the v1-only rollback window expires. |

Serde defaults that express ordinary optional API fields are not classified as legacy merely because they are defaults. They become deletion candidates only when a concrete old representation is retired.

## Agent-facing semantic compatibility

| Contract | Current consumer | Protected failure | Deletion trigger |
| --- | --- | --- | --- |
| Legacy `INVALID_REQUEST` fallback for a missing `workspaceId` | `ordivon-host` Workspace ensure/close compatibility path | Host mistakes a precise `WORKSPACE_NOT_FOUND` response for an unrecoverable Runtime failure during staged rollout | Runtime with precise Workspace errors is deployed and one complete compatibility observation window shows no supported Runtime returning the legacy missing-Workspace form |
| Coarse Job `status` (`queued`, `working`, terminal resolution) | Existing callers that display or log the historical summary | A minor Runtime release breaks callers that do not yet consume explicit execution semantics | Named live callers consume `attemptState`, `executionDisposition`, `deliveryDisposition`, and recovery fields for control decisions; the coarse field may then be reviewed separately rather than removed automatically |

The precise Workspace error migration is intentionally two-sided: Host accepts both the old `INVALID_REQUEST` form and `WORKSPACE_NOT_FOUND` before Runtime emits the precise code in production. The compatibility branch exists to order the rollout safely; it does not make the old classification semantically canonical.

## Deletion rule

A branch is eligible for deletion only when all of the following are true:

1. no current deployment or acceptance path selects it;
2. the explicit retention window has completed and no named live client is observed during it;
3. no current receipt or previous binary requires it for rollback;
4. no retained Registry object requires it for reconstruction or idempotent reattachment;
5. modern deployment, rollback, restart, and response-loss acceptance remain green without it.

The status projection reports `deletionCandidates`; it does not delete code or state. Removal remains a reviewed source change with normal deployment and rollback evidence.
