---
schema_version: 1
id: runtime.foundations.v1.engineering-consumption.r3-integration
status: completed
date: 2026-08-17
foundation_baseline: Runtime Foundations v1 (RF0-RF16 frozen)
r1_baseline: 7a03090945734c92bc43e1bd744109d6552ccfd9
r2_baseline: 8263b69
---

# Runtime Foundations v1 -> Engineering Consumption R3: Contract Hardening & Integration Readiness

## Purpose

R1 established that Runtime Core is already substantially aligned with frozen Runtime Foundations v1 and admitted only additive identity/evidence exposure. R2 showed that exact Runtime distinctions can still be destroyed by downstream consumer translation and corrected Host, Harness, and Security consumers.

R3 asks the next engineering questions without reopening RF0-RF16:

1. Are the exact Runtime fields consumed in R2 part of the currently supported Runtime/rollback contract, or were the consumers made stricter than production can support?
2. Should missing exact fields fall back to coarse `status`, or fail closed?
3. Can the R1 `operationDigest` additions be integrated independently?
4. What is the real publication order across Runtime, Computing Protocol, Host, Harness, and Security?
5. Does a real current Runtime falsifier validate the semantics outside fake/unit models?

R3 is an integration-readiness round, not a blanket deployment. Canonical repositories and production services remain unchanged at closeout.

---

## R3 result in one diagram

```text
                         +----------------------------+
                         | Runtime R1 additive lane   |
                         | a8f0740                    |
                         | operationDigest/evidence   |
                         +-------------+--------------+
                                       |
                                independent
                                       |
                                       v
                        current Runtime consumers

+----------------------------+
| Computing Protocol         |
| 5f9b418                    |
| patternProperties support  |
+-------------+--------------+
              |
              | exact Git dependency pin
              v
+----------------------------+
| Host                       |
| 5f76055 -> 8d7e58a         |
| exact delivery + R3 harden |
+----------------------------+

+----------------------------+       +----------------------------+
| Harness                    |       | Security                   |
| e760caa -> 7277e10         |       | 471b892 -> 5524d93         |
| exact observation contract |       | exact admission contract   |
+----------------------------+       +----------------------------+
          independent of Computing patternProperties fix

Finance: already-correct; no R3 change.
```

The integration graph is a **partial order**, not one forced global upgrade transaction.

---

## 1. Runtime compatibility and rollback boundary

### Current production

At R3 audit time the live Runtime reports:

```text
current deployed commit:
5561165077000818585bc959dd4bf87317b12f29

current deployment receipt:
20260813T183727Z-556116507700

canonical MCP protocol:
2026-07-28
```

The immediate receipt-bound previous Runtime available for rollback is:

```text
previous commit:
761bfe8dd7ca7c5e3e514891657c986eecb204e5

previous receipt:
20260813T022643Z-761bfe8dd7ca
```

Both are descendants of the Runtime change that introduced the exact Agent-facing execution/recovery projection (`16866157bb5aeca8b525510d42830631d8c6e37b`) and both expose the R2-required state vector:

```text
executionTerminal
executionDisposition
deliveryDisposition
recoveryRequired
resultAvailable
semanticCompletionEvaluated
```

### Compatibility conclusion

R2 consumer strictness is compatible with:

```text
current production Runtime
AND
current supported receipt-bound previous Runtime
```

Therefore R3 explicitly rejects a compatibility fallback such as:

```python
if exact_fields_missing:
    trust(status)
```

That fallback would recreate the exact semantic collapse R2 removed.

### Runtime contract floor

Once the R2 consumers are published, a Runtime rollback below the exact-field contract floor must be treated as a coordinated compatibility event, not an ordinary independent Runtime rollback.

Safe policy:

```text
supported Runtime has exact fields
    -> consumers use exact fields

unsupported/ancient Runtime lacks exact fields
    -> consumers fail closed
    -> do not infer from coarse status
```

If an operator intentionally rolls Runtime below that floor, the consumer revisions must also be rolled back or an explicit new compatibility design must be justified and tested.

---

## 2. R3 strict consumer-contract tests

R3 converts the compatibility conclusion into executable regression contracts.

### Host

New test:

```text
tests/test_runtime_semantics.py
```

It asserts that:

```json
{"status":"succeeded"}
```

without exact Runtime fields raises `RuntimeProtocolError`; exact committed success remains consumable.

### Harness

`SQLiteHarnessRuntimeBridgeTests` now explicitly asserts that a status-only Runtime projection fails `_runtime_delivery_state(...)` rather than being accepted as terminal.

The bridge owner then maps invalid/incomplete Runtime evidence to conservative Harness unknown behavior where appropriate.

### Security

`RuntimeAssignedActorTests` now explicitly asserts that a status-only projection raises `RuntimeMcpError` and cannot participate in Runtime-backed worker-result admission.

### Principle

```text
missing exact evidence != compatibility success
```

This is the cross-boundary form of the frozen Runtime Foundations truth-support discipline.

---

## 3. Runtime R1 integration readiness

A fresh Runtime integration Workspace was opened from canonical Runtime:

```text
base:
c6e45d9e41d3b4d64b5b3dace01497c53e574026

workspace:
runtime-r3-integration-runtime-20260817
```

R1 commit `7a03090` cherry-picked without conflict as:

```text
a8f07404a147d26b8189719eab2cdcff53850a33
runtime: consume foundations identity exposure
```

The candidate passed the publication-oriented Runtime validation chain:

```text
cargo fmt --all -- --check
cargo test --workspace --all-targets --all-features
cargo test -p ordivon-runtime-core --no-default-features --features transactional-runtime
python3 scripts/check_docs.py
scripts/local-acceptance check
```

The combined Runtime validation Job terminated successfully with committed delivery and no recovery requirement.

### Independence conclusion

R1 is additive and R2 consumers do not depend on `operationDigest`.

Therefore:

```text
Runtime R1 may publish before consumers
Runtime R1 may publish after consumers
Runtime R1 may roll back to current production without breaking R2 consumers
```

because the exact R2 execution/delivery fields predate R1.

`operationDigest` downstream adoption remains deferred until a concrete lineage consumer needs it; R3 does not manufacture a migration simply because publication is now safe.

---

## 4. Live integration falsification discovered a real contract blocker

R3 deliberately moved beyond fake Runtime tests and ran Host's existing real response-loss scenario against the current production Runtime:

```text
scripts/live_guarded_mutation.py
```

The scenario is particularly valuable because it exercises a high-load-bearing Runtime Foundations boundary:

```text
successful execution response is deliberately lost
    != permission to re-execute
```

### First live attempt: failed before mutation

The live path failed during Host Runtime catalog discovery with:

```text
unsupported JSON Schema keyword patternProperties
at $.inputSchema.$defs.properties.env
```

No mutation had been dispatched.

### Root cause

Current Runtime intentionally publishes environment maps using standard JSON Schema `patternProperties`. Runtime's own MCP tests explicitly protect this schema shape.

The shared Computing protocol normalizer:

```text
packages/ordivon-protocol/src/anc_tool_contract/model.py
```

allowed `additionalProperties`, `properties`, `$defs`, etc., but did not support `patternProperties`. Therefore Host correctly failed closed while normalizing a valid live Runtime contract.

This is a real integration mismatch:

```text
Runtime surface evolution
    x
shared ToolContract schema normalizer
```

It is **not** a Runtime Foundations/Core mismatch.

---

## 5. Computing Protocol fix

Workspace:

```text
runtime-r3-computing-schema-normalizer-20260817
```

Canonical Computing base:

```text
acd896688c26a5ea0dad22d66c7ea30882b1b245
```

Commit:

```text
5f9b418f1c366befa780f0ace1c6c8e64c721a3e
protocol: normalize JSON Schema patternProperties
```

### Minimal change

The normalizer now:

1. admits the standard `patternProperties` keyword;
2. treats its regex keys as schema-map keys just like property names under `properties` or definition names under `$defs`;
3. recursively strips presentation-only metadata inside each pattern schema;
4. continues rejecting unsupported schema keywords.

R3 deliberately did **not** change the normalizer into an accept-everything JSON Schema pass-through.

### Regression coverage

New tests prove both sides:

```text
valid patternProperties is preserved semantically
unknown unevaluatedProperties still fails closed
```

### Validation

```text
ordivon-protocol tests: 13 passed
Ruff: clean
protocol candidate check: ok=true, issues=[]
git diff --check: clean
```

More importantly, the exact patched Protocol source was then consumed by the real Host -> live Runtime falsifier, providing a concrete second consumer proof beyond package-local unit tests.

---

## 6. Second live failure was acceptance drift, not a Workspace leak

After the Protocol fix, the live guarded-mutation scenario progressed through the Runtime effect and produced all important success evidence:

```text
responseDroppedAfterExecAdmission = true
unknownStatePersisted             = true
oneWorkspaceExecInvocation        = true
oneRuntimeJobForClientRequestId   = true
originalRuntimeJobObserved        = true
noRedispatchAfterUnknown          = true
exactContentVerified              = true
taskCompleted                     = true
```

Only one assertion failed:

```text
runtimeWorkspaceClosed = false
```

### Investigation

The exact Runtime Workspace record was already:

```text
state = closed
```

and there was no physical Workspace directory left.

Runtime documentation also explicitly defines closed tombstones and precise `WORKSPACE_NOT_FOUND` behavior.

The stale component was Host's acceptance-only helper:

```text
src/ordivon_host/testing/runtime.py::workspace_absent
```

It recognized only the legacy missing-Workspace error:

```text
INVALID_REQUEST / workspaceId / not_committed
```

while Host production code already had the correct compatibility function:

```text
is_missing_workspace(...)
```

which accepts both legacy `INVALID_REQUEST` and precise `WORKSPACE_NOT_FOUND`.

### R3 correction

The acceptance helper now reuses the production classifier instead of owning a second, stale copy of the error-code policy.

This is a useful R3 lesson:

```text
acceptance model drift can generate false integration failures
just as fake Runtime drift can generate false unit failures.
```

The right correction was to remove duplicated compatibility knowledge, not weaken Runtime close semantics.

---

## 7. Final real response-loss falsifier

After both integration fixes, the exact same live scenario passed against the current production Runtime.

Receipt integrity digest:

```text
sha256:ee497d4027027d7b5d8ab1ffd99df7fffb74869dc6a358f6091709a46b5327ae
```

Runtime Job:

```text
job-01a00faf-e682-70d1-b770-d8dd1f025066
```

The final receipt establishes all of:

```text
responseDroppedAfterExecAdmission = true
unknownStatePersisted             = true
oneWorkspaceExecInvocation        = true
oneRuntimeJobForClientRequestId   = true
originalRuntimeJobObserved        = true
noRedispatchAfterUnknown          = true
exactContentVerified              = true
taskCompleted                     = true
runtimeWorkspaceClosed            = true
noProviderSessionPersisted        = true
```

The observed Runtime Job itself reported:

```text
executionTerminal            = true
executionDisposition         = succeeded
deliveryDisposition          = committed
recoveryRequired             = false
resultAvailable              = true
semanticCompletionEvaluated  = false
```

This is the strongest R1-R3 engineering-consumption evidence so far because it crosses:

```text
Host domain state
-> shared Protocol contract normalization
-> live MCP discovery
-> Runtime durable admission
-> intentionally lost response
-> exact Job recovery
-> Runtime terminal evidence
-> Host verification
-> Workspace closure
```

without a second physical execution.

---

## 8. Host R3 publication hardening

R2 Host behavior commit:

```text
5f76055dc73c80c3ed529c80aec993b02f096c4f
host: consume exact runtime delivery semantics
```

R3 Host hardening commit:

```text
8d7e58a0511734a454805e29d10e7d3bb754d2da
host: harden runtime integration contract
```

R3 adds:

1. exact status-only fail-closed contract tests;
2. acceptance-helper reuse of `is_missing_workspace`;
3. Host's exact Git dependency pin moved from old Computing Protocol revision `420dc356...` to the proven `5f9b418...`;
4. matching `uv.lock` update.

The Host publication gate passed:

```text
uv lock --check --offline
scripts/check_dependencies.py
scripts/check_docs.py
Ruff
full unittest discovery with MCP optional dependency
```

and the separate live response-loss acceptance passed against production Runtime.

### Publication precondition

`8d7e58a` pins an exact Git commit in the Computing repository. Therefore the Computing commit `5f9b418...` must be published/reachable before the Host candidate is installed or deployed.

This is an explicit dependency edge, not an atomic global transaction requirement.

---

## 9. Harness R3 hardening

R2 behavior commit:

```text
e760caaa0bdc0999c7b748ab627a37d9f6c8fbe3
harness: consume exact runtime delivery semantics
```

R3 hardening commit:

```text
7277e1074be83fa38c28cd8170c28d6f4223146e
harness: freeze exact runtime observation contract
```

The new test prevents status-only Runtime projections from becoming accepted terminal observations.

Full Harness suite after R3 hardening:

```text
424 tests total
3 skipped
0 failed
Ruff clean
git diff --check clean
```

### Why Harness does not need the Computing `patternProperties` fix for R2

Harness does pin `ordivon-protocol`, but the audited R2 SQLite Runtime bridge does not feed Runtime `workspace.exec.inputSchema` into `normalize_mcp_tool_contract`.

Its native-tool semantics layer keeps Runtime descriptors as data and constructs Harness-owned ToolContracts from Harness-owned model input schemas plus selected Runtime output evidence. No demonstrated R2 path hits the Host blocker.

Therefore R3 does not update the Harness Protocol pin merely for symmetry.

Reopen that dependency decision only when a Harness consumer begins normalizing the relevant live Runtime input schema or another concrete Protocol change is required.

---

## 10. Security R3 hardening

R2 behavior commit:

```text
471b892bd3cec493eb1847c5998eed0d8db55ffd
security: gate runtime success on exact delivery
```

R3 hardening commit:

```text
5524d937607f3ddd6ce8eab61dda986436a3f625
security: freeze exact runtime admission contract
```

R3 adds an explicit status-only fail-closed regression test and closes one publication-lint debt in the R2 error message.

Validation:

```text
Security unit suite: 424 passed / 0 failed
Ruff 0.16.2: clean
git diff --check: clean
```

Security does not package-pin `ordivon-protocol` for this Runtime admission path, so no Computing publication edge is introduced by R3.

---

## 11. Finance remains the control

Finance already consumes exact Runtime terminal/delivery evidence and owns its own research/domain admission. R3 found no new publication blocker in that relationship.

No Finance change is admitted.

---

## 12. Exact integration/publication DAG

R3 recommends the following partial order.

### Lane A — Computing -> Host

```text
1. publish Computing 5f9b418
2. publish Host R2 5f76055 + R3 8d7e58a
3. run Host live guarded-mutation acceptance against current Runtime
4. only then consider Host service deployment/publication complete
```

The order is required because Host `8d7e58a` pins Computing `5f9b418` exactly.

### Lane B — Harness

```text
publish e760caa -> 7277e10
```

This may proceed independently of Lane A and Runtime R1, provided Runtime remains at or above the exact-field contract floor.

### Lane C — Security

```text
publish 471b892 -> 5524d93
```

Also independent of Lane A and Runtime R1, under the same Runtime exact-field floor.

### Lane D — Runtime R1

```text
publish a8f0740
```

May happen before or after lanes A-C. It is additive and no current R2 consumer depends on `operationDigest`.

### Lane E — Finance

```text
no change
```

---

## 13. Rollback rules after publication

### Computing/Host

Because Host R3 has an exact Git pin:

```text
Host 8d7e58a requires Computing 5f9b418 to remain reachable.
```

Rolling Computing repository HEAD forward/backward does not alter already pinned Host dependency semantics, but deleting/unpublishing that Git object would violate the dependency contract.

Host rollback to pre-R2 also removes the strict consumer behavior and the new Protocol pin together if performed by exact release rollback.

### Runtime

Runtime R1 rollback is independent:

```text
R1 operationDigest present -> absent
```

is safe for R2 consumers because they do not consume it.

Runtime rollback below the R2 exact-field floor is **not** independently safe once strict consumers are published. Such a rollback must be coordinated with consumer rollback or a separately proven compatibility adapter.

### Harness/Security

Both may be independently rolled back to their pre-R2 releases if required; they do not change Runtime durable state formats.

---

## 14. What R3 deliberately did not build

R3 did not add:

- a shared cross-repository Runtime-semantics SDK;
- a `status` compatibility fallback for unsupported old Runtime releases;
- mandatory downstream `operationDigest` persistence;
- an atomic multi-repository release coordinator;
- a new Runtime version-negotiation protocol;
- a generic JSON Schema implementation in `anc_tool_contract`;
- speculative migrations in Game/World/Web/Studio/Finance.

Each was rejected because current evidence does not justify the complexity.

---

## 15. R3 engineering verdict

R3 validates a more complete engineering-consumption chain:

```text
Frozen Foundation distinction
        ↓
Runtime exact projection
        ↓
shared contract representation
        ↓
consumer translation
        ↓
consumer/domain action
        ↓
acceptance model
        ↓
real deployment/runtime behavior
```

A system can fail at **any** of these boundaries even when the Runtime Core itself is correct.

R3 found two concrete integration mismatches:

```text
1. live Runtime JSON Schema
   x old shared ToolContract normalizer

2. precise Runtime missing-Workspace error
   x stale Host acceptance helper
```

Both were corrected at the narrowest owner boundary. Neither required reopening Runtime Foundations or refactoring Runtime Core.

The final response-loss falsifier then proved the load-bearing behavior end-to-end.

### Final classification

```text
Runtime R1 additive change             publication-ready
Runtime R2 exact field floor           production + rollback compatible
Computing patternProperties support    real mismatch fixed
Host R2 consumer semantics             real mismatch fixed
Host R3 publication/acceptance         hardened and live-proven
Harness R2 consumer semantics          fixed
Harness R3 strict contract             hardened
Security R2 consumer semantics         fixed
Security R3 strict contract            hardened
Finance                                already-correct
operationDigest downstream rollout     deferred, no concrete consumer need
```

No FoundationReopenCondition was triggered.

---

## 16. Integration status at R3 closeout

All R1-R3 engineering candidates are committed in isolated Runtime Workspaces and clean, but canonical repositories remain unchanged:

```text
Computing canonical: acd896688c26a5ea0dad22d66c7ea30882b1b245
Host canonical:      942bf41cb4f3ef6bfc2b70644f269fe6251b10a7
Harness canonical:   286985c82874d293308297f66b23152c1ed53369
Security canonical:  6a7a8f9b22cb4995d436da2968b135248f8f6bb3
Runtime canonical:   c6e45d9e41d3b4d64b5b3dace01497c53e574026
```

R3 therefore closes at **integration-ready**, not silently merged/deployed.

A subsequent publication round can execute the DAG above without repeating RF0-RF16, R1 mapping, R2 consumer audit, or R3 compatibility research.
