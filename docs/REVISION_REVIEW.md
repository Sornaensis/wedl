# Three-way revision review v1

Revision review compares **three original Git commits at one exact story time**. It reports authored source changes and five kinds of divergent conflicts. It does not construct a merged world, publish a candidate, retain a review record, or supply an apply operation.

The implementation is `src/revision_review.rs`. The native entry points are `revision_review::decode(raw)` and `revision_review::review(root, &request)`. The CLI, service operation, author HTTP endpoint and author UI call this same engine. The report uses `wedl-rust/v1`; this is an evolving v1 contract, without aliases or migration adapters.

## Closed request

Every field is required. Unknown and duplicate JSON fields are rejected, including duplicates inside `at`.

```json
{
  "baseRevision": "eb4e4672316025a63cc31bf52118dc4c369c9290",
  "leftRevision": "893add06ffc7a279f0bac908b42349fd28f23502",
  "rightRevision": "65439949e405b6dffb267a2451776c5387b06a4b",
  "at": {"timeline": "main", "tick": "10", "order": "0"},
  "maxItems": 100,
  "maxCharacters": 100000
}
```

These full commit IDs belong to the reproducible [branch example](../examples/revisions/README.md). Create that fixture before using its request.

| Field | Contract |
| --- | --- |
| `baseRevision`, `leftRevision`, `rightRevision` | Full lowercase 40- or 64-hex object IDs which resolve to actual commit objects. Branch names, tags, abbreviated hashes, expressions and tree/blob IDs are rejected. |
| `at.timeline` | A nonblank exact timeline ID present in every original world, at most 128 UTF-8 bytes. |
| `at.tick` | Canonical decimal string within signed 64-bit range. Numeric JSON tokens, leading zeroes and `-0` are rejected. |
| `at.order` | Canonical decimal string within signed 32-bit range. |
| `maxItems` | Integer from 1 through 1000. Counts `changes` plus `conflicts`, together. |
| `maxCharacters` | Integer from 256 through 1,000,000. Bounds the complete serialized result, including metadata and hashes. |

The base must be an ancestor of both other commits. Equality is allowed: either or both branch pins may equal the base. The original commit-parent graph is consulted directly, with bounded object reads. Mutable grafts, replacement refs, shallow-parent rewriting, branch selection and live `HEAD` do not define ancestry for this operation.

All three sources must have the same exact world ID, valid current native source grammar, unique record IDs and case-insensitively distinct source paths. Each commit's original tree and blobs are loaded independently. The operation does not load worktree source, rebuild a cache, recover a publication journal or open an author draft store. The fixture deliberately makes the live world file invalid after creating the three commits, and review still succeeds.

The CLI is `wedl-rs --repo ROOT revision-review --file REQUEST.json`. Its request already contains three pins, so combining it with `--revision` is rejected. The author HTTP read is `GET /api/revision-review?request=URL_ENCODED_JSON`, under the existing loopback/local-author HTTP boundary with Host/Origin checks; the native server does not provide authentication or multiuser authorization. No other query fields are accepted. The native result is in the transport's `result` field. The transport omits unrelated live revision and fingerprint metadata.

## What is compared

Each validated original is admitted independently at the same `at`. The internal author scope includes audiences `public` and `author`, and perspectives `ordinary` and `author`. There is no request field which expands this scope. This is an author review, not a character knowledge query.

Source comparison recursively separates nonempty mappings into fields. Ordered arrays remain whole-member cells. Native description, effect, transition, observation, conversation and declaration identities match members within their exact owning record; generational transitions use their native `transition_id`. Empty arrays and mappings remain explicit values. Member source evidence retains its original ordinal even when earlier members are excluded at the horizon. Identity-free members compare in admitted array order.

Stable member arrays also compare the relative order of identities present in both branches. Independent insertions do not create an order conflict. A divergent reorder does, with each original admitted array displayed as evidence. Duplicate selected identities are refused rather than matched ambiguously. Source-path moves are reported separately from changed content.

The exact opaque Markdown body is one `bodyMarkdown` cell. Line endings and trailing whitespace are preserved. Native dated `descriptions` are separate source members; body headings are not converted into dated sections and no compatibility body parser is used. Applicable description and spatial-history correction members remain available as authored history, including predecessors, rather than being replaced by a current-winner-only rendering.

World declarations are source too: profiles, predicates, state definitions and bindings, relationship metrics and chronology can change independently of any record using them. The reviewer compares their current native authored representation.

### Conflict families

In general, a conflict requires both branches to differ from the base and from each other. One-sided edits and convergent edits are changes without a conflict.

| `kind` | Meaning and evidence |
| --- | --- |
| `text` | A divergent source cell, literal Markdown body, or shared-member order. Each side has `present`, `encoding`, `text` and an original citation. The encoding is `markdown` or `canonical-json`; absence has null text and citation. |
| `duplicate-id` | Both branches introduce an ID absent from the base, with different admitted content or source paths. It carries the two original record versions. Identical introductions at the same path converge. |
| `divergent-transition` | Both branches change the same owner-local native transition identity differently. It carries the exact owner/member reference and original citations for each present side. |
| `state` | The original native state folds at `at` produce three different presence/value results for one entity/key. It carries the native typed state subject, values and original provenance. |
| `active-scene` | Active scene identities diverge for a participating character, or for global singleton selection. Each side carries its scene versions and original source citations. |

Deleting an owner while the other branch modifies it can produce a text or transition conflict. Deletion alone is not a separate conflict family. Moving a record without changing its admitted content produces a `moved` classification and no fabricated changed field.

State uses the native fold on the **complete original world**, with scalar entity-effect admission at the selected horizon. No synthetic world with future records removed replaces it. Comparison ignores provenance-only differences. It preserves native scalar types and exact large integers; integer `1`, floating `1.0`, boolean `true` and string `"1"` are not silently coerced. Ordinary set state compares membership, while bound profile values retain their declared native structure and ordering. State evidence includes the native browser-safe payload and exact `valueRaw` representation.

Narrative source admission and local-author semantic scope are deliberately distinct. The state fold includes every native semantic owner and canonical event through `at`, independently of narrative audience/perspective labels. A state conflict can therefore appear even when the corresponding narrative source cell is withheld and `changes` is empty. `semantics.stateScope` records this complete-original author scope. Typed state references remain exact native operands from original source, including references to records whose narrative content is outside the selected horizon; they do not assert that the target's future content is present at `at`. `semantics.stateReferences` states this native policy. No horizon-pruned compatibility world or character-admission rule is substituted into state replay.

Active scenes use exact inclusive presence bounds and participant windows. An active scene without an end has presence only through its authored current cursor. Global selection comparison is omitted if any original has more than one active scene; a split front is not interpreted as a singleton. Character participation remains comparable independently. Valid source must still satisfy the native prohibition on overlapping live scene membership for the same character.

## Horizon and reference admission

| Source family | Review admission |
| --- | --- |
| Ordinary untimed owners | Existing native author label admission. Their source body remains an opaque authored cell. |
| Events | Canonical status and exact event time at or before `at`. |
| Scenes, conversations, environments | Available from their authored start. An ended interval does not erase its source history. Active presence is checked separately for active-scene conflicts. |
| Knowledge | Canonical history with an admitted transition. Assertion- and genealogy-linked claims also require an affirmative native historical state through `at`; later rejection does not erase the earlier authored account. |
| Relationships | An admitted transition, retaining native relationship status behavior. |
| Story points | Native canonical owner admission, with lifecycle transitions through `at`. |
| Lore concepts, assertions, briefs, existence episodes | Native canonical status, validity and author labels. Required endpoints are admitted independently. Historical evidence remains literal source syntax, not a current-target lookup or access grant. |
| Generational histories | Canonical status and initialization through `at`; native transition applicability supplies each member's first point. |
| Overlays | Native static validity or their authored inclusive interval. |

Transitions, observations, turns and recollections after `at` are excluded. Scene/conversation participants with a future `from` are excluded. Description, concept-link and spatial-history members use their declared validity. Applicable correction history is retained; a correction pointer to a withheld predecessor is omitted.

Only declared reference positions participate in reference admission. The reviewer reuses the native core, lore, assertion, brief and bound-state extractors, with explicit native set and story-condition operands. It does not discover edges by scanning strings. An opaque paragraph containing a future record's ID remains a paragraph. A withheld typed entity operand is removed as a wrapper; a withheld effect target, participant or concept link removes that whole member. Original array ordinals survive this projection.

Brief `required_refs` and `scope_ref` are conjunctions. A required owner or exact member outside admission withholds the entire brief, including through nested briefs and required assertion endpoints. A partial required-reference array cannot make the brief appear complete. For an otherwise admitted brief, the reviewer invokes native `briefs::collect_full_author` to prove one complete domain closure across author/public audiences and author/ordinary perspectives.

## Explicit refusal boundaries

The reviewer returns an error when it cannot establish a faithful complete comparison. It never returns a truncated conflict list as successful review.

* If an ID exists in one original but is outside that original's admission, while another original admits it, the result is `not_found`. This distinguishes unavailable source from deletion.
* If a compared state owner exists but its declaration or native state projection is unavailable in an original, the result is `unsupported`. Removed state declarations are still not assigned an invented cross-type absence equivalence.
* A mixed author/public brief can close through the shared union collector. Owners, members and typed endpoints pass independent admission within that one closure; a required withheld pin removes the whole brief before comparison. Separate partial compositions never substitute for a complete closure.
* Semantic replay skips only unbound scalar entity effects whose referenced targets are unavailable at the selected horizon, preserving earlier state and original effect ordinals. Initial values, entity sets, bound profiles and expected literals retain their native behavior. `semantics.futureEntityTargetPruning: true` and `stateReferences: native-values-with-scalar-entity-effect-pruning` identify this narrow [temporal rule](SEMANTIC_CONSEQUENCES.md#temporal-entity-operands); source and semantic admission remain separate contracts.
* YAML tags, non-string mapping keys, and ambiguous dotted source/reference positions return an explicit unsupported/unavailable error. No coercion or guessed coordinate is used.
* Invalid source, ancestry failures, wrong object types and exceeded bounds are errors. The existing `Error` code and transport error envelope apply; a failure is not a success-shaped report with missing rows.

The native validator remains partial. `validationComplete: false` is preserved in every successful report. `complete: true` means that this bounded native review result is complete within the documented contracts; it does not certify every upstream WEDL domain.

## Report

The result contains these stable groups:

| Fields | Meaning |
| --- | --- |
| `protocol`, `operation`, `outcome`, `scope` | `wedl-rust/v1`, `revision-review`, `ok`, `author-as-of`. |
| `worldId`, `at`, three `*Revision` fields | Exact world, horizon and original commit pins. |
| `sourceFingerprints` | Independent `base`, `left`, `right` fingerprints of all original canonical source bytes, including source excluded from horizon display. |
| `requestSha256` | Domain-separated SHA-256 binding the complete closed request, including the requested budgets. |
| `changes` | Deterministically ID-ordered record rows with `leftChange`, `rightChange`, original `base`/`left`/`right` versions and changed positions. Classifications are `none`, `added`, `removed`, `modified`, `moved`. |
| `conflicts` | Deterministically ordered rows from the five families above. |
| `writesPerformed`, `canApply` | Both false. These are observations about this read, not an apply authorization. |
| `complete`, `validationComplete` | Complete native result, with partial-validator status stated separately. |
| `bounds`, `semantics` | Requested and hard limits, observed work/positions/citations, exact source/state/body policy, and excluded retcon/merge behavior. |
| `reportSha256` | Domain-separated SHA-256 of the complete result before adding `reportSha256` itself. |

A source position is `{section, sourceOrdinal, memberId}`. A citation carries `entityId`, `sourcePath`, `blobOid`, `revision`, `fingerprint`, `section`, `sourceOrdinal` and `memberId`, where applicable. All revisions and blobs refer to original source. There are no candidate provenance labels, invented merge revisions or citations to a generated result. Native state provenance retains its established native citation shape.

## Complete bounds

Bounds are refusal thresholds, not pagination or truncation controls. Source/work gates run before complete validation and folds. Path-expansion gates run before token indexing; source positions are reserved before allocating each cell and citation. Result rows are priced as they are added, and the entire result is checked again after its final hash is present.

| Resource | Bound |
| --- | --- |
| JSON request text | 64 KiB |
| Original Git source | Shared historical reader: at most 4096 records and 32 MiB per original, 8 MiB per source blob. |
| Three-source aggregate | 8192 records; 64 MiB source bytes; 200,000 YAML nodes; maximum depth 64. Native semantic preflight additionally limits each original to 100,000 nodes. |
| Expanded frontmatter paths | 4096 bytes per path; 8 MiB conservative aggregate expansion, counted before indexing. |
| Replay and projection work | 8,000,000 aggregate estimated/charged units; native fold preflight also caps each original's replay estimate at 2,000,000. |
| Source positions | 4096 per original; 12,288 across the three views, including sequence-order evidence. |
| Native semantic projections | 1024 subjects per original; state subject union also at most 1024. |
| Result rows | `changes.length + conflicts.length <= maxItems`, at most 1000. |
| Changed positions and citations | 4096 changed positions; 16,384 report citation occurrences. |
| Complete result | At most 8 MiB of serialized JSON and at most requested `maxCharacters` Unicode characters, including metadata, escaping and hashes. |

The historical reader separately bounds original commit ancestry at 8192 visited commits and 16 MiB commit bytes per traversal, 256 KiB per commit and 64 parents per commit. These object-read limits apply before source comparison.

## Verification and remaining work

`tests/revision_review.rs` exercises real branching Git histories, the five conflict families, source moves, deletion versus modification, convergent introductions, original blob citations, native temporal/member admission, stable identities and order, large and typed values, set/provenance equality, active split fronts, author scopes, required brief closure, unsupported boundaries, source duplication and complete budget refusal. The same reads preserve every file byte in the fixture, including live source, Git index/refs, private files and cache sentinels. `tests/edit_revision_transport.rs` checks native/CLI/HTTP agreement and transport refusals.

[Typed retcon impact](RETCON_IMPACT.md), [saved retcon decisions](RETCON_REVIEWS.md)
and [event/grouped consequence reports](EVENT_CONSEQUENCES.md) are separate native
contracts. Saved decisions grant no canonical approval. This three-original read
still creates no merge candidate and supplies no merge/apply operation.
