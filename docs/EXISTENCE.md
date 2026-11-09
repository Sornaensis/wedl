# Authored existence episodes

An existence episode records one authored statement about an existing character.
Its kind is `embodied`, `disembodied`, `constructed`, `other` or `unknown`.
Separate IDs remain separate statements, including overlapping accounts of the
same kind and an overlapping `unknown` account. The reader selects no winner and
derives no age, resurrection, identity continuity or vital state from episodes.
Existing birth, death, indefinite existence and disclosure answers retain their
own generational-history sources.

The world must explicitly declare both `existence-v1` and
`generational-core-v1`. Lore profiles and `lore-core-v1` are optional. The source
schema remains `wedl/v1` and the Rust result protocol remains `wedl-rust/v1`.

## Source contract

An episode has ordinary record identity and common metadata:

```yaml
schema: wedl/v1
kind: existence-episode
id: existence_00000000000000000000000001
title: An authored embodied interval
domain: harbor.existence
status: canonical
character_id: char_00000000000000000000000001
episode_kind: embodied
valid:
  kind: interval
  timeline: main
  start: {timeline: main, tick: 0, order: 0}
start_evidence:
- revision: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  path: story/characters/earlier-account.md
  blob_oid: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
  target: {kind: record, id: char_00000000000000000000000001}
audience: [author]
perspectives: [author]
```

The repeated hexadecimal values illustrate syntactically valid historical
pointers; they do not establish that those objects exist. `character_id` must
name an actual current character by its exact ID. A historical citation target
has separate rules and need not exist in current source.

| Field | Rule |
| --- | --- |
| `id` | `existence_` followed by 26 Crockford characters |
| `title` | Nonblank text, at most 256 characters |
| `domain` | Dotted identifier, at most 256 characters |
| `status` | Canonical by default; `canonical`, `draft` or `retired` |
| `character_id` | Exact existing character ID; titles and aliases are not references |
| `episode_kind` | Exactly `embodied`, `disembodied`, `constructed`, `other` or `unknown` |
| `valid` | Required interval mapping on one declared timeline |
| `start_evidence` | Required array of closed historical citations |
| `end_evidence` | Conditional historical citation array, governed by the interval rules below |
| `audience`, `perspectives` | Required nonempty, sorted, unique arrays of scope labels |
| `provenance` | Optional authored string, including an empty string |

Reads first validate the complete source snapshot, including agreement between
native parsed fields, exact source bytes and the advertised fingerprint. Shared
record contracts and optional chronology/descriptions are checked together with
the episode grammar. All retained statuses validate, including draft, retired,
historical and future episodes. Every episode body must fit within 1 MiB of
UTF-8 bytes. Its prose remains opaque and is absent from the episode list
response. Ordinary metadata,
explicit chronology, description sections, concept links and extension metadata
retain their existing contracts. No arbitrary metadata or body text becomes
an existence bound, citation, replacement edge or character fact.

### Interval and evidence dependencies

`valid` contains exactly `kind: interval`, its declared `timeline`, and optional
`start` and `end` endpoints. Each supplied endpoint contains exactly `timeline`,
native signed i64 integer `tick` and native signed i32 integer `order`. Its
timeline must match the interval. Bounds are inclusive and start must not follow
end. Omitting a bound makes that side unbounded; an explicit null is invalid.
Timeless validity, numeric strings, booleans and floating-point coordinates are
invalid source for these fields.

Every affirmative kind requires nonempty `start_evidence`, even when its start
bound is omitted. A bounded end requires nonempty `end_evidence`. An unbounded
end forbids the `end_evidence` field, including an empty array or null.

An `unknown` episode instead requires both bounds omitted,
`start_evidence: []`, and no `end_evidence`. It records authored uncertainty and
asserts no existence. Its explicit timeline still governs a dated read. The
same character may have an applicable affirmative statement and an applicable
unknown statement; both remain visible.

### Historical citations

Each evidence item contains exactly `revision`, `path`, `blob_oid` and `target`.
Revision and blob IDs are 40 or 64 lowercase hexadecimal characters. Paths are
safe repository-relative paths without empty, `.` or `..` components,
backslashes, colons or NUL. Targets use one of these closed shapes:

- `{kind: record, id: NONBLANK_ID}`
- `{kind: section, owner_id: NONBLANK_ID, id: NONBLANK_MEMBER}`
- `{kind: transition, owner_id: NONBLANK_ID, id: NONBLANK_MEMBER}`

Each evidence array uses the existing finite value guard and requires unique
items. The same historical pointer may occur once in each endpoint's evidence
array. IDs and member names are historical strings, independently of the
current record namespace. Evidence does not enter the live reference graph.

Validation checks syntax only. It does not authenticate historical Git objects,
open old paths, resolve old targets against today's world, or establish
disclosure rights for historical contents. The result preserves the authored
pointers as `startEvidence` and optional `endEvidence`, explicitly labels them
`evidenceStatus: unverified-historical`, and separately cites the actual current
episode source. No historical body or target metadata is fetched.

## Author reads

The native API, service, CLI, closed GET API and local browser use one list
operation:

```sh
wedl-rs --repo my-world existence list char_00000000000000000000000001 --tick 5 --order 0 --audience author --view author --limit 100
wedl-rs --repo my-world existence list char_00000000000000000000000001
```

The GET route is `/api/existence/list`, with required `character` and optional
`tick`, `order`, `timeline`, `audience`, `view` and `limit`. Unknown parameters are
refused. The native entry point is
`existence::list(world, character_id, at, audience, view, limit)`.

A target is an exact character ID. A missing, wrong-kind or unavailable target,
or an available character with no matching episodes, gives the same empty
result shape. The response echoes the caller's target string without adding a
character title, aliases or existence conclusion. An empty list means that no
admitted authored statement was supplied.

Audience and view default to `author`. A supplied tick selects an exact horizon;
order defaults to `2147483647` and timeline to the world default. Order or
timeline without a tick is invalid. With no horizon, the list returns admitted
history across **all authored timelines**, matching the current Rust lore and
description convention.

The character endpoint and each episode have independent scope checks. An
ordinary character's optional root `audience` and `perspectives` arrays must
contain the selected labels when supplied. Omitted arrays impose no additional
constraint; an empty array admits nothing. Existing ordinary endpoint labels
retain membership behavior without a new ordering requirement. Malformed scope
data fails closed. Ordinary character status does not impose an extra
availability or time restriction.

Each listed episode must be canonical, match its own ordered audience and
perspective labels, and apply at the selected horizon. There is no replacement
or overlap suppression. Items sort by episode ID. All endpoint, scope and time
filters apply before counting and enforcing the complete-result limit of
1–1,000. Overflow refuses the whole result; the API does not truncate or return
a cursor.

The same episode admission rule applies when another author reader uses an
episode as a description owner or typed reference endpoint. A future, draft,
retired or out-of-scope episode, or one whose character is unavailable, cannot
supply description sections or eligible lore, assertion or governance references
through a separate route. A description section retains its own additional
scope and applicability checks.

### Result and citations

The envelope contains `protocol`, `perspective: author`, `scope`, `basis`, `at`,
`audience`, `view`, caller `characterId`, `count`, `items` and `limit`. Scope is
`author-as-of-existence` or `author-all-time-existence`; basis is
`authored-existence-episodes`.

Each item contains `episodeId`, title, `characterId`, `episodeKind`, `valid`,
`startEvidence`, optional `endEvidence`, optional `provenance`, `evidenceStatus`
and an actual source `citation`. Native source tick and order values use the
established exact decimal-string wire representation. Current citations retain
the episode ID, actual source path, blob OID when available, revision, source
fingerprint and section `record`. They are distinct from the unverified
historical pointers preserved in the evidence arrays.

## Editing and remaining boundaries

The ordinary reviewed `record create` and `record update` requests support
existence episodes. The caller supplies the exact `existence_` ID; the writer
supplies the source schema and standard existence path for new records. Supplied
fields replace their authored values; omitted fields retain their current
values. Preview validates the
complete candidate, including evidence/bound dependencies and noncanonical
episodes, before the existing hash-bound save. Unchanged body bytes and common
metadata remain intact.

The current partial update request does not remove optional root fields. To
change a previously bounded end into an unbounded end, direct source editing
must omit both the interval's `end` and the root `end_evidence` field. Setting
`end_evidence` to `[]` or null is not removal. This slice introduces no separate
writer, migration, source-version adapter or cache protocol. Browser reads
remain immutable snapshot operations; source publication uses the existing CLI
authoring path.

## Character unknown contract

`character existence VIEWER TARGET --tick T` and GET
`/api/character/existence?character=VIEWER&target=TARGET&tick=T` provide the
separate character read. The native entry point is
`existence::character(world, viewer, target, at, limit)`, also exported through
`narrative::existence`. It requires the existence and generational capabilities,
a valid captured source, an exact viewer and an exact horizon.

The pinned source has no typed learned existence-episode payload. Every valid
read therefore returns `items: []`, `count: 0`, basis
`explicitly-learned-existence` and reason `typed-episode-knowledge-unavailable`.
The target is echoed without looking up its existence or supplying labels.
Present, unavailable and nonexistent targets share that same unknown shape.
Limits remain 1–1000; author audience, view and history parameters are rejected.

Existing learned words continue through generic character context and search.
A learned pointer to an episode does not import its current title, kind,
interval, evidence, body or description sections. Canonical edits cannot change
those learned answers or their budgets. The story clock and generational vital
history remain separate. This explicit unknown result implements the pinned
character boundary; it does not promise an invented typed episode grammar.

## Pinned evidence and verification

This contract was checked against commit
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`:

- [Existence source grammar](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/existence.py)
  defines independent capability requirements, interval/evidence dependencies
  and separate overlapping statements.
- [Existence runtime](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/existence_runtime.py)
  gates episode and character scope, preserves historical pointers as payload,
  cites actual episode source, and keeps learned pointers separate.
- [Existence authoring](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/existence_authoring.py),
  [guide](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/docs/guides/existence.md)
  and source/runtime/authoring tests cover guarded replacement and preservation
  of independent generational and learned evidence.

Rust retains its common metadata conventions, current source snapshots,
complete-or-error lists and all-timeline history. `tests/existence.rs` exercises
the source grammar, historical syntax and preservation, endpoint scopes, exact
time and limits, overlapping statements, and unchanged character/vital
behavior. Shared writer and interface tests cover publication and transport.
