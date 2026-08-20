# Runtime Dogfood & Case Studies

Dogfood supplies concrete falsification evidence. It does not exist to demonstrate that the theory is correct.

## Priority domains

### Finance / external consequential operations

High-value probes: unknown order outcome, partial fill, response loss, venue reconciliation, stale order/position observation, retry safety.

### External API / provider operations

High-value probes: provider substitution, timeout ambiguity, idempotency-scope mismatch, delayed external effect and stale receipts.

### Durable Agent / Harness interaction

High-value probes: repeated tool calls, cross-agent handoff, context loss, stale observations, false semantic completion from tool-local success.

### Host / continuity interaction

High-value probes: historical READY state vs current semantic authority, recovery of work after owner supersession, currentness/standing distinction.

### Network-dependent realization

High-value probes: communication success without operational resolution, topology/capability changes after admission, split currentness between Runtime and Network observations.

## Case-study rule

A case study must name the concrete operational claim under test and identify its Identity, Scope, Support and Standing. Pure architecture illustration is not dogfood.
