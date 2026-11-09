# Dated description sections

Description sections are explicitly authored Markdown members on an existing
record. They require the world capability `descriptions-v1`, independently of
lore. They do not extract body headings, infer dates or teach characters facts.
The source schema and API remain `wedl/v1` and `wedl-rust/v1`.

## Source contract

Every known canonical record kind, including world, assertion and context-brief, can carry an
optional root `descriptions` array. Hypotheses cannot own description sections.
Omission and an explicit empty array remain distinct in source; both produce an
empty admitted list. Null is invalid.

Each ordered member has exactly these fields:

| Field | Source rule |
| --- | --- |
| `id` | `description_` followed by 26 Crockford characters; unique within the owner |
| `heading` | Nonblank UTF-8 text, at most 256 characters |
| `markdown` | Exact authored UTF-8 text, at most 1 MiB; empty text is allowed |
| `valid` | Existing closed timeless or inclusive interval grammar |
| `audience` | Sorted unique nonempty array of literal presentation labels |
| `perspectives` | Sorted unique nonempty array of literal presentation labels |
| `replaces` | Optional exact ID of an earlier member on the same owner |

Member labels use the current lore convention: nonblank single-line strings of
at most 256 characters. They are opaque selectors, not entity references or
permission grants. ID-looking headings, Markdown and labels remain literal.
Local member IDs can repeat on different owners without sharing identity or
replacement history.

```yaml
descriptions:
- id: description_00000000000000000000000001
  heading: The harbor before reform
  markdown: |
    The old account describes three lamps beside the quay.
  valid:
    kind: interval
    timeline: main
    start: {timeline: main, tick: 0, order: 0}
    end: {timeline: main, tick: 9, order: 0}
  audience: [author]
  perspectives: [author]
```

`valid: {kind: timeless}` applies at every horizon. An interval requires its
declared timeline and may omit either endpoint. Supplied endpoints contain
exactly timeline, signed i64 integer tick and signed i32 integer order. The
endpoint timeline must match the interval; inclusive start must not follow end.
Source numeric strings, booleans, floats and null endpoints are rejected. Query
JSON preserves tick and order as exact decimal strings.

The description aggregate uses the existing finite value guard: depth at most
16, at most 16,384 nodes including keys, at most 256 keys per mapping and 1,024
items per array. These combined limits may admit fewer than 1,024 sections. They
apply to the authored section aggregate, not to the owner's unrelated metadata
or opaque body bytes.

### Explicit corrections

A replacement must name an earlier member in the same source array. Its `valid`,
`audience` and `perspectives` must exactly equal that earlier member's values.
Forward, self and cross-owner references fail. Heading and Markdown may change.
The same rule applies to a replacement of an earlier correction.

At an exact horizon, scope and applicability are applied first, then eligible
replacement edges suppress their eligible predecessors. Unrelated overlapping
accounts remain visible. Two corrections of the same predecessor both remain;
source order does not select a winner. Reading history with no horizon retains
all admitted members, including replaced ones.

## Author reads

The CLI, closed GET API and local browser share two operations:

```sh
wedl-rs --repo my-world descriptions list OWNER_ID --tick 5 --order 0 --audience author --view author --limit 100
wedl-rs --repo my-world descriptions section OWNER_ID SECTION_ID --tick 5 --order 0 --audience author --view author
```

Use an exact owner ID and the stable section ID returned by the list. Titles and
headings do not resolve as IDs. The GET routes are `/api/descriptions/list` with
`owner` and optional `limit`, and `/api/descriptions/section` with `owner` and
`section`. Both accept optional `tick`, `order`, `timeline`, `audience` and `view`.
Unknown parameters are refused.

Audience and view default to `author`. A tick selects an exact story horizon;
order defaults to `2147483647` and timeline to the world default. Order or
timeline without a tick is invalid. With no horizon, the reader returns admitted
history across all authored timelines, matching the Rust lore convention.

List limits are 1–1,000. Admission and replacement suppression precede counting
and limits; overflow refuses the whole result. An available owner with no
matching sections returns an empty list. A missing or unavailable owner or
requested section returns the same unavailable error. A replaced section is
unavailable in the dated view and can still be inspected in admitted history.

### Owner and section scope

An ordinary owner is available unless its optional root `audience` or
`perspectives` array excludes the selected label. Omitted owner labels add no
constraint; an empty owner array admits nothing. Ordinary owner arrays retain
set-membership behavior without imposing new ordering rules on unrelated source
metadata. Non-array or non-string owner scope data fails closed when used.

Lore-concept and context-brief owners additionally require canonical status and
their own applicable validity. A brief's section cannot bypass a future,
draft or retired owner. Existence-episode owners require
canonical status, applicable episode validity, matching own labels and an
independently admitted character endpoint. Their dated sections cannot bypass
those episode bounds, even when a section itself is timeless. Ordinary owners retain author
availability across statuses and event times. A section has its own explicit
validity and does not inherit an event's time or an owner's body applicability.
Hypotheses are unavailable.

After owner admission, member audience, perspective and validity filter the
section list. A section does not acquire visibility merely because its owner is
available. The author presentation labels do not implement character evidence
or authorization.

## Output and citations

Both reads return the author envelope: protocol, perspective, scope, exact `at`,
audience, view and `basis: authored-description-sections`, plus `ownerId`,
`ownerKind` and title. The scope is `author-as-of-descriptions` or
`author-all-time-descriptions`.

The list adds `items` and `count`. A section lookup instead adds one section's
fields directly to the envelope. Each section contains `sectionId`, heading,
exact Markdown, validity, optional `replaces`, citation and target
`{kind: section, ownerId, sectionId}`.

Citations retain the owner ID, source path, blob OID, revision, source fingerprint
and original `descriptions[N]` index. Filtering and suppression never renumber
that source position. The stable target combines owner and local section ID;
a repeated local ID on another owner cannot resolve to the same section.

The original body is not included or relabeled as explicitly timeless. Its
existing bytes and existing author/character rules remain intact.

## Reviewed editing and remaining boundaries

The ordinary reviewed `record create` and `record update` requests accept
`descriptions` wherever the existing writer supports that owner kind. World
updates accept the same field. A supplied array replaces the complete array;
omission preserves it and `[]` explicitly clears it. Null does not delete the
field. The preview validates the complete candidate, including hidden, historical
and noncanonical sections, before the normal fingerprint/hash-bound save.

Profile declarations, unrelated metadata and opaque body prose are preserved
unless explicitly changed by that ordinary request. No separate description
writer, migration, source-version adapter or retry framework is introduced.
Current local-author sessions support [reviewed browser publication](BROWSER_PUBLICATION.md)
through the existing writer and capture fresh source for each canonical request.
Historical sessions remain fixed and GET-only.

## Character section reads

`character descriptions VIEWER OWNER --tick T` and
`character section VIEWER OWNER SECTION --tick T` use the separate shared
[character admission contract](NARRATIVE_ADMISSION.md). The corresponding GET
routes are `/api/character/descriptions` and `/api/character/section`, with exact
`character`, `owner`, optional `section`, and required `tick` selectors.
Optional order and timeline retain the common exact-time rules; lists accept
limits from 1 to 1000 and refuse incomplete results.

The owner must be independently known through the viewer's own held, scoped
knowledge or be the viewer. Owner scope and applicable concept/brief validity are
checked before member scope, member validity and correction suppression.
Canonical assertion, existence-episode and hypothesis payloads cannot enter
through description ownership. An ordinary owner's event time or status supplies
no additional section interval; each section keeps its explicitly authored bounds.

Character audience is `public` or the exact viewer; perspective is `ordinary`
or the exact viewer. Callers cannot supply alternative author labels. Only
admitted replacements suppress earlier sections. Returned sections retain
owner/section identity, heading, exact Markdown, validity and a safe target,
but no owner title, source index or author citation. A replacement pointer is
included only if its target is also admitted. Missing and inaccessible exact
owners/sections use the same generic unavailable error.

Admitted sections also enter generic character context and search. Untimed body
prose remains excluded, and private or future sections cannot affect counts,
scores, snippets or packet bounds. The social and environmental author readers
retain their existing separate compositions. Exact [writing frames](WRITING_FRAMES.md)
can admit a selected scene's roots and [briefs](BRIEFS.md) can require exact
sections. Brief-owned sections require their owner's canonical status and
applicability in both author and character reads; a section cannot reopen a
future or noncanonical brief. Species, social and environmental character
compositions attach these native sections only for their admitted evidence
owners. Species/magic author compositions attach native sections when the
capability is enabled, retaining their author citations and correction rules.
Manuscript/reader workflows remain in the plan.

## Public retrieval training

Explicit [semantic/hybrid search](RETRIEVAL.md) uses a separate fixed public
prior. Eligible training text comes only from public, ordinary, timeless
sections on canonical/active/closed world, location or object owners without
`valid` or `time` qualification. Owner labels still apply, and eligible
replacements suppress their predecessors. Only section heading and Markdown
train; ordinary bodies and owner metadata remain excluded.

This training does not admit the owner, produce a search hit or teach any fact.
Returned character descriptions still require their existing known-owner,
member and exact-horizon admission. Private and interval sections never train
the prior, even if independently available as character recall targets. This
native-section prior is a deliberate Rust source choice, not an adapter for
the pinned implementation's inherited body-section grammar.

## Pinned evidence

The contract was checked against commit
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`:

- [Description source grammar](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/descriptions.py)
  defines local identity, exact Markdown, independent capability and earlier
  same-applicability corrections.
- [Description runtime](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/description_runtime.py)
  filters owner/member scope before replacement suppression and retains history.
- [Description guide](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/docs/guides/descriptions.md),
  `tests/test_descriptions.py`, `tests/test_description_runtime.py`,
  `tests/test_description_authoring.py` and entitlement/transport tests cover
  source preservation, three intervals, overlapping accounts, stable references,
  reviewed publication and shared admission/retrieval work.

Rust uses current source snapshots, the established exact integer codec,
complete-or-error lists and current all-timeline history instead of upstream
request cursors or profile-version machinery. `tests/description.rs` covers this
local source and author-read contract; writer and interface tests exercise the
shared publication and transport paths.
