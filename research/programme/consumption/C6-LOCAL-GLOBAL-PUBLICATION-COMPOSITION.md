# C6 — Local / Global Composition & Publication Pipeline

## Status

`THEORY_CONFIRMED_END_TO_END_SUCCESS_IS_COMPOSED_NOT_ATOMIC`

C6 tests Local/Global Non-Lifting, Non-Upgrading Composition and Typed Order Grammar against a real multi-layer publication pipeline.

No Runtime Foundation reopen condition is admitted.

## Real historical pipeline

C6 reuses Branch J:

`task:ordivon-atlas-end-to-end-live-transition-dogfood-20260818@rev5`

The tested owner was Runtime; the downstream consumer was Atlas.

The durable acceptance evidence is Atlas commit:

`276ff15c6a157c548330e6eccd013bc71e446be6`

with baseline Atlas commit:

`e223f9f31eb5f8b912e2a9e420e390bc86240a5a`.

## Stage identities

The pipeline contains distinct identities/claims:

```text
L  local candidate commit / publication candidate
D  remote Git delivery / ref advancement
A  owner AuthorityVersionRef + CURRENT relation
P  immutable owner publication payload/source fence
X  retained Atlas projection
R  Atlas refresh realization
X' refreshed Atlas projection
```

These are connected, but are not one identity or one state.

## Case 1 — local success without publication

Branch J created a first Runtime v5 candidate:

```text
local candidate commit = 8bcb5cb
candidate AuthorityVersionRef = sha256:a3497b4440734dccec6e170e99306fa189c2ae34fa87386d8376b96c7e10433a
```

The local candidate existed successfully.

Before publication, exact push lease detected concurrent Runtime main advancement to:

`6724b6c7a7a2832cdb77eeb792bf23ca79861ad5`

The push was rejected:

`REJECTED_BY_EXACT_LEASE; NOT PUBLISHED; NOT AUTHORITY`.

Therefore:

```text
LocalCandidateExists = true
RemoteDelivery = false
OwnerAuthorityAdvanced = false
```

This is direct evidence that:

`LocalCommitSuccess != PublicationSuccess`.

A local commit cannot be promoted into remote/owner success merely because its bytes are valid.

## Case 2 — owner publication succeeds while downstream projection is stale

J reconciled the concurrent Runtime changes and rebuilt/published v5 safely.

Published transport revision:

`ee92d11eeede13aaff68880d9c395e23b617571e`

Published AuthorityVersionRef:

`sha256:e06cac5f69942068fabe80dc5da22fc1fb566d3004ce4951df545534fda289d9`

Previous AuthorityVersionRef:

`sha256:227cc7e253de5fa10be7cbecdfd2e7d84724b507c4a0504836fc63996ac53497`

At this point owner-native publication had succeeded and `CURRENT` had advanced.

But before Atlas refresh, the retained Atlas Runtime projection still referenced v4 `227c...` and evaluated:

`SOURCE_ADVANCED_STALE`.

Network remained independently:

`CURRENT_TO_SOURCE`.

Therefore:

```text
RemoteDelivery = true
OwnerCurrentAdvanced = true
DownstreamProjectionFresh = false
```

This state is valid and expected.

Thus:

`OwnerPublicationSuccess != DownstreamProjectionConvergence`.

## Case 3 — downstream refresh converges later

Atlas refresh then observed the owner v5 authority and generated a new projection.

Post-refresh Runtime:

```text
current AuthorityVersionRef = e06c...
projection health = CURRENT_TO_SOURCE
previous projected AuthorityVersionRef = 227c...
previous projection currentness = SOURCE_ADVANCED_STALE
```

Network remained stable.

The owner authority history preserved all Runtime v1-v5 versions rather than collapsing them into one rewritten state.

This supports:

`DownstreamConvergence` only after a distinct refresh/observation realization.

## Publication pipeline as explicit claim bridges

A correct composition can be represented as:

```text
C1: LocalCandidateExists(L)
   supported by local Git object/history

B1: exact concurrency/lease condition permits delivery

C2: RemoteRefContains(L')
   supported by remote Git observation

B2: owner CURRENT + immutable publication validates AuthorityVersion A

C3: OwnerAuthorityCurrent(A)
   supported by owner-native publication surface

B3: downstream Atlas observes/verifies A

C4: AtlasProjectionCurrent(X', A)
   supported by Atlas refresh/currentness evaluation
```

No bridge may be skipped by borrowing evidence from another layer.

## Why this is not one transaction

The observed pipeline admits meaningful intermediate states:

```text
LOCAL_ONLY
OWNER_PUBLISHED_DOWNSTREAM_STALE
FULLY_CONVERGED
```

It can also admit:

```text
DELIVERY_UNKNOWN
REMOTE_CONFLICT
BROKEN_OWNER_POINTER
DOWNSTREAM_CURRENTNESS_UNKNOWN
```

A universal transaction/state machine would either erase these distinctions or falsely imply atomicity across systems with different owners and evidence authorities.

## Typed order result

C6 requires several order relations:

- local Git ancestry/order;
- remote ref advancement order;
- owner AuthorityVersion succession;
- Atlas observation/refresh order;
- projection currentness relation.

The real trace contains causal/dependency edges, for example owner publication must exist before Atlas can project that version.

But these do not justify one universal total order over all owner/system events. Network remained stable while Runtime advanced, demonstrating independent owner order.

Therefore:

`PipelineDependencyOrder != UniversalRuntimeTotalOrder`.

## F2 — Lost acknowledgement

Branch H already supplied the delivery ambiguity variant: an HTTPS Git push timed out with no success proof and remote state had to be observed before deciding whether retry was admissible.

C6 imports that result into the pipeline grammar:

```text
local push attempt timeout
!= remote delivery absent
!= owner publication absent
```

Delivery must be resolved at the transport/effect layer before later composition claims are admitted.

## F4 — Local / Global Non-Lifting

**PASS / strongly confirmed.**

Three independent counterexamples exist:

1. local candidate exists but is not published;
2. owner publication is current while retained Atlas projection is stale;
3. fresh Atlas projection does not become owner truth or universal Goal/World truth.

## F9 — Non-Upgrading Composition

**PASS / strongly confirmed.**

Even when all stages eventually succeed, end-to-end convergence is not obtained by simply adding local success bits.

It requires explicit bridge evidence at each transition.

The composition rule is therefore closer to:

```text
EndToEndClaim(C)
  = Compose(
      scoped stage claims,
      explicit bridge proofs,
      compatible identities,
      compatible horizons,
      current standing
    )
```

and not:

`GlobalSuccess = Success1 + Success2 + Success3 + ...`.

## Strong negative result: one `success` flag is lossy

A single `publicationSucceeded=true` cannot distinguish:

- local candidate only;
- remote delivery known;
- owner semantic authority advanced;
- downstream projection stale;
- downstream projection converged.

Therefore any consumer that needs recovery/currentness must expose the layer or claim subject of success.

## Engineering consequence

A publication/realization pipeline should preserve distinct observations such as:

```text
LocalCandidateDisposition
DeliveryDisposition / DeliveryKnowledge
RemoteRefObservation
AuthorityVersionRef
OwnerAuthorityStanding
PublicationSourceFence
DownstreamProjectionHealth
```

This is a research/engineering grammar, not a mandate for one universal schema.

The key operational discipline is:

> local success may establish the precondition for the next layer; it does not establish the next layer's claim.

## Theory disposition

- Local / Global Non-Lifting: **strongly confirmed**.
- Non-Upgrading Composition: **strongly confirmed**.
- Typed Order Grammar: **confirmed in a live multi-owner pipeline**.
- Identity / Lineage Conservation: **required across candidate/delivery/authority/projection identities**.
- Scope Conservation: **required at every bridge**.
- F2 Lost Acknowledgement: **confirmed as a transport-layer ambiguity case**.

No R1-R8 Runtime Foundation reopen condition is triggered.

## Broader synthesis

C5 showed that claims need typed support.

C6 shows how such claims compose across a real pipeline:

`Global operational legitimacy is a supported composition of scoped claims, not the aggregation of local success states.`

This remains part of Operational Realization Theory rather than a new project candidate.
