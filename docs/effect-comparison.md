# Effect Commitment Comparison

This document is the Runtime-owned baseline for Issue #56. It does not claim that the Ordivon Effect path has already defeated simpler Tool or durable-Activity designs.

## Research question

Does preserving Effect, Binding, Dispatch, explicit `UNKNOWN`, evidence, and reconciliation prevent materially important failures beyond:

1. a plain MCP `tools/call`;
2. a Tool call plus client request identity, idempotency, and audit;
3. a durable Activity with retry and persisted result;
4. the current Ordivon Effect-to-Runtime path?

## Required backends

The comparison requires two materially different world effects:

- **Runtime source mutation** — exact Git revision, Workspace mutation or execution, Runtime Job and Attempt evidence;
- **external Fetch or Browser** — an Edge-owned structured backend with stable request or Dispatch correlation, receipt lookup, Artifact provenance, and explicit ambiguity.

The Runtime backend exists. The second backend remains owned by existing Edge Issues #24 and #26. Runtime does not implement a substitute browser, proxy, remote fleet, or second Edge adapter merely to complete the experiment.

## Controlled variants

| Variant | Identity and retry behavior | Expected ambiguity behavior |
| --- | --- | --- |
| Plain Tool | New call identity on every invocation | Lost response is indistinguishable from failure unless the backend independently exposes lookup |
| Idempotency plus audit | Stable client request identity and backend audit row | Duplicate admission may be prevented, but Tool drift and semantic rebinding remain variant-specific |
| Durable Activity | Persisted Activity identity with declared retry policy | Recovery follows workflow semantics; non-idempotent activity behavior must remain explicit |
| Ordivon Effect path | Stable Effect, immutable Binding, distinct Dispatch and backend correlation, Observation or Artifact, reconciliation | `UNKNOWN` remains separate from failure and blind redispatch is forbidden |

## Shared fault matrix

Every variant receives the same source or target revision, Tool contract revision, authority or capability input, payload, budget, and completion test. Inject:

- response loss after backend success;
- duplicate delivery before and after backend commitment;
- Host restart;
- Runtime or backend restart;
- Tool contract drift;
- source or target revision drift;
- stale authority or capability profile;
- non-idempotent external behavior;
- malformed or incomplete result evidence.

## Measurements

Runtime can already provide the following mechanical facts without another database:

- admission-to-dispatch latency;
- dispatch-to-Runner binding latency;
- Runner binding-to-terminal latency;
- cancellation-to-terminal latency;
- reconciliation-failure-to-convergence latency;
- duplicate physical dispatches;
- automatic versus administrative recovery;
- unresolved and capacity-held work;
- Artifact count, bytes, truncation, and terminal reason.

The complete experiment must additionally measure world-level duplicate Effects, false success and false failure, unsafe redispatch, fresh-Host recovery, manual reconciliation time, implementation size, storage, and accepted outcome quality. Runtime does not infer those semantic or world facts from process exit.

## Decision rule

Retain an additional Runtime or cross-project commitment object only when its failure reduction exceeds its latency, storage, implementation, and cognitive cost across both backends.

Possible closeouts are deliberately symmetric:

- **retain** the complete Effect path if it prevents repeated serious failures at acceptable cost;
- **shrink** it to identity, explicit `UNKNOWN`, correlation, and reconciliation if other objects add no leverage;
- **reject** a separate Effect layer if ordinary Tool or Activity mechanisms provide equivalent recovery and clarity more simply.

Until the second backend exists, the Runtime-side result is a measured baseline and executable fault design—not a completed two-backend proof.
