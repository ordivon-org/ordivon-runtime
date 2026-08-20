# C4 — Cross-Owner Standing / Currentness Heterogeneity

## Status

`CROSS_OWNER_THEORY_CONFIRMED_PROMOTION_RECOMMENDED`

C4 tests whether the currentness grammar established in Runtime C1 survives a materially different owner topology: Network lives as an independent semantic owner inside a shared physical research corpus rather than inside its own implementation repository.

No Runtime Foundation reopen condition is admitted.

## Why Network is a heterogeneous test

Runtime C1 used an owner whose research authority surface lives inside the independent `ordivon-runtime` repository.

Network is different:

- Network has no standalone implementation repository;
- its semantic owner corpus lives under `owners/network/` inside shared `ordivon-research` physical durability;
- older Host continuity records predate that shared corpus materialization;
- Atlas consumes Network through an owner-native authority/current surface inside the shared corpus;
- local shared-corpus checkout, remote shared-corpus main, owner publication source fence and owner AuthorityVersionRef can all differ.

If the same standing/currentness semantics survives this topology, the result is not merely a Runtime-local lifecycle property.

## Historical owner materialization sequence

### Host consolidation horizon

`task:network-research-core-consolidation-branch-20260818@rev3`

At this historical horizon:

- NDF0-NDF5 were consolidated/frozen;
- NDF6 was not admitted;
- no standalone Network research repo/root existed;
- canonical semantic continuity lived in Host pending authorized physical materialization.

### Host materialization-repair horizon

`task:network-research-materialization-repair-20260818@rev3`

At this later historical horizon:

- no `/root/projects/ordivon-network` existed;
- no shared `ordivon-research`/Atlas Git root was yet admitted;
- physical materialization correctly stopped with `PHYSICAL LOCATION ESCALATION`.

These statements remain historically valid. They are not current physical-topology truth after later Research System work created the shared owner corpus.

### Owner authority publication horizon

`task:ordivon-research-system-owner-authority-closeout-materialization-20260818@rev3`

Later work established Network owner-native publication in shared corpus:

- initial AuthorityVersionRef `sha256:bfadaaaad3b01f9c4388e4e4a75e77c782c2c3111849e5c4598052ec740ee79f`;
- source revision `0fde1702e18cbaa12896497e13f400f64c7d1756`;
- publication committed/pushed in the shared corpus publication chain beginning at `80bd37393ff46703be1dec6940bd11ea0f7149dc`.

The Atlas MVP subsequently observed `bfada...` as live Network `CURRENT_TO_SOURCE`.

## Current C4 observation

C4 opened the local shared-corpus repository and initially observed local `main`:

`0fde1702e18cbaa12896497e13f400f64c7d1756`

At that local checkout, `owners/network/authority/` was absent. A naive local-only consumer could therefore incorrectly infer that Network had no authority publication surface.

Remote Git observation then established:

`origin/main = edfb2bb80803a586636804015169f0c01d6e9709`

At remote main, Network `CURRENT.json` declares:

```text
live current AuthorityVersionRef
  = sha256:dbdbb759b2b86b898a343cbb81646b283c589676989e919537f1a6cbc2b1df91

previous AuthorityVersionRef
  = sha256:bfadaaaad3b01f9c4388e4e4a75e77c782c2c3111849e5c4598052ec740ee79f
```

The current immutable publication `dbdb...` exists. Its source fence is:

`80bd37393ff46703be1dec6940bd11ea0f7149dc`

The previous immutable publication `bfada...` remains present. Its source fence is:

`0fde1702e18cbaa12896497e13f400f64c7d1756`

The current Network publication was introduced by later authority evolution (`42c641d...`) that preserved the previous publication rather than replacing history.

## The decisive heterogeneous transition

Atlas MVP historical evidence recorded Network:

```text
bfada... = CURRENT_TO_SOURCE
```

C4 live owner authority now declares:

```text
dbdb... = CURRENT
previous = bfada...
```

Therefore a retained Atlas projection bound to `bfada...` must now classify:

```text
SOURCE_ADVANCED_STALE
HISTORICAL_NOT_CURRENT
```

while a fresh projection bound to `dbdb...` classifies:

```text
CURRENT_TO_SOURCE
CURRENT_DECLARED
```

The bytes and historical validity of `bfada...` did not need to change for its standing to change.

This reproduces the central C1 currentness transition under a different owner and physical topology.

## Four typed currentness / horizon roles

C4 strengthens the grammar beyond a scalar `current` field.

### 1. Continuity history

Host task/checkpoint facts answer:

> What was the work/control frontier at that exact historical horizon?

They do not mint present owner semantic authority or present Git topology.

### 2. Transport observation

A local checkout or observed remote ref answers:

> Which Git byte/history horizon was observed at this transport endpoint?

C4 proves local checkout currentness and remote-ref currentness can diverge substantially.

### 3. Authority currentness

Owner `CURRENT` answers:

> Which AuthorityVersionRef is currently declared by the semantic owner?

This is the authority relation used for `CURRENT_TO_SOURCE` versus `SOURCE_ADVANCED_STALE`.

### 4. Publication source horizon

Publication `sourceRevision` answers:

> Which exact source corpus horizon supports this immutable publication's claims?

It is a support-scope fence, not a substitute for AuthorityVersionRef currentness or branch-tip freshness.

## Strong counterexample to recency-based inference

The local C4 checkout is exactly:

`0fde1702...`

which is also the previous `bfada...` publication source revision.

Yet `bfada...` is no longer current because live owner `CURRENT` has advanced to `dbdb...`.

Thus even an exact match between local checkout and a historical publication's source fence does **not** restore that publication's current standing.

Conversely, current publication `dbdb...` is source-fenced at `80bd373...` while remote main is later (`edfb2bb...`). It remains current because currentness is declared through the owner AuthorityVersionRef, not through equality with the latest Git commit.

This gives two opposing falsifiers to any generic recency rule.

## Falsifier results

### F3 — Stale authority

**PASS / cross-owner confirmed.**

`bfada...` remains immutable, present and historically valid but no longer equals owner `CURRENT`.

### F6 — Restore / recovery

**PASS / cross-owner confirmed.**

Restoring the local shared-corpus checkout at `0fde1702...` would restore an old physical source horizon, not current Network semantic authority. Recovery must resolve the current owner authority surface rather than trust restored/local Git state.

### F7 — Historical / currentness

**PASS / strongly cross-owner confirmed.**

The same relation observed in Runtime C1 reproduces in Network despite a different repository topology and owner materialization history.

## Cross-owner law extracted

C1 + C4 now support the following broader formulation:

```text
Standing(x) is not an intrinsic property of retained bytes.
Standing(x) is a typed relation between x and a current authority / validity domain.
```

More concretely:

```text
HistoricalIntegrity(x)
  != CurrentStanding(x)

LocalTransportTip
  != RemoteTransportTip
  != OwnerAuthorityVersion
  != PublicationSourceHorizon
```

These distinctions are not Runtime-specific execution semantics.

## Project-promotion decision

C4 crosses the programme's previously stated threshold for a serious independent-candidate decision:

- Runtime independent-repo live transition: observed in C1;
- Network shared-corpus authority transition: observed in C4;
- Host continuity exhibits historical-but-not-current facts in both owner histories;
- Atlas already consumes the same semantics as a downstream currentness/recovery projection;
- the semantics governs research authority/currentness without requiring Runtime execution ownership.

Therefore:

`Standing / Currentness Semantics = PROMOTION_RECOMMENDED`

This means the topic is now justified as an independent research programme candidate. It does **not** mean Runtime transfers its lifecycle semantics wholesale, nor that a new repository/schema/service should be created automatically.

The initial independent scope should study typed standing/currentness relations across retained historical objects, authority generations, validity domains, source fences and recovery—not generic time/versioning/lifecycle of everything.

## Runtime consequence

Runtime retains its owner-native lifecycle/retention/standing family for Runtime-owned operational claims and resources.

The cross-owner project candidate should own the more abstract question:

> Under what typed authority/validity relation does a historically retained claim/object remain current, usable or authoritative now?

Runtime consumes that abstraction where useful; Host, Atlas, Network and other owners may also consume it without transferring their semantic truth to Runtime.

## Foundation disposition

No Runtime R1-R8 reopen trigger fires.

C4 is an **owner-boundary export**, not evidence for a new Runtime Foundation.
