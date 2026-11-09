# Native event and grouped consequences v1

These are bounded, informational author reads. `event-consequences` examines one
exact event in a captured source world. `grouped-consequences` constructs an
explicit native consequence candidate and attributes changed source
contributions. Neither command approves, applies, publishes, changes private
records, updates a cache, or infers a causal narrative. Source remains `wedl/v1`;
responses use `wedl-rust/v1`.

The implementation follows the event-local and grouped-source contracts in
upstream `event_consequences.py`, `consequence_delta.py`, and
`consequence_report.py` at the pinned commit
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`, using the existing Rust source grammar
and runtime folds. [The executable example](../examples/consequences/README.md)
exercises both reads with a tick greater than JavaScript's exact integer range.

## Closed requests and transports

```json
{
  "expectedFingerprint": "<exact captured lowercase SHA-256>",
  "eventId": "event_00000000000000000000000001",
  "at": {"timeline": "main", "tick": "9007199254740994", "order": "0"},
  "maxItems": 1000,
  "maxCharacters": 1000000
}
```

Use `wedl-rs --repo WORLD event-consequences --file request.json`, or
`GET /api/event-consequences?request=JSON`. The library entry points are
`event_consequences::decode_event` and `event_consequences::read`.

```json
{
  "plan": {
    "expectedFingerprint": "<exact captured lowercase SHA-256>",
    "operations": [{
      "type": "record.update",
      "entityId": "event_00000000000000000000000001",
      "patch": "kind: event\nbody: Literal revised event prose.\n"
    }]
  },
  "at": {"timeline": "main", "tick": "9007199254740994", "order": "0"},
  "focusEventIds": [],
  "maxItems": 1000,
  "maxCharacters": 1000000
}
```

Use `grouped-consequences --file request.json`, or
`GET /api/grouped-consequences?request=JSON`. The library entry points are
`decode_grouped` and `preview` in the same module. The `plan` is the existing
closed consequence plan, including its exact retained YAML strings and optional
explicit semantic `analysis`. An analysis remains visible as `semanticAnalysis`
at its own declared horizon; the grouped report does not change its policy or
convert its result into approval.

All shown fields are required, including `focusEventIds`, which may be empty.
Unknown and duplicate fields are refused, including nested duplicate plan keys.
References use exact record IDs, never titles, aliases, slugs, or prefix matching.
Explicit focus IDs must identify admitted canonical events in the base or
candidate. Automatic groups can additionally include events named by changed
explicit source contributions.

Time coordinates are canonical decimal strings: signed 64-bit `tick` and signed
32-bit `order`. Numeric JSON tokens, leading zeroes and `-0` are refused. The
timeline must be declared in each world being read. Both transports preserve the
normal native envelope and captured-source fingerprint. Historical service reads
use the explicit immutable revision; current-source reads retain their ordinary
currentness fence. The pure library functions operate only on their supplied
world snapshots.

## Event-local comparison

Let `T` be the focus event's complete `(timeline, tick, order)` and `H` the
requested horizon. The event must be canonical and have occurred by `H`.
`changes` compares the native folds at **T against T**:

1. Keep the event and every source owner in both views. An event-presence
   condition remains true in the exclusion view.
2. In one private evaluation view, suppress only the focus event's `effects` and
   knowledge, relationship, or story-point lifecycle transitions whose exact
   `causing_event` is the focus event **and** whose full time equals `T`.
3. Keep every unrelated same-time write. Keep delayed focus-caused transitions
   as original source history; they are outside the local comparison at `T`.
4. Reuse native typed state, knowledge, relationship, story-point and outcome
   semantics. Compare values/presence; a citation-only state change or equal
   overwrite is not a value difference.

There is no `T-1` calculation, so signed clock extrema behave normally. The
synthetic exclusion view is not validated as new authored source, assigned a new
fingerprint, or serialized as a candidate. Original source paths, blob IDs,
revision, fingerprint, member identities and original array ordinals remain the
citation authority. The original complete world must pass native validation
before any counterfactual fold. This preserves native conflict rules, including
strictly ordered histories and refusal of two canonical events writing the same
state cell at one identical coordinate.

The report separates these evidence families:

| Field | Meaning |
| --- | --- |
| `effects` | Every authored focus effect in source order, including value no-ops; `effectRaw` preserves exact typed JSON. |
| `changes` | Native semantic differences from exact-time contribution exclusion. This may include changed eligibility of dependent story-points. |
| `causedTransitions` | Explicitly focus-caused native history through `H`, with original `sourceOrdinal`, exact `time`, and `atEventTime`. Delayed history stays distinguishable. |
| `outcomes` | Explicit scene/story-point outcome links, with the actual link halves and their citations. |
| `causalSuccessors` | Direct explicitly linked canonical successors after `T` and through `H`; no transitive closure. |
| `currentAtHorizon` | Current values at `H` for the explicitly affected state cells, history owners and outcome pairs, with supersession evidence. |
| `advisories` | A missing reciprocal story-point outcome half is `CONSEQUENCE-LINK-001`. A scene's ordinary one-sided link is not an advisory. |

Supersession is `unchanged` when the current writer or latest history member is
still explicitly focus-owned. A later different writer/member is `superseded`
with its citation, even if it writes an equal value. A delayed latest member
caused by the focus remains `unchanged`. Insufficient applicable provenance is
`unknown`. These labels describe the current authored contribution, not proof
that an event caused a later event or that an authored belief is true.

Cleared values remain known absence with a real writer citation. An effective
first knowledge/relationship transition compares against known absent history.
Unavailable owners remain distinct from absence. Relationship snapshots retain
native direction and latest-snapshot replacement behavior; history fields are
not merged into a later snapshot.

## Grouped source attribution

The shared consequence constructor resolves operation targets and constructs
one complete candidate. Invalid original or candidate source is refused before
semantic comparison. Both originals are folded at the same exact `H`.

Only exact resolved operation targets contribute source-difference rows,
including no-op operations. Literal source sections are compared after the
existing native author-as-of source admission, member filtering, typed reference
pruning, and complete context-brief closure. Each changed section has before and
after presence, a browser-safe `value`, and an exact typed JSON `valueRaw`.
`bodyMarkdown` retains literal authored prose. An extant owner outside `H` is
excluded, not reported as a deletion. If an extant owner is excluded in either
original, its source row is omitted; an independently available semantic
difference can still appear. Truly empty untimed native histories can
still have source edits; future-only histories remain excluded.

Section attribution follows these rules:

| Changed source section | Group owner |
| --- | --- |
| Event `effects` or `frontmatter.time` | That exact event. |
| Event `related_story_points` | That exact event, when representable by the existing plan codec. |
| Native `transitions` / `lifecycle.transitions` | One event only when every changed member is explicitly caused and all changed members name that same event. |
| `outcome_events` | One event only when the changed membership identifies exactly one event. |
| Mixed-event, partly uncaused history; title, body, metadata, initial state, or other source | Unattributed. |

Stable owner-local member IDs align insertions without treating a shifted
ordinal as a changed contribution. A real reorder remains a source change.
Source partitions cite only their represented sections; a grouped transition
does not acquire title/body evidence from another partition.

Semantic attribution is a separate pass over changed explicit contributions.
Changed event effects/time can group a state-value difference; changed caused
history can group its native history subject; outcome flags identify their
explicit event. An unchanged latest effect cannot supply attribution for an
initial-state edit. An indirect eligibility change does not acquire an event
owner merely because its final value changed at the same time. Multiple changed
explicit contributors may place one semantic difference in more than one
group; `uniqueCausation` is always false.

`eventGroups` contains each event's semantic `changes` and source
`recordChanges`. `unattributedChanges` and `unattributedRecordChanges` retain all
remaining admitted results. A source-only change can produce a group with no
semantic differences, and an explicit focus event may have an empty group.
Ordering is deterministic by event time/ID and native subject/source order.

## Complete bounds and explicit limits

| Bound | Limit |
| --- | --- |
| Encoded request | 524,288 bytes; the embedded plan retains its own 262,144-byte limit and 64-operation limit. |
| Requested report items | `maxItems` 1–1,000. |
| Requested complete JSON characters | `maxCharacters` 256–1,000,000, including hashes and metadata. |
| Complete encoded report | 8 MiB; duplicated grouped partitions are also charged before copying. |
| Event groups / explicit focus IDs | 100 each. |
| Contribution sequence alignment | 2,000,000 aggregate matrix/work cells, checked before allocation. |
| Native semantic preflight | 4,096 records, 32 MiB source, 100,000 nodes, depth 64, 2,000,000 estimated fold work, 1,024 subjects. |
| Admitted source paths/work | Existing revision-source limits, including 4,096 bytes per path, 8 MiB path expansion and 8,000,000 work units. |

An event report counts effects, local changes, caused histories, outcomes,
successors, current snapshots and advisories. A grouped report counts groups,
unique semantic differences, unique touched-source rows and changed sections;
explicit plan-analysis expectation rows and its separate delta rows are also
charged. Exceeding any limit fails the whole read, with no truncated successful
report. `requestSha256` and `reportSha256` bind the complete request/result and
budgets. `complete:true` means the selected native report was not truncated;
`validationComplete:false` preserves the project's broader conformance limit.

Current native boundaries remain visible:

- State folds use the complete original local-author world with
  [scalar entity-effect pruning](SEMANTIC_CONSEQUENCES.md#temporal-entity-operands).
  `futureEntityTargetPruning:true` applies only to unbound scalar entity effects,
  as identified by `stateReferences`. Skipped effects leave earlier state
  effective and preserve source ordinals. Initial values, sets, bound profiles
  and expected literals keep their existing behavior. The separate authored
  effect list retains exact source and does not imply an effective semantic
  contribution or establish character admission.
- Typed relationship, story-point and event edits use the existing consequence
  Plan codec. Grouped semantic comparisons inspect their authored histories and
  indirect differences. These reads add no new write grammar or inferred effects.
- Candidate preparation inherits the existing complete source-preview ceiling
  of 1,000,000 characters before the final grouped-report budget is applied.
- Context briefs use one complete native author-union closure. Any required
  withheld owner or member closes that brief; no partial closure is copied.
- This is not a character perspective, learned-claim truth adjudication,
  proposed merge, retcon approval, publication authority, or generalized causal
  explanation. `writesPerformed` and `canApply` are always false.
