# Exact writing frames

A writing frame selects one captured canonical source snapshot, an optional
independently pinned private work, optional exact scene and POV, explicit story
coordinates and reading lens. It is an operational request,
not a source record or shared cursor. Resolving it never writes Markdown, moves
world or scene time, fills a missing coordinate or changes another selection.
The source and API remain `wedl/v1` and `wedl-rust/v1`.

## Closed request

The frame has exactly six required JSON fields:

```json
{
  "fingerprint": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "work": null,
  "sceneId": "scene_00000000000000000000000002",
  "povId": "char_00000000000000000000000001",
  "at": {"timeline": "main", "tick": "12", "order": "0"},
  "lens": "character"
}
```

The repeated hash illustrates the shape only. Copy the actual fingerprint from
`status` for the source snapshot being read. The browser can capture its served
snapshot explicitly; a saved frame does not silently capture a newer source.

| Field | Rule |
| --- | --- |
| `fingerprint` | Exact captured canonical source hash: 64 lowercase hexadecimal characters |
| `work` | Null or exactly `{workId, authorFingerprint}`; an exact private work ID and its complete private source hash |
| `sceneId` | Exact existing scene ID or null |
| `povId` | Exact existing character ID or null; required for character and dramatic-irony lenses |
| `at` | Exactly `timeline`, `tick` and `order`; no defaults |
| `lens` | `author`, `character` or `dramatic-irony` |

Tick and order are canonical signed decimal **strings**, within i64 and i32
bounds respectively. Numeric JSON coordinates, `+1`, `01`, `-0`, overflow and
omitted coordinates are refused. The timeline must be explicitly named and
declared. Input is at most 16 KiB; duplicate keys at any depth, unknown fields,
non-object shapes and malformed optional IDs fail before source resolution.
Selections and timeline names are bounded to 128 UTF-8 bytes.

A selected work uses this shape, with its actual private fingerprint copied from
author Writing choices:

```json
{
  "workId": "work_00000000000000000000000001",
  "authorFingerprint": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
}
```

The work ID uses the private manuscript source grammar; its fingerprint is 64
lowercase hexadecimal characters. Both nested fields are required. Selecting a
work validates its complete segment closure against the same captured canonical
revision. It supplies no manuscript prose, work title, scene, POV, time, active
cursor or nearby event to context. Segment placements, source links and repeated
occurrences never derive frame coordinates or grant character learning or reader
disclosure. A work can be selected without selecting an occurrence. There is no
occurrence field. Frame selection remains independent of explicit authoring
request times.

## Resolve and discovery

The native entry points are `writing_frames::decode`, `resolve`, `choices`,
`context`, `decode_families`, `context_with_families` and
`with_author_snapshot`. `Frame`, `WorkSelection`, `Lens` and `FamilySelection`
are public typed inputs. Native callers retain the
same shape and bound checks even when constructing a frame directly.

`resolve(world, authors, frame)` validates the complete captured canonical source
and exact fingerprint, then returns the unchanged frame. `authors` is an explicit
`Option<&manuscript::Snapshot>`: `None` supports unbound frames and refuses a
selected work. A supplied snapshot must match the world's revision and its own
exact raw source, parsed projections and fingerprint before any bound work is
used. The complete selected work's segment references, placement timelines,
source links and POV pointers must resolve, including occurrences later than the
frame horizon. Unreferenced segment links do not become dependencies. Validation
does not serialize private prose or impose reader/report occurrence or output
bounds on valid author source. Stale pins, missing/wrong-kind
selections, undeclared timelines and unavailable character scenes share one
generic unavailable error. It never returns a replacement fingerprint or changes
the request. Invalid captured bytes, parsed fields or fingerprint consistency
produce generic source-validation failure without author diagnostics.

`choices(world, authors, frame, limit)` returns `protocol`, the input `frame`,
`works`, `authorFingerprint`, `scenes`, `povs`, `moments` and `limit`.
Work/scene/POV entries contain only stable `id` and `label`. Duplicate labels
remain distinct IDs. Names sort by full Unicode case-folded label and ID.
Named moments contain `id`, `label` and exact `at`, using event times or authored
scene start/current/end coordinates. They sort by timeline, numeric tick/order
and stable identity. Only moments at/before the selected horizon appear.

Author discovery can show authored source names at that horizon. Its private
work catalogue validates every offered work closure before returning the complete
list. A nonempty catalogue supplies the captured private `authorFingerprint`;
an empty catalogue supplies null. Character and dramatic-irony choices always
return `works: []` and `authorFingerprint: null`, while still validating any
explicitly selected work. Character and
dramatic-irony discovery admit knowledge, source scope and current scene presence
before exposing names, IDs, order or counts. Scene choices require actual scoped
presence. POV choices come from the current admitted record set. An unlearned
event cannot supply a named moment. Character catalogues contain no private work
titles or raw learning/acquisition metadata. Limits are 1–1000, independently applied to each
complete list; any overflow refuses the entire response.

Frame controls are local-author operations, including when selecting a character
lens. Their echoed canonical fingerprint and any work selection are supplied by
the caller. Nested character evidence keeps its separate metadata-free contract.
A canonical source change invalidates its old frame pin. A private manuscript
edit changes the private pin and invalidates bound work selections, while
unbound frames remain independent of private manuscripts. After explicitly
selecting the appropriate new pin, changes that preserve admitted evidence,
observable replayed current facts and exact scene eligibility leave the character
component unchanged. The scene cursor and participant intervals can affect that
eligibility at the fixed frame time; the world clock never replaces `frame.at`.
The browser remains a local author tool, not authentication.

The pure native operations use only their supplied snapshots. Unbound character
and dramatic-irony operations ignore private snapshots entirely. The filesystem
adapter `with_author_snapshot(world, frame, discover_works, read)` captures private
source only for a bound work or explicit author work discovery. Current capture
checks the loaded canonical fingerprint against fresh source before capture and
after the complete result, and rechecks private source and the publication stamp
before emission. Historical capture verifies canonical and private originals at
the same exact commit, with no current-source fallback. Unbound context/action
operations perform no private filesystem capture. Service Writing choices enable
author work discovery; resolve and context capture only an explicitly bound work.

## Explicit scene scope

A character frame with a scene requires that the scene's own scope permits the
viewer and the viewer is actually present at the exact horizon. Existing
inclusive participant intervals apply. Planned/abandoned scenes grant no
presence; an open active scene stops at its authored current cursor.

In addition to the existing self and held-knowledge roots, this selected scene
can admit its own ID, location, currently present participants, explicitly listed
objects/environments and the endpoints of scoped canonical outgoing relationships
with a transition at/before the horizon. Held ordinary knowledge may also add its
explicit `claim.subject` while a scene is selected. Every resulting record still
passes its own scope and domain applicability. These are direct roots, not a
transitive link traversal or inferred knowledge transition.

Typed assertion/genealogy pointers still grant no canonical payload or endpoint.
Future/retired/draft context briefs are unavailable as generic description owners
or typed reference targets even when their IDs have been acquired. Ordinary
owners retain their existing independently dated section behavior.

Only the new scene-aware frame path uses these extra grants. Existing
`character context`, search and narrative reads retain their prior no-scene
defaults. A frame does not permanently teach its roots or mutate knowledge.

## Complete writing context

`context(world, authors, frame, brief_ids, max_characters)` requires both an explicit
scene and POV. Its result contains `protocol`, `perspective`,
`scope: captured-writing-frame`, `frame`, `maxCharacters` and the selected
components:

| Lens | Returned components |
| --- | --- |
| `character` | `characterPrompt` |
| `author` | `authorMargin` |
| `dramatic-irony` | Both, separately admitted from the same captured world/time |

These components are structured native JSON. Character context retains existing
beliefs, conversations and observations, description sections admitted through
the selected scene scope, and the literal present facts below. Its `scene`
contains the exact scene/time and admitted location/participant/object/environment
identities, without source indices, raw metadata or canonical titles. Unknown
location is null; withheld list members are absent. Untimed ordinary bodies and
canonical assertion payloads remain excluded.

### Literal present facts

`characterPrompt.present` contains three complete arrays:

| Array | Admitted facts |
| --- | --- |
| `objects` | An explicitly listed, admitted object that is physically supported by an admitted, currently present holder, the admitted scene location, or an independently admitted container chain. Each row has `objectId`, selected `state` fields and their `valueTypes`. |
| `environments` | Canonical, source-scoped environments active at the exact horizon and explicitly targeting this scene or its admitted location. Rows retain `environmentId`, those selected `targets`, exact `time`, and authored `conditions`/`conditionTypes` and `sensory` when present. |
| `relationships` | The viewer's scoped canonical outgoing relationships to admitted targets, using the native latest transition at/before the horizon. Rows retain stable relationship/target/transition IDs, exact `time`, and authored metrics/types, facets, kind and status when present. |

Object state is restricted to operative `holder`, `location`, `container` and
unbound `condition`. Profile-bound placement exposes only its declared operative
entity leaf; profile siblings and bound conditions remain absent. Entity-valued
conditions require admitted endpoints. Container references grant no additional
record knowledge; unknown/hidden intermediate objects cannot support presence,
and a containment cycle does not establish it. A listed object alone is not
proof that it is physically present.

Canonical state replay supplies observable current values even when the event
that changed them is not character evidence. Event identities, causal notes and
prose remain omitted. Future and other-timeline effects do not apply. Active
environment targeting is local to this projection and grants no description,
profile or permanent learning access. Relationship values are literal authored
state; the projection computes no trust category, reciprocal attitude or rule
outcome.

Integer values use exact decimal strings, with type metadata distinguishing them
from authored strings and other scalars. False, zero, empty values and authored
absence remain distinct. No canonical titles, raw bodies, relationship notes,
file paths, source ordinals or global source metadata enter `present`. The
component permits at most 1,000 emitted rows total across the three arrays,
10,000 projected detail nodes and 4 MiB of compact serialized JSON. Placement traversal has a separate 10,000 admitted-node
guard; listed but physically absent candidates do not consume the emitted-row
limit. Bounds refuse incomplete results. The entire component also counts toward
the parent context's Unicode-scalar budget. These additions are confined to
selected-scene Writing frames; ordinary character context/search stay unchanged.

### Separate components and required material

The author margin composes the existing full author character context and the
exact selected author scene read. Its source citations and canonical values
stay in that component. The dramatic-irony `characterPrompt` is exactly the same
object as a standalone character-lens frame at the same selections and required
briefs and families. Its assembly cannot borrow author data or a newer source
snapshot.

Optional brief selectors are an ordered list of at most 100 exact brief IDs.
Repeated selections are stably deduplicated. Empty selection requires no briefs
capability. Nonempty selections use [BRIEFS.md](BRIEFS.md): complete required
material is admitted before measuring either component, with no more than 1000
emitted items per selected lens. An unavailable brief, scope reference or required
pin refuses the entire writing context with a generic unavailable result.
The author margin uses one complete union of author/public audiences and
author/ordinary perspectives, so independently scoped nested briefs and required
targets can compose together. Character pins retain their own independent scope.
Selected brief bodies are explicit dated source; ordinary untimed record bodies
do not acquire that exception for characters. Ordinary author body pins use the
same event, environment, aggregate and transition time gates as dated search.

The total budget counts Unicode scalar values in compact inner result JSON,
including frame metadata, both selected components and required material. Pretty
printing and the outer service envelope are excluded. Oversize results fail;
there is no ranking, clipping, truncation or partially accepted pin set. Native
budgets are 1–1000000; CLI/service/HTTP accept 256–100000, default 16000.

## Optional family context

`context_with_families(world, authors, frame, brief_ids, families, max_characters)` adds
explicit specialized reads from the same captured world, scene scope, POV and
exact frame time. The original `context` operation delegates with an empty
selection and preserves its existing output. Families are a separate request
object; they are never an additional frame field, stored world record or knowledge
grant.

The `families` object is closed, limited to 16 KiB and permits at most one member
of each family. Unknown or duplicate keys at any depth, null family members and
malformed selections are refused. `{}` has the same behavior as omission. Each
optional `beliefTargets` list defaults to empty and contains at most eight
distinct exact assertion or knowledge IDs.

| Member | Exact selection shape | Character component |
| --- | --- | --- |
| `species` | Required `individualId` and `selection`, plus optional `beliefTargets` | Existing explicit common/individual selectors over independently admitted concepts; dated owner sections and selected learned words stay separate |
| `social` | Required `language`, `sense`, `form`, `contexts`, plus optional `beliefTargets`; `sense` and `form` may be null | Known language/sense/spelling/script/register and explicit cultural roots, following the native social reader |
| `environment` | Required `contexts`, plus optional `beliefTargets` | One to eight known roots and their admitted direct links, following the native environment reader |
| `magic` | Optional `author: {rootId, selection}` and optional `beliefTargets` | One to eight explicitly selected learned assertion/knowledge targets; no canonical magic role composition |

Species `selection` retains its existing exact fields (`common_profile`,
`individual_profile`, `individual_link`, optional `common_link`, and role/field
maps). Magic's nested author `selection` retains the existing profile-role,
link-field, restriction and exception maps. These declarations select current
source fields; they do not introduce profile versions or inferred biology,
culture or magical rules. See [SPECIES.md](SPECIES.md), [MAGIC.md](MAGIC.md),
[SOCIAL.md](SOCIAL.md) and [ENVIRONMENT.md](ENVIRONMENT.md).

For a character-only frame, a magic selection can contain only learned targets:

```json
{
  "magic": {
    "beliefTargets": ["assertion_00000000000000000000000019"]
  },
  "environment": {
    "contexts": ["concept_00000000000000000000000032"],
    "beliefTargets": ["assertion_00000000000000000000000009"]
  }
}
```

An author or dramatic-irony frame that selects magic must provide its `author`
object. A character or dramatic-irony frame must provide one to eight magic
belief targets. Supplied canonical magic author selectors are never source-read
by the character branch. Author family branches ignore the character target
lists. Other families allow empty learned selections. Requested unknown or
forgotten learned targets preserve their native empty result; they do not grant
canonical knowledge or fail an otherwise available family.

Each returned component adds `familyContexts` with the selected family names
and complete native reports. Character reports admit concepts and endpoints
before traversal, sorting, bounds and evidence selection. Their typed values
retain exact used shapes and identities without current author titles, bodies,
source citations or original reference positions. Dated sections use the same
captured scope, and selected learned words are never joined to canonical
assertion subjects, classifications, values or corrections. Character magic
contains literal learned explanations only. The author margin independently
contains the existing authored projections, citations and classified accounts.

All required family roots must resolve; missing, withheld or wrong-role roots
refuse the whole writing context with the generic unavailable result. Family
reads are collected before the final packet budget, and a late unavailable
family takes precedence over another family's or a required brief's limit failure. Existing required
brief pins retain their stronger all-or-nothing admission contract. No family
may return a partial neighboring packet when another required family fails.

A native `linkageComplete: false` report is retained with its explicit admitted
unknowns. This describes a fully returned report with unresolved selected
linkage; it is not truncation or a claim that an action is ready to execute.
The separate [magic requirement capture](ACTIONS.md#separate-captured-magic-requirements)
requires complete authored linkage. Character and dramatic requirement captures
use learned-only magic accounts and always compare literals as unknown.
Explicit [consequence plans](CONSEQUENCES.md) remain separate inputs; a family
report never derives their operations or approves their candidate source.

The new character family reports retain their native complete record/traversal
guards, a combined 512 admitted evidence-item bound and a 262,144-byte compact
UTF-8 report bound including a newline. Existing species/magic author traversal
limits remain unchanged; their optional native assertion/description evidence
has the same 512-item aggregate bound without requiring previously undeclared
capabilities. The enclosing writing context still applies one Unicode-scalar
budget to both components, frame metadata, selected briefs and every family
report. A dramatic-irony `characterPrompt` is exactly equal to the standalone
character component with the same selections.

## Private review and proposal context

[Private action review](ACTIONS.md) reuses the frame's source, scene, POV and
lens, while the draft's saved horizon supplies the review clock. Character and
dramatic review require actual POV presence at that horizon. Intent, question
answers and proposal plans remain private input; they are never inserted into
admitted character context or the author margin as facts.

Every action now has required ordered `proposals` history, initially empty.
New human proposals retain an exact context fingerprint, ordered answered-question
snapshot and explicit native consequence plan. Appending from resolution-needed
or stale reasoning requires a fresh review of the completed proposed draft and
an explicit sufficiency decision. The review digest covers the whole draft,
including immutable history and exact raw plan strings, without adding them to
the context packet as evidence. A previously reviewed resolution draft cannot
authorize a later append.

Original draft base, each proposal's context and the current save binding remain
independent. An unsuperseded proposal whose source has changed makes a proposed
action effectively stale; reading a new frame never refreshes that provenance.
Explicit supersession preserves the earlier proposal unchanged. Editing proposed
content requires the stale phase. Proposal history, review confirmations and
plans are not new frame fields, world records or knowledge grants.

An [exact saved-proposal preview](ACTIONS.md#preview-an-exact-saved-proposal)
selects current source, private-file hash, action and proposal IDs; it does not
take another frame or replace the stored plan. The candidate source and complete
diagnostics are reviewable even when final validation reports errors. All
previews remain read-only and `canApply:false`. The
[proposal walkthrough](../examples/actions/PROPOSALS.md) demonstrates this
workflow. Separate [outcome approval and application](OUTCOMES.md) now publish
one exact saved alternative with a terminal receipt and guarded recovery;
optional [semantic analysis](SEMANTIC_CONSEQUENCES.md) adds explicit checks and
a same-horizon comparison without changing frame or character admission.

## CLI, HTTP and browser

```sh
wedl-rs --repo my-world writing resolve --file frame.json
wedl-rs --repo my-world writing choices --file frame.json --limit 100
wedl-rs --repo my-world writing context --file frame.json --brief brief_00000000000000000000000002 --max-characters 100000
wedl-rs --repo my-world writing context --file frame.json --families '{"environment":{"contexts":["concept_00000000000000000000000032"],"beliefTargets":[]}}' --max-characters 100000
```

Use `--file -` for stdin. Exact source snapshots can already be loaded through
the CLI's existing revision selection; a frame must match the loaded snapshot.
Pure reads of an old captured `World` and matching private snapshot remain
independent of later worktree changes. A current filesystem-backed bound-work
read instead requires that the served canonical and private pins still match
current source; it refuses stale captures until the caller explicitly refreshes
them. Original historical frames keep reading their selected commit.

The closed GET routes are `/api/writing/resolve`, `/api/writing/choices` and
`/api/writing/context`. Each requires the `frame` query parameter containing the
closed JSON object. Choices additionally accepts `limit`; context accepts
`briefs` as a JSON string array, `families` as the separate closed JSON object,
and `maxCharacters`. Unknown or duplicate query
parameters fail. Author labels, arbitrary known-ID grants and independent time
overrides are not accepted.

The Writing browser panel reads and edits this tab's explicit frame, loads/saves
local frame JSON, and renders the character and author components separately.
Changing inputs invalidates prior results and pending requests before another
selection is resolved. No server-global frame or source mutation is created.
Explicit capture buttons copy one family selection into the Writing panel from
the family forms, preserving other selected families. The JSON summary is
read-only, with controls to remove one family or clear the selection. To change
a family, edit its form and capture it again. These selections use the Writing
frame's horizon and POV. Changing or clearing them invalidates prior results
and pending requests. Frame-file download saves the six-field frame, including
its nullable private work selection;
family JSON remains a separate explicit input through CLI and HTTP.

### Browser history and refresh

Successful native validation saves only the exact six-field frame in this tab's
browser history. Back, Forward and reload restore those inputs and revalidate
them through the native resolver before enabling frame-dependent actions.
Original canonical and private work pins are retained. A stale frame stays
visible but unusable until the author explicitly selects fresh pins. Malformed
saved shapes are rejected before they can populate the controls.

The first edit of an accepted frame creates one unselected draft history entry;
accepting its replacement updates that entry. Repeated acceptance of the same
frame creates no duplicate. History contains no context, prose, family selectors
or unrelated drafts, and preserves other browser state. Each tab has its own
selection. A source refresh revalidates the unchanged accepted frame only while
Writing remains active; it does not capture new pins or add a history entry.
Edits, view changes and newer requests discard late validation results.

The same tab has separate [shared-time](CURRENT_TIME.md) and
[scene/movement](SCENE_CONTROL.md) controls. Each captures its own source pin and
explicit time, exports an ordinary consequence Plan and leaves the Writing frame
unchanged. Editing either control invalidates its previous report and pending
responses. Publication still uses the separately reviewed Actions workflow.

## Pinned evidence and remaining scope

This contract follows the operational boundaries in
[`writing_frames.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/writing_frames.py),
[`context.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/context.py),
[`brief_runtime.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/brief_runtime.py)
and the pinned
[`test_writing_frames.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/tests/test_writing_frames.py),
[`test_writing_frame_transport.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/tests/test_writing_frame_transport.py)
and [`test_writing_workflows.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/tests/test_writing_workflows.py).
Rust uses its existing exact canonical/private source fingerprints, immutable
snapshots and complete-result bounds
instead of copying revision-only requests, compiled historical caches or ranked
truncation. It does not add a new source version, migration or adapter.

Specialized character family composition and explicit family context selection
are now implemented. Explicit [semantic/hybrid search](RETRIEVAL.md) is separate
from the complete frame packet and does not rank or truncate required pins.
Private [action drafts](ACTIONS.md) now reuse the frame's explicit perspective
while taking their own saved review horizon. Human proposal history and
[explicit consequence previews](CONSEQUENCES.md) are implemented separately.
Magic requirement captures preserve learned-only character/dramatic reading
and cannot approve an outcome. Saved-outcome approval and application use the
separate [outcome contract](OUTCOMES.md), including selected required checks.
Private work binding and author work choices now use the existing
[manuscript source contract](MANUSCRIPT.md). They introduce no occurrence
selection or reader authority. [Typed retcon impact](RETCON_IMPACT.md)
and [saved reviews](RETCON_REVIEWS.md) take an independent explicit plan and target;
they do not infer dependencies or approval from a writing frame.
Standalone [revision review](REVISION_REVIEW.md) now compares three independently
admitted original Git snapshots; it does not supply a writing-frame selection.
The frame supplies explicit reading context; it does not authorize a write or
evaluate an action's consequences.
