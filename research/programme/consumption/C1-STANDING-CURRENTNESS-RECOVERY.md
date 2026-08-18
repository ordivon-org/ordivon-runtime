# C1 — Standing / Currentness Recovery Consumption

## Status

`THEORY_CONFIRMED_WITH_LIVE_CURRENTNESS_TRANSITION`

No Runtime Foundation reopen condition is admitted by this case.

## Consumer / problem

A reconnecting Ordivon research consumer may possess all of the following simultaneously:

- an old Host research checkpoint that remains exact historical continuity evidence;
- immutable owner publications whose digests still verify;
- a retained downstream projection bound to an older AuthorityVersionRef;
- a newer owner-declared `CURRENT` AuthorityVersionRef;
- Git history that has advanced beyond one or more publication source fences.

The consumer must recover current semantic authority without deleting history, laundering Git recency into semantic authority, or pretending a publication supports source changes outside its exact source fence.

## Live case, not synthetic fixture

C1 began with Runtime main at:

`6724b6c7a7a2832cdb77eeb792bf23ca79861ad5`

At that observation horizon, owner `CURRENT.json` declared:

- current authority version: `sha256:227cc7e253de5fa10be7cbecdfd2e7d84724b507c4a0504836fc63996ac53497`;
- previous authority version: `sha256:350d042df3c01399cf9314c6954f0c3f4c45bdeb660aa275556e098a77ec62eb`.

The current publication `227c...` fenced source revision:

`95116717e576b078ecd57338aea85b7d372946b3`

The previous `350d...` publication fenced:

`2e00028bfd285e8f4e793c41a19564ed6e14324e`

Both payloads were byte-valid and historically addressable. The retained `350d...` authority version was therefore historical but not current; `227c...` matched live `CURRENT`.

While C1 was being executed, an independent Runtime owner-publication transition advanced remote main to `ee92d11...`, added publication:

`sha256:e06cac5f69942068fabe80dc5da22fc1fb566d3004ce4951df545534fda289d9`

and changed `CURRENT` to:

```text
current  = e06c...
previous = 227c...
```

The C1 work and that authority transition touched disjoint paths and were reconciled without rewriting either lineage. After merge, the C1 observation head was:

`a2b35c7ef44e62acfbd3c2e819869c8689489207`

The new `e06c...` publication fences source revision:

`6724b6c7a7a2832cdb77eeb792bf23ca79861ad5`

which contains Programme Formation v1 but predates the C1 result itself.

This yielded a real two-epoch currentness transition.

## Epoch A

```text
live CURRENT = 227c...

projected 227c...
  integrity = VALID
  projection health = CURRENT_TO_SOURCE
  standing = CURRENT_DECLARED

projected 350d...
  integrity = VALID
  projection health = SOURCE_ADVANCED_STALE
  standing = HISTORICAL_NOT_CURRENT
```

## Epoch B — owner authority advances during C1

```text
live CURRENT = e06c...

projected e06c...
  integrity = VALID
  projection health = CURRENT_TO_SOURCE
  standing = CURRENT_DECLARED

retained 227c...
  integrity = VALID
  projection health = SOURCE_ADVANCED_STALE
  standing = HISTORICAL_NOT_CURRENT

retained 350d...
  integrity = VALID
  projection health = SOURCE_ADVANCED_STALE
  standing = HISTORICAL_NOT_CURRENT
```

Nothing about the bytes, digest validity or historical truth of `227c...` changed between Epoch A and Epoch B. What changed was its relation to the owner's live AuthorityVersionRef.

That is the central C1 observation.

## Host continuity cross-check

The historical Host consolidation checkpoint `task:runtime-research-core-consolidation-branch-20260818@rev3` is still exactly recoverable and says physical materialization remained pending at that historical horizon.

A later Host materialization checkpoint `task:runtime-research-materialization-repair-20260818@rev3` records that materialization completed at `7af1b0d5...`.

Both records remain valid historical continuity facts. Neither is owner semantic currentness merely because it remains readable or recoverable.

## Operational claim under test

> A recovery consumer can distinguish historical continuity, owner-declared authority currentness, published source support and newer observed Git history without allowing any one of those facts to silently become all the others.

## Identity

Distinct identities must remain distinct:

- Host continuity task/revision;
- owner AuthorityRef;
- AuthorityVersionRef;
- retained projection AuthorityVersionRef;
- publication payload;
- publication sourceRevision;
- Git branch tip;
- individual ResultRef / closeout standing.

A newer Git commit is not a newer AuthorityVersionRef unless the owner publishes it as such.

## Scope

At least three scopes are required:

1. **Continuity history/currentness** — Host facts about a work lineage.
2. **Authority currentness** — whether a retained/projected AuthorityVersionRef equals the owner's live `CURRENT` AuthorityVersionRef.
3. **Published source horizon** — the exact sourceRevision that supports a publication's claims, distinct from later Git history.

A scalar `current=true` is insufficient.

## Support

- Host provides exact continuity/control provenance but explicitly does not validate current Git/domain truth.
- Git provides exact byte/history ancestry and branch-tip observations but does not mint semantic authority.
- Owner `CURRENT` declares the live AuthorityVersionRef.
- Publication `sourceRevision` fences the corpus horizon supporting that AuthorityVersionRef's claims.
- Atlas/currentness consumers compare a retained projection's AuthorityVersionRef against live owner `CURRENT`; timestamps and raw branch-tip recency are not the authority rule.

## Source-fence clarification

During Epoch A, publication `227c...` fenced `9511671...` while observed Git main was already `6724b6c...`. That fact **did not** make `227c...` stale. It remained `CURRENT_TO_SOURCE` because its AuthorityVersionRef matched live `CURRENT`.

During Epoch B, publication `e06c...` fences `6724b6c...` while the C1 merge head is later. That also does not revoke `e06c...` currentness.

The relation:

```text
publication sourceRevision
    versus
observed Git tip
```

is source-horizon/provenance information, not owner semantic currentness by itself. Post-fence Git changes are simply outside that publication's support scope until the owner publishes them.

## Falsifier results

### F3 — Stale authority

**PASS / theory confirmed.**

`227c...` moved from current to historical without any corruption or deletion. Physical readability and integrity do not imply current standing.

### F6 — Restore / recovery

**PASS / theory confirmed.**

Recovering an old Host checkpoint or old AuthorityVersionRef cannot safely restore its former semantic standing. Recovery must re-resolve live owner authority and construct an admissible continuation.

### F7 — Historical / currentness

**PASS / theory strongly confirmed.**

C1 observed a real AuthorityVersionRef transition while running. The same immutable `227c...` object was `CURRENT_TO_SOURCE` in Epoch A and `SOURCE_ADVANCED_STALE` in Epoch B solely because owner `CURRENT` advanced to `e06c...`.

This is direct evidence that:

`HistoricalValidity != Currentness`.

## Implementation hazard discovered and corrected during C1

An initial C1 probe incorrectly attempted to derive `SOURCE_ADVANCED_STALE` from:

```text
publication.sourceRevision != Git HEAD
```

That rule was rejected before C1 closeout because it launders Git recency into semantic authority and would misclassify healthy publications.

The corrected probe derives authority projection health from:

```text
projected AuthorityVersionRef
        versus
live owner CURRENT AuthorityVersionRef
```

and reports publication-source-versus-Git-tip relation only as separate diagnostic provenance.

This self-correction is part of the C1 evidence, not erased history.

## Minimal engineering consequence

Any Research/Atlas/Host recovery adapter consuming owner research currentness should keep separate fields for at least:

```text
ContinuityFact
ProjectedAuthorityVersionRef
LiveAuthorityVersionRef
AuthorityProjectionHealth
PublicationSourceRevision
ObservedSourceTip (optional diagnostic)
CurrentRecovery locator/role
```

`AuthorityProjectionHealth` should preserve the existing fail-closed states:

- `CURRENT_TO_SOURCE`;
- `SOURCE_ADVANCED_STALE`;
- `BROKEN_POINTER`;
- `AUTHORITY_CHANGED_UNRESOLVED`;
- `CURRENTNESS_UNKNOWN`.

The publication-source/tip relation must not redefine semantic standing.

## Theory disposition

- Scope Conservation: **confirmed**.
- Identity / Lineage Conservation: **confirmed**.
- History / Reproduction Non-Identity: **confirmed**.
- Evidence / Truth Non-Lifting: **confirmed**.
- Orthogonality: **confirmed**.
- Open-World Re-grounding: **confirmed**.
- Standing / Currentness Semantics: **materially strengthened as a cross-owner research candidate**.

No R1–R8 Foundation reopen condition is triggered.

The observed implementation issue was in the first probe's currentness classifier, not in the Runtime theory. It was repaired minimally before durability closeout.

## Next admissible action

Use `tools/currentness_probe.py` as the C1 regression probe. At C1 closeout it must classify:

- live `e06c...` => `CURRENT_TO_SOURCE`;
- retained `227c...` => `SOURCE_ADVANCED_STALE`;
- retained `350d...` => `SOURCE_ADVANCED_STALE`;
- all three immutable publication digests => valid.

C1 itself is newer than the `e06c...` publication source fence. That is acceptable: C1 remains new owner source evidence until a future owner publication chooses to include it. Recovery must not mint that publication automatically.
