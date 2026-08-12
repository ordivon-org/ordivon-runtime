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

## Remote MCP authentication compatibility

Remote authentication is an edge compatibility contract, not a Runtime protocol lifecycle. The Runtime origin remains loopback-only and keeps a private static Bearer credential for trusted-local clients. That credential is not an interoperability mechanism and must never be copied into hosted AI platforms.

The public MCP path is split deliberately:

1. Cloudflare Tunnel provides public HTTPS ingress to the loopback origin but does not establish identity.
2. Cloudflare Access Managed OAuth acts as the public OAuth authorization server and protected-resource facade for hosted or interactive MCP clients.
3. Access forwards a signed `Cf-Access-Jwt-Assertion` after OAuth authorization.
4. Runtime verifies that assertion's RS256 signature, exact issuer, Access application audience, and expiry against Access JWKS before admitting the request.

Runtime retrieves Access JWKS lazily. A JWKS outage therefore fails the Access-authenticated request closed without turning public network availability into a startup dependency of the trusted-local Runtime. A nonempty or syntactically JWT-shaped Access header is never sufficient authentication.

Access currently advertises Dynamic Client Registration (DCR). MCP `2026-07-28` prefers Client ID Metadata Documents (CIMD), while retaining DCR as a compatibility path for authorization servers and clients that have not migrated. Therefore DCR callback policy is tracked as an edge-client compatibility surface rather than being encoded into Runtime Tool or wire semantics. No platform-specific callback, OAuth client identifier, or authorization-server implementation belongs in Runtime Core.

Named callback compatibility is expanded only from official client documentation or a directly observed registration attempt. The current known set includes ChatGPT's existing connector callback, VS Code's loopback/web callbacks, and Claude's documented MCP callbacks. Unknown hosted-client callback URIs, including any not documented by their platform, must not be guessed or covered by broad wildcard domains merely to make registration succeed.

The legacy `ORDIVON_TRUST_CF_ACCESS` name is retained only as a rollback-compatible enable flag. When it is true, `ORDIVON_CF_ACCESS_ISSUER` and `ORDIVON_CF_ACCESS_AUDIENCE` are mandatory and the assertion is cryptographically verified. Renaming or deleting the legacy flag is eligible only after the receipt-bound previous binary can no longer be restored and one complete compatibility observation window proves no operator configuration depends on the old name.

`workspace.content` is an additive Tool-catalog capability rather than a replacement for `workspace.read` or `artifact.read`. Older clients keep their UTF-8 read semantics and can ignore the new Tool; clients that need native media must refresh discovery and bind the new catalog digest before calling it. No persisted Job/request identity or Registry migration is introduced by this capability.

## Persisted Runtime state

| Contract | Current consumer | Protected failure | Deletion trigger |
| --- | --- | --- | --- |
| Registry migrations v1–v4 and checksum validation | The production Registry and fresh installation or upgrade paths | Existing Job, Attempt, event, reservation, and repair history becomes unreadable or ambiguous | One explicit major-state cutover produces a new accepted baseline, archives the old Registry with a verified receipt, and removes every rollback dependency |
| Legacy request-identity derivation from the stored plan | Jobs admitted before explicit `requestIdentityDigest` | A fresh Host cannot reattach to an older idempotent request after response loss | No retained Registry contains a Job that requires derivation, or a major-state cutover intentionally retires those Jobs |
| Client-request lookup index without a schema-version advance | Current recovery callers and the receipt-bound previous binary | Response-loss recovery cannot locate the original Job, or rollback cannot open the Registry | The previous-binary rollback window expires and the next intentional schema baseline incorporates the query contract |
| Workspace Patch receipt table without a schema-version advance | Harness durable Patch recovery and the receipt-bound previous binary | A lost response causes duplicate file effects, or rollback cannot open the Registry | The previous-binary rollback window expires and a later intentional schema baseline incorporates the isolated Patch receipt contract |
| Job execution-provider side table and `runtime-operation-v4` commitment marker without a schema-version advance | New Jobs that bind the Runtime-owned Linux Runner or Windows launcher, while the receipt-bound previous binary must still decode retained Execution Plans | Provider bytes/configuration drift after admission changes the machinery that realizes an already committed operation, or a new plan field makes rollback unable to decode retained Jobs | Provider state is committed atomically beside the Job, its digest is also recorded in the backward-readable raw Workspace snapshot and operation identity, and the strict Execution Plan JSON shape is unchanged. The old binary ignores the additive table/marker. Explicit rollback now holds `admission.lock` and drains active/held Jobs before handoff so an old binary cannot inherit a nonterminal provider-bound Job. Missing/tampered current side truth fails closed instead of degrading to historical semantics. |
| Additive defaults for budgets, execution profiles, foreign references, and observation preferences | Requests admitted before those optional fields existed | Replaying the same operation changes identity or old records fail to decode | A major-state cutover proves no retained request or Execution Plan requires the default |
| Additive `inputSetId` / `effectiveInputs` Execution Plan fields and separately versioned input-bound request identities | Existing Jobs whose plans predate immutable input binding, plus callers of `workspace.execBound` | Old plans fail to decode, or input-bound execution aliases legacy/proposal identity | Empty/default input fields preserve old plan decoding. Historical exact-input Core requests retain `runtime-request-input-v1:`; public proposal-plus-input admission uses `runtime-request-input-v2:` through the separate `workspace.execBound` Tool, leaving historical `workspace.exec` / `workspace.execPlan` identities unchanged. |
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
