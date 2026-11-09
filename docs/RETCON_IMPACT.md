# Native retcon impact — v1

Retcon impact follows the reverse closure of **declared typed source references**
from one exact target at one exact story horizon. It independently validates and
admits the base and candidate originals, reports every supported occurrence in
the complete bounded closure, and writes nothing. It does not infer effects or
approve canonical changes. Every successful report has `canApply: false` and
`writesPerformed: false`.

## Request and entry points

The closed preview request is:

```json
{
  "plan": {
    "expectedFingerprint": "<64 lowercase hexadecimal characters>",
    "operations": [
      {
        "type": "record.update",
        "entityId": "char_00000000000000000000000001",
        "patch": "kind: character\nbody: An explicitly proposed revision.\n"
      }
    ]
  },
  "impactRequest": {
    "target": {"kind": "record", "id": "char_00000000000000000000000001"},
    "at": {"timeline": "main", "tick": "10", "order": "0"},
    "depth": 32,
    "maxItems": 1000
  }
}
```

`plan` is the existing native consequence plan. It has no impact-specific field.
Its optional semantic analysis retains its existing grammar. The complete final
candidate must validate; retained **required** semantic checks must permit the
analysis. The existing source-and-diff consequence preview remains separately
available. Retcon preview returns the impact report only.

- CLI: `retcon-impact preview --file REQUEST.json`, using the selected world.
- HTTP: `GET /api/retcon-impact/preview?request=JSON` on the author surface.
- Rust: `decode_preview`, `preview`, and the pure `evaluate` and
  `evaluate_with_authors` operations in `retcon_impact`.

Current preview uses bounded exact source capture and the native publication
fence. A changed or forged captured `World` is refused. An explicitly selected
historical world remains bound to its immutable revision. The adapter captures
the private manuscript source from that same revision, or the current private
source alongside a current world, and fences both current source sets. Missing
private directories represent an empty author snapshot.

Pure `evaluate` accepts two in-memory canonical originals and validates both
itself. Pure `evaluate_with_authors` additionally accepts one exact
`manuscript::Snapshot`, validates it against the base, and uses the same private
source in both graphs. Neither operation reads HEAD or the filesystem. A
consequence plan changes canonical records only; candidate removal of a former
manuscript target removes the candidate edge without invalidating the original
private source evidence.

All objects are closed. Recursive duplicate JSON fields, including escaped
spellings inside tagged references and nested plan operations, are rejected.
Time coordinates are canonical decimal strings: signed 64-bit `tick` and signed
32-bit `order`. Numeric JSON tokens, leading zeros, and out-of-range coordinates
are refused. The selected timeline must be declared in both originals.

## Exact targets

References use native **source-style snake_case** member fields. A target never
resolves by title, alias, suffix, or an ID found in prose.

| `kind` | Required fields besides `kind` | Meaning |
| --- | --- | --- |
| `record` | `id` | An admitted native record |
| `section` | `owner_id`, `id` | An admitted explicit description/spatial-history member or world calendar, era, or anchor |
| `transition` | `owner_id`, `id` | An admitted knowledge, relationship, story-point lifecycle, or generational transition/initialization |
| `knowledge` | `entity_id`, `transition_id` | An exact knowledge history item used as live evidence |
| `observation` | `entity_id`, `observation_id` | An exact scene observation |
| `turn` | `entity_id`, `turn_id` | An exact conversation speech turn |
| `recollection` | `entity_id`, `recollection_id` | An exact conversation recollection |
| `author-record` | `id` | An admitted exact manuscript segment or work from the supplied private snapshot |

Every identity is a nonblank exact string of at most 256 bytes. A member ID is
scoped by its owner and kind. The same spelling in another owner does not match.
Only speech turns are live `turn` targets. Effect IDs identify source occurrences
in citations; there is no effect target variant.

Canonical-only `evaluate` has no private author nodes. The author-aware pure
operation and preview adapter support manuscript/work nodes. Markdown headings
are not converted into sections: the body remains one exact opaque
`bodyMarkdown` value, and dated sections use explicit native source members.

## Admission and traversal

Each original uses the established native source admission helpers. The author
scope is the union of `public` and `author` audiences and `ordinary` and `author`
perspectives. This is source history at the horizon, not an active-presence
query or an actor's epistemic read.

Timed owners and members must have begun at or before the selected horizon.
Applicable descriptions and spatial-history corrections retain their authored
history. Native validity, canonical-status, applicability, and linked-knowledge
rules remain authoritative. Hypotheses are nonoperative and excluded before
dependency construction. A brief whose required exact pin is withheld is
withheld in full, including through other briefs. A brief closure that cannot
be established by a supported native author lens is explicitly unsupported.

Unavailable canonical typed operands are pruned before source comparison and
graph construction. An effect on an unavailable target, for example, does not supply
an edge or select a state declaration into the witness. Hidden or future
records and members do not become traversal bridges.

Edges come from native reference codecs and exact supported member links:
ordinary entity references; event effects and causes; acquisition provenance;
scene/conversation participants and source members; story-point condition and
dependency operands; generational endpoints, causes and corrections; spatial
references; typed profile/predicate entity leaves; applicable concept links;
assertion references; brief pins; and live genealogical evidence. Distinct
fields and array occurrences remain distinct even when they reference the same
target. Prose, untyped strings, arbitrary metadata and historical citation
records do not create inferred dependencies.

Private segments contribute ordered `source_links` as `manuscript-source`
edges and an optional `pov_character_id` as a `manuscript-pov` edge. Works
contribute each explicit chapter segment occurrence as a `work-segment` edge.
Duplicate source links and expressly permitted segment reuse retain their
separate source positions. An optional segment `placement` must be on the
selected timeline at or before the horizon; an excluded segment cannot bridge
traversal into its works. Private source bodies are opaque and never scanned
for IDs. Private source fields remain exact once their document is admitted;
an unavailable canonical target supplies no edge.

Traversal is iterative breadth first. Reaching an owner record also makes all
of that owner's admitted section, transition, and live-evidence keys available
for reverse traversal. Thus a whole-record target includes exact incoming
member references. Edges and visited keys are cycle-safe; each occurrence is
reported once per original with its shortest discovered distance. Base and
candidate traversal remain independent and deterministic.

## Report and provenance

The report contains `target`, `at`, `depth`, `maxItems`, `base`, `candidate`,
`items`, explicit `scope`, and informational flags. `mentions` and
`semanticSuggestions` are empty. Each item contains:

```text
snapshot, owner, target, relation, distance,
citation: entityId, sourcePath, section, sourceOrdinal, memberId, time, provenance
```

`section` is the exact native source field path, including original array
indices. `sourceOrdinal` retains the physical authored member position;
`memberId` retains the stable owner-local member identity where one exists.
Nested typed arrays preserve the containing effect/history member's ordinal
and ID. Citation time uses exact decimal-string coordinates.

A base citation has source provenance `{kind, revision, blobOid}`. A current
working original may have null revision/blob identity. Canonical candidate
provenance is `{kind: "candidate", baseRevision, planSha256}`. Unchanged private
source citations retain their original source provenance in both base and
candidate rows. The candidate is never assigned an invented Git commit. The
enclosing `candidate` contains the same literal base revision and native
consequence-plan digest.

Private occurrence fields use `source_links[i]`, `pov_character_id`, and
`chapters[c].segments[i]`. Work citations retain the chapter-local segment
ordinal and exact chapter ID. Placement governs admission; private citation
`time` remains null because the static source link does not assert a story
event or learning time.

## Complete bounds and failures

| Bound | Limit |
| --- | --- |
| Impact request | 65,536 encoded bytes |
| Preview wrapper | 524,288 encoded bytes; existing plan bounds also apply |
| Requested depth | 1–32 |
| Requested report items | 1–1,000, **across both originals together** |
| Complete impact report | 262,144 UTF-8 bytes |
| Admitted graph record/member nodes | 4,096 per original |
| Complete typed graph occurrences | 16,384 per original |
| Graph indexing/occurrence work | 1,000,000 units per original |
| Original source preflight | 4,096 records, 32 MiB, 100,000 nodes, 64 levels |
| Private manuscript source | 1,024 documents, 16 MiB total, 2 MiB per document; manuscript structural bounds also apply |
| Additional validation estimate | 8,000,000 work units per original |
| Source path expansion | 4,096 bytes per field path, 8 MiB aggregate |

The shared source projector's complete work bounds also apply. Candidate
construction retains the existing consequence source/diff and character limits.
These are complete-operation limits, not pagination controls. If a further
unseen occurrence would lie beyond `depth`, the entire request fails. Item,
byte, node, work, malformed-source, and ambiguous-position failures likewise
return no partial graph. A target absent or withheld in both originals is
unavailable. Errors use the native error envelope (`usage_error`, `not_found`,
`validation_error`, `conflict`, or `unsupported`, as applicable).

## Saved-review witness

The shared internal `capture` and `witness` operations support private saved
retcon reviews. Their versioned admitted-input value is:

```text
version: "wedl-retcon-witness/v1"
worldId, target, at, depth, maxItems, seeds
inputs: [{ref, path, present, sections: [{field, ordinal, valueRaw}]}]
edges: [{owner, target, relation, field, ordinal, memberId, time}]
consulted: [{path, field, valueRaw}]
```

Seeds retain the explicit target, operation-touched IDs, admitted owner members,
and the complete original base/candidate dependency closures. Originally absent
created targets remain explicit absent inputs with their candidate path hints.
Recomputation rebuilds the admitted reverse graph and follows **new incoming
references** from those retained seeds; it does not merely reread the original
input list. Removed or renamed members remain explicit absent original inputs.
Source path moves are observable changes.

Witness sections retain admitted frontmatter in authored top-level field order
and one exact opaque Markdown body. Arrays preserve admitted authored order.
Excluded members are removed before position numbering, so inserting a future
member before an admitted occurrence changes the immutable report citation but
does not change the witness. Reordering admitted members does change it.

Every `valueRaw` is a canonical JSON **string**, with ordered object keys and
native scalar distinctions. An integer greater than JavaScript's exact integer
range survives browser round trips; `1` and `1.0` remain distinct. A Markdown
body is encoded as a JSON string inside `valueRaw`, preserving exact characters
and line endings. The witness has no raw numeric source payload leaves.

Whole world declaration collections are excluded from ordinary source cells.
`consulted` includes schema/capabilities/default timeline, the selected timeline,
and exact state keys, profile bindings/profiles, predicates and relationship
metrics consulted by selected admitted inputs. Explicit world chronology
section targets retain their exact declaration values. Unrelated declarations
do not enter the witness merely because they share a world source file.

The complete witness is limited to 1,048,576 UTF-8 bytes and 4,096 combined
inputs, edges, consulted declarations, and source sections. It contains no
revision, tree, blob, whole-source fingerprint, cache identity, or private-review
hash. Saved-review reconstruction, hashes, first-parent history, decisions and
storage belong to the separate retcon-review service. Impact preview itself
does not create or mutate any private review.
