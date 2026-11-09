# Implementation scope

The native v1 implementation accepts canonical `wedl/v1` and private manuscript
`wedl-author/v1` source, and emits `wedl-rust/v1` author responses and
`wedl-reader-presentation/v1` reader outcomes. Explicit operator files use
`wedl-reader-policy/v1`. Package, source, API and cache labels stay fixed until the entire
plan is complete. Breaking changes are allowed within v1; fixtures change
directly with the current rules. No migrations, compatibility shims or old field
aliases are provided. The v1 label does not imply full feature coverage.

## Working

| Area | Delivered behavior |
| --- | --- |
| Source | Exact UTF-8 Markdown bytes, duplicate-key/ID refusal, fingerprints, worktree and literal Git revision reads |
| State | Seven closed types, typed effects, presence-based exclusivity, exact integer output and deterministic temporal replay |
| Story points | Authored lifecycle, typed conditions, prerequisite states, cycle checks, pure eligibility and blocked reasons |
| Core reads | Knowledge, directed relationships, conversations, timeline, author context, explicit causal ancestry, active environments |
| Scenes | Exact inclusive participation, overlap validation, dated observations and author scene query |
| Character views | Exact-ID context/transcript over current own beliefs, linked learned words, admitted dated sections, audible beats, retained recollections and visible observations |
| Spatial topology | Canonical ancestry/direct children, route and portal adjacency, complete reachability, deterministic hop/weighted paths; exact same-unit route totals |
| Geometry and anchors | Two/three-axis local-planar and EPSG:4326 maps, point/line/polygon placement, bounding relations, Euclidean shape distances and literal map correspondences |
| Overlays | Static or inclusive time-bounded explicit memberships, with authored audience/view filters and complete-result limits |
| Spatial history | Typed location/route members, exact validity and corrections, historical containment checks, cited author history, effective route inspection and optional dated geometry |
| Calendar chronology | Exact cyclic/table days, partial civil/era formatting, bounds, explicit-epoch conversion and exact/interval story anchors |
| Chronology annotations | All seven source forms validate; inclusive partial dates, reference lists, supported pair relations and five date-search predicates preserve uncertainty |
| Parentage and unions | Exact author-horizon replay, active family/memberships, cited ancestry/descendants and deterministic parent/child paths; visible transition histories and explicit replacement links |
| Organizations and affiliations | Authored organization lifecycle, renames and hierarchy; current roles and optional former roles at the selected horizon |
| Legacies and succession | Legacy states and organization links; explicit tenures, holders, inclusive vacancy intervals, authored succession links and unresolved claims |
| Vital history | Known affirmative states from every visible canonical history; unknown/withheld disclosures excluded and conflicting histories retained |
| Learned genealogy | Seven typed knowledge assertion kinds; exact-viewer held-belief admission, learned family/organization/legacy/vital reads and cited paths without canonical bridges |
| Generational discovery and context | Author and learned labels, endpoint discovery, word-prefix search and complete bounded contexts at an explicit horizon |
| Structured lore | Closed world profile definitions, eight exact value types, scoped canonical concepts, explicit dated links, profile-bound state, projected typed references and cited author reads |
| Species descriptions | Explicit common/individual profiles and root fields; author evidence, independently admitted character composition and complete affected-individual lists |
| Magic profiles | Explicit role/profile composition with author assertions and sections; separate character learned explanations without canonical joins |
| Dated descriptions | Explicit stable local sections with exact Markdown, inclusive/timeless validity, scope filtering, local corrections and cited history/applicability reads |
| Authored assertions | Current finite predicates, classified propositions, explicit attribution, projected typed values and independent correction forks with source citations |
| Linked learned assertions | Exact assertion/knowledge targets, explicit affirmative acquisition, retained held rejection, forgetting and literal learned wording independent of canonical truth |
| Shared character narrative reads | Private derived record scope; admitted dated sections, typed concepts/links and spatial members; exact IDs and horizons without source metadata |
| Authored context briefs | Scoped required record/section/transition pins, recursive stable deduplication, complete admission before limits and explicit unknown when any pin is unavailable |
| Captured writing frames | Six required fields with nullable exact private work binding, same-revision canonical/private pins, author-only work choices, explicit scene/POV/time/lens, literal admitted present facts, separate complete character/author components and native browser-history revalidation |
| Social/language context | Exact language/sense/form selection, independently dated spelling alternatives, explicit cultural roots and separate assertions/sections |
| Environment/material/technology context | Seven current profile roles, one-hop admitted typed links, root evidence, unknowns, differing literal accounts and cited edit hooks |
| Existence | Independent scoped episodes with explicit kind and interval, historical evidence syntax, exact author history/as-of lists and no vital-state inference |
| Collective parties and governance | Closed typed Party conversion, six explicit current-profile roles, authored location/overlay jurisdiction, independent bases/obligations, linked native assertions and cited supersession pointers |
| Search | Unchanged default all-time search; explicit dated prose/core-field admission and post-ranking thread grouping across lexical/local LSA/hybrid modes; optional separate all-time lexical hypothesis lane; independent public character training prior and admitted-only evidence |
| Shared current time | Pure exact-source preview of one Plan updating the world and every active scene together, with complete candidate validation and the existing separately reviewed outcome publisher |
| Coordinated scenes | Exact scene create/advance/close and character-move Plans, inclusive handoffs, listed carried-object transfer and candidate placement reconciliation |
| Authoring | Minimal new world; typed creation/updates for all twenty-eight non-world kinds, kind-specific common fields and world timelines/capabilities/threads/metrics/chronology/profiles/predicates/state keys/bindings; literal affordances; complete candidate preview and the existing exact atomic save |
| Hypotheses | Author-only all-time list/detail; explicit create, adopt-existing and reject Plans; complete preview/export and closed source validation; no canonical inference or learning |
| Private actions | Separate durable JSON drafts, exact source/file pins, human-confirmed readiness, immutable proposal history, complete consequence previews and separately approved saved-outcome publication with terminal receipts and guarded recovery |
| Retained edits | Exact private buffers, explicit body mode, separate base/save/file pins, complete saved review and approved target/receipt publication |
| Historical author reads | Original committed private action/edit/manuscript bytes and blob metadata, required inert historical citations, and bounded three-way revision review at one exact horizon |
| Private manuscripts | Exact segment/work Markdown, separate source fingerprints and guarded saves, literal chapter/segment occurrences, explicit reuse and current or same-commit historical assembly |
| Private reveal plans | Closed occurrence-bound record/section/text admissions, shared manuscript source bounds/fingerprint/saves, complete selected work validation and private author resolution |
| Reader disclosure | Cumulative reveal admissions intersected with trusted field permissions, labels, exact time and character learning; minimal units, correction filtering and conservative body privacy |
| Reader presentation | Installed original-commit public/simulation policies, complete admitted-only paging/search, authenticated cursors, revocation fences and a separate loopback reader origin |
| Retcon impact and review | Typed canonical/private reverse occurrence graphs, manuscript source/POV/work edges, original/candidate citations, immutable private analysis/decisions, admitted-input witnesses and independent live/first-parent currentness |
| Event and grouped consequences | Exact-time local contribution exclusion, delayed history, explicit outcomes/successors/supersession and separate attributed or mixed source sections |
| Literal requirements | Independent selected magic capture, exact typed author field/assertion comparison and explicit unknown for learned-only character/dramatic capture |
| Interfaces | Shared library/CLI/author HTTP with guarded private saves and approved publication, local browser with Generations, Lore, Narrative, Writing, Hypotheses, Actions, Edits, Revisions, Manuscripts/reveals/reader simulation, Retcons and Consequences controls; separate reader assets/POST routes, exact source inspection, strict decoding and tab-local private import/export |

## Native boundaries

- Static native Plan schemas, editable kind/field/default/transition guidance,
  Bash/PowerShell completion, validated author context and editable scaffolds are
  implemented through shared interfaces. The integrated callable-scope review
  found no further named feature gap after the Writing additions below. Full
  starters refuse valid direct-source values that the typed update
  codec cannot represent; they never discard those values to manufacture a Plan.
- Character views use the explicit evidence and family readers. Ordinary bodies
  and inferred canonical state or relationships remain outside generic character
  evidence. Exact Writing frames additionally admit their selected scene's
  observable object/environment facts and the viewer's own directed relationship
  state; those facts do not become permanent knowledge.
- Browser Writing history restores exact accepted frames through Back, Forward
  and reload, then requires native revalidation. Refresh revalidates an unchanged
  accepted active frame with its original pins. No restoration silently repins.
- Explicit removal of top-level optional fields and deletion of records remain
  direct source work. Reviewed browser publication uses the existing writer.
- Reader presentation exposes admitted record, section and text units, matching
  the pinned page/search consumer. Typed spatial/domain composition remains in
  the separate author/character readers. Generic dated author retrieval retains
  explicitly listed core field lanes, with typed lore/assertion/history
  projections in their own workflows.
  Executable coordinate transforms are outside the pinned implemented
  target; authored anchor correspondences are supported.
- Author outcomes use explicit Plans and reviewed publication. Magic requirement
  comparison stays literal, and canonical role composition remains author-only;
  neither computes an action outcome, cost, sufficiency or rule execution.
- Historical citations identify records, sections or transitions. Saved-outcome
  review authenticates their original owner/path/blob; no standalone arbitrary
  historical-citation resolver is part of the pinned callable target.
- Event/grouped analysis discloses its
  native semantic scope. Typed relationship/story-point edits now use the ordinary
  consequence Plan codec, with no automatic reciprocity or eligibility activation.
- Full-author briefs now use one complete audience/view union closure, including
  typed endpoints and nested lifecycle pins. Future unbound scalar-entity effects
  are omitted before semantic replay; sets, profiles and initial values retain
  their existing semantics, matching the inspected consequence boundary.
- Three-way review uses the complete native author-union collector for required
  brief closures and does not invent a merged world or implicit context; see
  [REVISION_REVIEW.md](REVISION_REVIEW.md).

Core navigation reads, admitted catalogue page/labels, saved-action original
evidence verification and private hypothesis links are implemented. The fresh
pinned scope audit confirmed the authoring helper inventory above; native
schema/context/scaffold operations consolidate those upstream route families.
The current audit also supplied the observable Writing component and automatic
browser-frame revalidation. Earlier restoration and compiled-artifact mismatch
observations remain unresolved; this is not exhaustive upstream conformance.
See [CORRECTNESS_REVIEW.md](CORRECTNESS_REVIEW.md).
See [PARITY.md](PARITY.md) for the functional iteration estimate, evidence and
uncertainty. The estimate targets the pinned original implementation and excludes
migrations, exact wire/source compatibility and release infrastructure.

Declared optional capabilities select the implemented native domain checks;
coverage warnings retain their partial-validation and noncertification limits. Raw
record inspection is available to the local author; it is not a semantic query
or character entitlement check. Unknown operations fail rather than return
successful placeholder data. `validate.complete` remains false.

## Deliberate limits

One Cargo package with separate local-author and isolated public-reader origins,
and separate canonical and private manuscript source schemas. Single-record
creation and editing use optimistic source checks and an OS lock between WEDL writers.
External editors require coordination. Reviewed saved outcomes additionally
use a bounded recoverable multi-file journal and native reader fence. No
automatic Git commit, authenticated multiuser server or external vector-search
service exists. Current-worktree author requests capture fresh canonical source;
six guarded publication routes reuse the existing writers. The browser refreshes
after confirmed writes while retaining unsaved input and its original pins.
Historical and immutable routers remain read-only. Private operations retain
their own source capture fences. Retcon queue/detail reads independently capture
a live comparison and may retain original private source with `unknown` currentness.

Private actions live in durable `author/actions/<action-id>.json` files outside
the canonical `story/` loader and disposable cache. Their exact bytes, IDs and
prose never contribute to canonical counts, fingerprints, validation, search or
character context. The existing writer lock protects atomic single-file saves;
current source fingerprint and prior private-file SHA-256 are separate checks.
Creation uses a null prior hash and the current source as immutable original
base. Updates preserve that base. Competing edits refuse rather than merge or
overwrite. A separate approved outcome path uses a publication journal and
terminal applied receipt. Historical list/detail additionally read original
private action/edit blobs at the selected exact commit without a live fallback.
Current-only consumers remain fenced. No Git staging/commit or private delete
operation is implemented. Retained edits use `author/edits/<edit-id>.json`, keep
unfinished patch/body text, and apply through the shared publisher. Independent
three-way revision review reads three original commits without creating a
merged candidate. See [EDITS.md](EDITS.md) and [REVISION_REVIEW.md](REVISION_REVIEW.md).

Private manuscripts use `wedl-author/v1` under `author/segments/`,
`author/works/` and `author/reveals/`. Their exact bytes and paths define a separate fingerprint;
other private stores and canonical `story/` source do not enter it. `manuscripts`
provides `list`, `detail`, `resolve`, `preview-save` and `save`, each with `--file FILE`.
The first four have read-only GET routes under `/api/manuscripts/`; current author
saves use the guarded POST route or CLI. The Manuscripts tab preserves exact
imported source until edited, resolves saved occurrences/reveal plans, saves
private source and downloads exact native requests. See
[BROWSER_PUBLICATION.md](BROWSER_PUBLICATION.md).
Historical source is read-only and uses original private/canonical bytes from
one literal full commit, without live fallback. Independent source inspection
remains available when canonical references are invalid.

Saves require current canonical and whole-manuscript fingerprints plus the exact
prior-file hash (`null` for creation), validate complete candidate dependencies,
and repeat source checks under the shared writer lock before atomic replacement.
Same-content saves still require valid pins. Complete raw and expanded-source
bounds fail rather than returning partial data. Work order follows chapter and
segment arrays, including explicit reuse; placement supplies no reorder, reader
grant or character evidence. Typed retcon source/POV/work edges preserve authored
occurrences and original private provenance. Saved-review witnesses include
relevant private changes and newly incoming references without using the whole
author fingerprint as a staleness proxy. See [MANUSCRIPT.md](MANUSCRIPT.md),
[RETCON_IMPACT.md](RETCON_IMPACT.md), and the
[manuscript walkthrough](../examples/manuscripts/README.md).

Reveal plans select one exact work and ordered chapter-local occurrence
milestones. Each cumulative admission is an exact canonical record, an exact
authored description member (including a replaced member), or literal text with
a unique local `revealtext_` ID. Source shape, milestone order, duplicate
identities, full selected work closure and canonical links are validated before
projection. The pure compiler and private `reveal-plans resolve --file FILE` /
GET `/api/reveal-plans/resolve` return source-bound author reports; current/null
resolution is an explicit native author adaptation. The reader requires an
original committed edition. Complete candidate saves validate all three private
kinds. Retcon source capture keeps reveal structural validation but deliberately
excludes reveal semantic references and graph nodes, matching the installed
segment/work consumer. See [REVEAL_PLANS.md](REVEAL_PLANS.md).

Pure reader disclosure requires independently supplied ordinary authority and
intersects record/field paths, owner/member labels and exact story-time
applicability before correction suppression and admission matching. Section-only
units contain their own heading without the owner title/body. Text admissions
preserve their exact string. No private plan/work/segment labels, source hashes,
paths, links or prose enter reader units. Seven ordinary body kinds retain
temporal and heading-audience restrictions. The native implementation
conservatively retains private ancestor restrictions through every descendant,
unless a nearer explicit audience mapping overrides them. Heading matching
retains Unicode folding, private-name and line-boundary behavior. Character
lenses reuse native learning and always exclude raw body Markdown. No placement,
POV or reveal admission invents character acquisition. See [REVEALS.md](REVEALS.md).

`reader page`, `reader search` and `reader preview` take closed `--file` requests
and optional explicit trusted `--policy` configuration. `serve-reader --policy`
defaults to loopback port 8766 and exposes only reader HTML/script and page/search
POST routes. `serve --reader-simulation-policy` separately installs the author
simulation used by the Manuscripts tab and GET `/api/reader-presentation/preview`.
The CLI loads operator files at startup; native callers can replace or revoke
public/simulation installations, with a final installation check before emission.
There is no HTTP policy installer, default grant, source discovery or permission
derived from the local author session.

Trusted `wedl-reader-policy/v1` files pin the exact original Git revision,
work/plan, canonical field scope, generation, maximum occurrence and a 32-byte
cursor secret. Public policy additionally fixes publication, horizon and text
policy. Simulation adds a principal and author-permission scope, intersected
with the canonical scope. Public reads use only public/ordinary labels; simulation
does too. Request JSON never supplies grants, generation or secrets. An absent
or mismatched policy returns uniform unavailability before source access.
Original canonical/private capture remains authenticated and bounded, with
repository identity checks before/after reads and no working-file fallback.

Native cursors are 72 lowercase hexadecimal characters containing an offset and
HMAC-SHA-256 tag, with separate public/simulation domains. They bind repository,
edition, work/plan blobs, issuer/scope, horizon, occurrence ceiling, text policy,
generation, complete admitted dataset and the exact page/search selection.
This intentionally differs from upstream's base64 JSON cursor representation.
Search performs Unicode casefolded substring matching only inside admitted titles
or Markdown. Complete work/results are bounded to 1,024 occurrences and units,
2 MiB admitted Markdown and 4 MiB serialized response; paging cannot bypass a
complete-data overflow. Requests are at most 16 KiB, with page limits of 1–100 items.
Both reader surfaces render text and invalidate late responses when selections
change. The public origin contains no author/source/status/token routes, refuses
foreign Host/Origin values, and sends `no-store` responses. It is an anonymous
loopback publication consumer, not a general authenticated multiuser server.
See [READER.md](READER.md) and the executable
[reader walkthrough](../examples/readers/README.md).

The closed private action draft has intent, exact actor/target references, saved horizon,
questions, body, required `evidence` arrays on drafts/proposals, required `proposals` history and one of `draft`, `context-needed`,
`resolution-needed`, `proposed`, `stale`, terminal `cancelled` or terminal `applied`.
Only the dedicated outcome operation can enter applied and add its receipt. New actions begin
in draft with empty history. Answer absence differs from an explicit empty string;
null answers are invalid. Every resolution-needed save and newly appended proposal
requires a fresh complete review and explicit human sufficiency confirmation.
Saving recomputes the digest over the exact draft and selected context. Review
uses the saved action horizon with the selected frame’s scene, POV and lens;
private prose, plans and answers never become admitted evidence.

Human proposals form an immutable prefix of at most 64 entries. Each entry has a
stable ID, summary, source/context pin, ordered snapshot of all answer-present
questions, literal `human` resolver and explicit consequence plan. Optional
supersession retires only its named earlier entry. Appending is available from
saved resolution-needed or stale reasoning, with the completed proposed draft
reviewed afterward. An unchanged proposed save takes null review. Substantive
edits require stale, and a phase-only rejection to resolution-needed requires a
fresh review. Any active proposal retaining an old source pin makes the effective
phase stale. Saved-outcome historical evidence resolution and private hypothesis
references remain named functional gaps; the pinned executable consumer is
identified in [CORRECTNESS_REVIEW.md](CORRECTNESS_REVIEW.md). External resolver
execution and automatic outcome or knowledge generation are absent. Explicit approved
application, current-state replay, exact-source receipt verification and rollback
are documented in [OUTCOMES.md](OUTCOMES.md). Optional plan analysis now adds
seven explicit expectation predicates and a same-horizon delta across typed
state, knowledge, relationships, story-points and outcome links. Required checks
are bound into approval; current semantic rechecks remain separate from exact
source verification. See [SEMANTIC_CONSEQUENCES.md](SEMANTIC_CONSEQUENCES.md).

Consequence plans use exact raw YAML strings in 1–64 record-create/update
operations, bounded by 256 KiB. Pure candidate construction preserves existing
single-record codec behavior and validates the complete final world. Reports
contain exact before/after source, deterministic diffs, candidate hashes and
diagnostics; invalid but constructible candidates remain inert private plans.
Standalone preview is CLI/native only. Saved preview additionally binds the
current source, exact stored bytes and selected active proposal in a current
proposed draft. Neither preview writes files, creates caches or locks, or applies
the candidate. Complete result limits are 256–1,000,000 Unicode characters plus
an 8 MiB native byte cap; oversized results fail without truncation. See
[CONSEQUENCES.md](CONSEQUENCES.md).

Action draft JSON and requests reject duplicate/unknown fields and preserve exact
decimal-string coordinates and Markdown content. A draft is at most 256 KiB and
a request at most 512 KiB, with additional field/structure bounds. The complete
store is bounded by 1,024 drafts and 16 MiB. GET list/detail/review/preview-proposal
and separate requirement capture/compare retain the existing 16 KiB raw URL bound.
Saved-proposal GET uses compact exact selectors rather than a plan payload.
Larger review inputs can be exported for the native CLI; HTTP private save,
private-save preview, delete, approval and apply routes do not exist. The Actions
panel retains unsaved input only in its own tab, preserves valid UTF-8 imported
prose and raw YAML until edited, separates answer presence and human sufficiency,
and invalidates stale pending results. Only an unchanged pinned saved proposal
can be previewed. Browser discard clears local input; cancellation is the durable
private operation. See [ACTIONS.md](ACTIONS.md) and the
[worked proposal example](../examples/actions/PROPOSALS.md).

YAML follows the Rust parser's scalar rules; PyYAML compatibility is not a goal.
Exact ID/title/alias resolution is implemented; slug suggestions are not.
Character admission uses explicit source scope and exact story horizons. Section audience declarations
are validated but do not make untimed prose visible. A recorded observation or
audible beat does not synthesize a belief or recollection. Conversation membership
is independent of linked scene membership. Recollections retain authored memory
text; inaccessible transcript IDs are omitted instead of expanding canonical text.
See `CHARACTER_VIEWS.md` for the full boundary.

Shared character narrative reads derive a private scope from the exact viewer and
current scoped knowledge, including plain `claim.object.entity` acquisitions.
Typed assertion/genealogy pointers and literal names do not grant their canonical
targets. Owner/member labels use `public` or the viewer ID for audience and
`ordinary` or the viewer ID for perspective. Direct character read requests cannot
provide extra known IDs, an author label or a scene grant. Captured writing frames
derive scene-local admission through validated presence as described below.
Concepts retain canonical status and exact
validity; sections and spatial members retain their own inclusive bounds.

Nine separate `character` commands and closed GET routes expose assertions,
descriptions, section, lore, concept, links, spatial-history, existence and
governance. Every operation requires a tick, with optional timeline/order. List
limits are 1–1000 and refuse incomplete results. Character successes contain only
the protocol/result transport envelope and omit canonical titles/body/provenance,
source paths, hashes, revisions, fingerprints and source-array indexes.

Linked knowledge claims contain exactly key, statement and assertion_id. At least
one affirmative acquisition is required before the horizon; a later held rejection
retains the learned wording, while forgetting removes it. Current canonical
validity, corrections, values and attribution never replace learned words.
Generic character context/search now include these linked claims and admitted
dated sections before corpus construction, ranking or budget checks. Admitted
typed concept/links/spatial reads are separate inspectable projections and do not
invent additional learned claims.

Search now has explicit `lexical`, `semantic` and `hybrid` modes on the existing
author and character CLI/HTTP entry points. Without author horizon or thread
selections, omitted mode and explicit lexical mode call the old functions
unchanged, including their complete wire shape,
BM25 direction and disposable author cache. Unknown, empty, incorrectly cased
or duplicate modes fail. Query limits remain 4,096 UTF-8 bytes/64 terms and
1–100 returned rows. No provider, model, dimension, score-threshold or grant
parameters are accepted.

Author search accepts an optional explicit horizon and 1–32 sorted unique exact
declared thread IDs. The selected dated corpus keeps ordinary identity titles
timeless while admitting bodies by authored event time, completed aggregate end,
inclusive environment interval or earliest timed transition. Applicable dated
descriptions are independently scoped, corrected and cited candidates. Future
bodies, other-timeline intervals and non-author members are excluded before lexical statistics,
semantic fitting and input bounds. Raw typed lore, assertions, generational and
existence aggregates, assertion-linked knowledge, context briefs and hypotheses
are excluded from this generic dated lane; applicable description members on
typed owners retain their own admission. Supported core environment conditions,
transitions, unbound event effects, knowledge statements, observations and
conversation members now contribute separately dated, cited rows. Raw profile
values and other domain aggregates are not flattened into that corpus.

Thread grouping preserves a stable ANY-membership subsequence after complete
ranking and before the output limit. It changes neither training nor scores.
Thread-only reads preserve established all-time semantics, including lexical
kind filtering after global BM25 statistics. CLI repeated `--thread` values are
sorted without deduplication; the shared native/HTTP contract rejects duplicates
and unsorted lists. Explicit horizon/thread requests are read-only and use
request-local tables, never the persistent author FTS cache. They confer no
character knowledge or reader authority. See [RETRIEVAL.md](RETRIEVAL.md).

Semantic retrieval is ephemeral local LSA over word unigram/bigram TF-IDF with
a bounded truncated SVD solved through a Gram/Jacobi procedure. Author training
and candidates use the admitted title/body/description corpus after the exact
kind filter and exclusion of hypotheses; omitted selections retain the existing
all-time title/body corpus. Character targets are precisely the existing
admitted evidence documents. Their training prior is independent of viewer,
horizon, query and candidates: eligible timeless public/ordinary description
headings and Markdown on canonical/active/closed world, location and object
owners with no `valid` or `time` qualifier. Owner labels must also permit that
public/ordinary scope. Public eligible corrections apply inside the prior;
private, dated and ineligible-owner sections do not suppress eligible training.
Canonical titles, body prose and character target words do not train this prior.
Training texts never become candidates through their training role.

Nonlexical results add `mode`, model metadata under `retrieval`, and nullable
per-row lexical/semantic ranks and scores. Semantic ranking keeps finite cosine
scores, including zero and negative scores, when the query projection is nonzero.
Zero query projection yields no semantic hits. Hybrid combines complete lane
rankings with equal-weight reciprocal-rank fusion at `k = 60`, then applies the
result limit; a genuine lexical lane may contribute when the semantic query
projection is zero. Scores describe retrieval order, not belief confidence or
story truth. Stable ties use candidate identity; private source titles, paths,
indices and corpus statistics cannot determine character order or counts.

Missing required training, failed convergence and exceeded complete-input or
work bounds are errors, never an implicit lexical fallback. Nonempty featureless
training retains the pinned inert feature and produces no semantic query hits. The engine admits
at most 1,024 combined training/candidate inputs and 512 KiB of charged text,
then at most 512 distinct training texts, 8,192 selected features and 192 requested
dimensions, with numeric-work caps. No model or index is persisted by semantic or
hybrid reads. Both new modes preserve character protocol/result-only envelopes.
The Search browser separates retrieval mode from perspective, preserves exact
time text, and invalidates earlier or pending results on query, viewer, horizon,
mode, source or tab changes. There is no source schema, dependency or cache-version
change.

Context briefs require `briefs-v1`, an exact `brief_` identity, explicit domain,
scope, validity, audience/perspective labels and ordered required references.
Region/era scopes select a current exact location/overlay record or world era
section. Required record, section and transition references validate their exact
owner and member. Composition follows nested brief requirements, stably
deduplicates targets and terminates cycles; it performs no graph-wide expansion
or semantic ranking. An unavailable required pin makes the whole selected brief
unknown before result-limit checks. Unknown output contains no partial text or
hidden-target explanation. Lists are complete or refuse their 1–1000 bound.
The admitted brief's own title/body is explicit context material. Ordinary
character record targets contribute only safe identities or admitted domain
projections, with no automatic import of source bodies or canonical labels.

Author `briefs compose` permits an optional horizon and explicit presentation
labels. `character brief` requires an exact viewer/horizon and independently
admitted brief and required pins; its outer envelope remains protocol/result
only. The browser exposes both choices in Narrative with irrelevant controls
disabled and omitted. Required selection is separate from incidental context and
does not create character knowledge.

Captured writing frames have six required fields: `fingerprint`, nullable `work`,
nullable `sceneId`/`povId`, `at:{timeline,tick,order}` and `lens`. `work` is null or
exactly `{workId,authorFingerprint}`, selecting an exact private work and its
complete private source hash. Time coordinates are canonical bounded decimal
strings; canonical and private hashes are exact lowercase 64-character fingerprints.
Unknown/duplicate fields, alternate selector shapes and missing coordinates fail
before private selection lookup. There is no occurrence selector, current-time default or
fallback scene/POV. Resolve echoes a valid frame; stale, hidden, absent or wrong-kind
selections fail uniformly without disclosing a replacement source pin. Character
and dramatic-irony lenses require a POV, and context requires both a scene and POV.

Bound work validates its complete segment closure against canonical/private
snapshots at the same revision. Historical reads use the same original full
commit without live fallback. Current filesystem-backed capture rechecks the
served canonical pin, private source and publication stamp before emitting the
complete result. Pure native calls take supplied snapshots; unbound context and
action reads do not capture private files. Work order, placement, source links and
private POV annotations supply no frame coordinates, prose, learning or reader
disclosure. Private edits invalidate bound private pins without changing the
canonical fingerprint or unbound Writing admission.

Frame choices filter before labels, ordering and complete list bounds. Only the
author lens offers minimal work IDs/titles and a private catalogue fingerprint;
character/dramatic choices return `works: []` and `authorFingerprint: null` while
still validating any bound work. A character
scene must admit the viewer's exact presence; that scene's explicit location,
present participants, objects and environments provide ephemeral scope subject
to each target's own labels. Context renders an admitted character prompt, an
author margin or both, preserving the same character component under dramatic
irony. Selected briefs are checked before the complete compact-result budget.
CLI/HTTP budgets are 256–100,000 characters, default 16,000; native callers may
request 1–1,000,000. There is no truncation or outcome synthesis. Writing calls are
local author controls, so their outer envelope/frame retain the captured source
pin; the nested character prompt contains no global source metadata.

Optional writing `families` are separate from the six-field frame. The closed
16-KiB JSON accepts species `individualId`/`selection`, social
`language`/nullable `sense`/nullable `form`/`contexts`, environment `contexts`,
and magic `author:{rootId,selection}`. Each accepts optional `beliefTargets` with
at most eight distinct exact assertion/knowledge IDs. Unknown or duplicate keys,
null family objects and malformed selections fail before private lookup. Magic
requires its author selection for author/dramatic lenses and at least one learned
target for character/dramatic lenses. The character branch does not inspect the
canonical author selection; the author branch ignores learned targets. Selected
family results appear in the applicable packet's `familyContexts` object.
Absent or empty families preserve the prior context shape. The portable focused
example fits all four families under all three lenses at the documented horizon.
The broader example fits character and author separately but exceeds the public
100,000-character budget under dramatic irony; this is a complete-result error,
not a truncated packet or an automatic change of selected families.

The Writing browser tab loads/validates exact frame files, offers selectable
author work and admitted scene/POV/moment choices, exports validated JSON and separates character/author
panels. Lore and Narrative offer explicit family capture; each capture replaces
only that family, and the two magic captures preserve the opposite branch.
Writing shows the exact selection with per-family removal and clear controls.
Each browser tab owns its frame and family selection. Field/lens/source/family
changes invalidate earlier results and pending asynchronous responses. Neither a frame read nor a
choice changes world/scene cursors, source bytes, another tab or publication state.

Complete character responses, rankings, counts and budget failures are tested
against valid private/future edits. The application still validates the whole
snapshot first and returns a generic failure for invalid source. This is a local
author tool, without authentication or a guarantee against timing side channels.

As-of context excludes untimed bodies. Environments match authored targets
without inferred propagation; causal history follows authored edges only. Typed
requests reject unknown fields, top-level nulls and identity edits; arrays/maps replace as
a whole. Structured updates normalize the YAML header while retaining omitted
metadata values and exact body bytes.
Chronology permits supported nullable data such as an isolated calendar's
`epoch:null` and open annotation bounds; null never acts as a field-delete instruction.

Structured lore requires explicit `lore-core-v1` opt-in. Profiles contain closed
object schemas over string, boolean, number, null, entity, enum, array and object
types. They define no defaults, executable expressions or coercion. Current
profile definition updates validate all dependent concepts, bound initial values
and bound `set` effects directly; v1 has no
profile version-history, migration or hash-pinning layer. Whole profile/value
replacement can remove nested optional fields. Explicit `concept_links` arrays
can be replaced or cleared on the world and every editable non-world kind;
hypotheses cannot own them.

The `lore` commands are author presentation operations. Profiles/profile are
all-time registry reads. Catalog/concept/links default to audience/view
`author`/`author`, with an optional exact horizon and catalogue-only exact domain
filter. No horizon considers all authored timelines; a supplied horizon applies
inclusive validity and exact timeline matching. `order`/`timeline` without
`tick` fail. Concept status, time and labels filter before title/alias resolution,
reference projection and complete-result bounds of 1–1,000. Draft, retired and
out-of-scope concepts are unavailable; typed references and links to them are
removed. Ordinary non-concept record targets and owners retain their existing
author boundary across statuses and event times. Existence-episode owners and
targets additionally require canonical status, applicable validity and their own
scope labels together with their character endpoint's scope labels. That episode
gate applies to descriptions and typed references in lore, assertions and
governance. Context-brief owners and typed targets independently require their
own canonical status, labels and applicable validity; generic endpoint admission
does not compose their required closure or region/era scope reference. Integer output is exact decimal text with explicit type metadata;
enum scalar types and original source indexes in reference citations survive
projection. Canonical values do not enter generic character responses or infer
traits, abilities, membership, authority or knowledge. Separate `character lore`,
`concept` and `links` commands project admitted typed data without author source
metadata. The browser displays these structured results and provides no lore
editing form.

World `state_profile_bindings` explicitly selects an object profile for a declared
kind/state key. The profile governs initial and `set` values; `clear` removes a
bound value and set-member operations are rejected. Binding, profile and state-key
updates validate the complete current candidate. `lore state` requires an exact
horizon and returns only bound keys, projected references, used types and last
assignment/clear citations. A clear retains its effect citation without creating
a value or type shape. Ordinary state/context omit these keys and their citations;
raw source inspection retains its existing full-author contract. Bound values do
not enter generic character context/search or imply physical placement behavior.

Species composition uses the existing scoped author concept projection and exact
concept/profile IDs. Callers choose the common/individual profiles, their root
entity-reference fields, and optional root-field mappings for seven descriptive
roles. Ref-shaped ordinary objects, strings and concept links do not supply
inheritance. Common statements and individual variation remain separate; absent
or unselected roles are unknown, while nulls and empty values remain explicit.
Admitted cycles or traversal bounds refuse a partial composition. Affected lists
count only admitted variation concepts explicitly reaching the selected common
concept and fail rather than truncate at their limit. These operations expose
undated concept prose as such. Author composition and affected reads include
native classified assertions and dated sections for their selected owners when
the corresponding capabilities are enabled. Character variants independently
admit the root, linked descriptions, typed endpoints and applicable sections,
then omit ordinary bodies, canonical labels and source metadata. Character
composition accepts optional explicit learned targets without joining them to
canonical subjects or assertions. Affected counts and limits see only admitted
individuals.

Magic composition likewise uses scoped canonical concepts and exact IDs. Four
closed JSON maps explicitly select profile roles, ordered typed root link fields,
and separate restriction/exception fields. A profile has at most one selected
role. Only admitted entity references connect records; literal strings and
ref-shaped ordinary objects remain authored values. Null, empty and unknown
statements retain their distinctions and citations. Cycles, more than 64 records,
depth greater than 16 or more than 1024 visits (including the starting record)
refuse a partial composition.
The author CLI, read-only HTTP route and Lore browser controls preserve the same
selection and now include native assertions and dated sections for traversed
owners when their capabilities are enabled. Character magic is deliberately a
separate literal learned-account operation: it requires one to eight exact
assertion/knowledge targets, accepts no canonical root/profile binding, and keeps
prior learned wording through held rejection until forgetting. Canonical values,
corrections, roles and applicability never replace it. Both author selection and
character learned targets can accompany writing context. Separate magic
requirement capture accepts only that selected family. An author capture requires
complete admitted linkage and permits exact typed field or assertion comparison;
character and dramatic-irony captures use learned accounts and compare as unknown.
The original frame remains pinned even when the effective comparison perspective
is character. No captured report supplied by a caller is trusted or imported.
Expected literals retain raw JSON number tokens: exact decimal comparison equates
`1` with `1.0` while distinguishing adjacent integers above `2^53`; array order and
string case remain significant. Missing, ambiguous or unsupported operands are
unknown. Public complete-result budgets are 256–100,000 characters; native capture
supports 1–1,000,000 subject to its 256 KiB byte/work bounds. Comparison results
never enter draft saves or change readiness. Explicit saved-outcome application
uses its own approval contract. Separate explicit semantic checks can gate that
application only when retained in the reviewed plan; literal magic comparisons
remain independent.

Descriptions use explicit `descriptions-v1` without requiring Lore. Stable local
members retain exact Markdown and inclusive or timeless validity. Owner/member
scope is applied before local correction suppression and result limits. A local
correction must name an earlier section with identical validity and scope.
No horizon retains admitted history across timelines, including replaced members;
an exact horizon suppresses only the admitted replaced sections. Existing body
prose remains independent and is never automatically extracted into dated sections.

Predicate declarations require `lore-core-v1`; assertion records additionally
require `assertions-v1`. Typed values retain their classification and optional
attribution. Explicit replacement links require the same subject, predicate and
validity; alternatives and forks remain distinct. Scope, applicability and live
reference projection precede correction suppression and complete-result limits.
An unavailable root entity value withholds the assertion. Hidden nested values
are pruned from objects/arrays, so a remaining projected object can still supply
an admitted correction. Historical evidence is syntax checked but is not
authenticated or exposed as current evidence; returned citations refer to the
current authored source.

The description and assertion record/member reads use exact IDs, optional exact
horizons and closed author audience/view parameters. Their predicate catalogue
returns current declarations and accepts only a limit. List limits from 1 to 1000 fail
instead of truncating.
Source edits share the existing reviewed writer: all editable kinds accept
description arrays, the world accepts predicate replacement, and assertions
support typed creation and updates. Dated sections enter character reads only
after independent owner/member admission. Linked assertion knowledge follows
its separate learning/rejection/forgetting contract. The family author contexts
join current assertions and dated sections. Specialized character families
reuse admitted sections and explicitly selected learned streams. Minimal reveal
admission of dated prose is implemented separately; richer structured reader
context/retrieval remains outside the minimal reader scope. Reviewed author
publication is implemented through the existing action/edit writer.

Social and environmental family reads require an exact horizon and reuse one
immutable source snapshot. Social selection keeps exact language/sense identity
and literal form matching, including empty text. Explicit typed associations
supply scripts/registers; no culture, religion or language use is inferred from
membership, species or location. Environment selection follows only one hop of
admitted typed links and timed concept links on explicit roots; direct endpoints
supply no extra evidence reads. Self-links and cycles are not recursively folded.

The current `social-*` and `context-*` profile IDs supply roles without profile
versions. Native concept values, assertion classifications/attribution and dated
sections keep their citations. Quantities/date wording remain literal. Explicit
unresolved or absent admitted accounts stay unknown; different literal values
remain competing accounts without truth ranking. Both reads enforce complete
32-record, 512-evidence-item and 262144-byte compact report bounds. CLI, closed
GET and Narrative author controls share the same required horizon and author
labels. Separate character operations require an exact viewer and independently
admitted roots/references, omit canonical labels and source positions, and use
admitted native sections plus optional learned-target streams. Selected beliefs
remain independent of canonical subjects and classifications. They accept no
caller audience/view/scene/known-ID grants or adjustable limits. The same immutable
frame scope is reused when these families are explicitly captured for Writing.
Existing reviewed source edits are reused. Reader presentation exposes the
separate minimal admitted record/section/text projection; it does not execute
structured family composition, matching the pinned reader consumer.

Existence records require `existence-v1` and `generational-core-v1`. Their exact
character IDs, authored episode kinds, explicit intervals and scope labels are
validated in every retained status. Affirmative starts require historical
evidence even when unbounded; bounded ends require separate end evidence.
Unknown episodes have wholly unbounded validity, empty start evidence and no
end evidence. Historical pointers are syntax checked without opening a historical
repository or claiming authentication. Returned evidence is explicitly
`unverified-historical`, beside a current episode source citation. Overlaps and
different kinds remain independent; there is no birth/death or identity inference.

Collective-party presentation requires `collective-parties-v1` with
`generational-core-v1` and `lore-core-v1`. The closed `{kind,id}` Party grammar
accepts exact character, organization or concept endpoints, with no inferred
membership, leader or authority. Typed source entity references keep their
ordinary `{kind:record,id}` form. These helpers do not widen generational claim
claimants, interpersonal relationships, parentage or union endpoints.

Governance selects an exact current profile and all six explicit roles:
territory, party, basis, subjects, obligations and supersedes. Null selects no
field; non-null root fields are unique and must match the role's type. The JSON
decoder rejects duplicate/unknown keys, partial maps and wrong value types.
Unknown profiles and incompatible role bindings return empty policy results.
No field name, profile name or text value supplies inferred role semantics.
Selected role values must retain all their admitted typed references; a hidden
selected endpoint withholds the policy row. Missing roles remain explicitly
unknown, and authored null obligations are separately marked known.

Known jurisdiction follows a direct location or explicit overlay membership.
Inclusive exact horizons filter policy validity and timed membership; history
without a horizon skips timed overlay membership. Independent bases, obligations
and supersession pointers retain their source citations and do not rank competing
authority or suppress another policy. Linked statements reuse native assertion
projection and corrections. Only typed root entity values create reverse
statement links; reference-shaped literal objects do not.

Existence and governance author reads share the CLI/service/closed GET contract
and optional horizon in the Narrative panel. No horizon considers all authored
timelines; `order` or `timeline` alone fails. Scope defaults to `author` and
result limits of 1–1000 are complete or error. Unknown, hidden and wrong-kind
targets have uniform empty shapes without endpoint labels. Author routes have no
viewer or character-mode parameters. Separate character existence reads preserve
an empty unknown result because learned words have no typed episode snapshot.
Character governance requires an admitted exact location/overlay and returns
learned assertion wording with unknown jurisdiction, null basis, no parties and
unassigned applicability. It neither selects profile membership nor joins
canonical role values, and accepts no profile binding. The pinned reader exposes
minimal admitted units rather than typed governance/existence composition.
Authority adjudication and rule execution are deliberately absent in both
implementations.

New scenes/conversations default to planned/draft; their status can be edited
explicitly. All statuses validate their contents. Active scene cursors must
match the declared world cursor, and status edits do not move it. Replacing a
whole map/list can omit nested optional fields; there is no top-level field
deletion command. Scene/conv links need not be reciprocal and do not infer
attendance, location, events or beliefs. Separate
[conversation controls](CONVERSATION_CONTROL.md) construct a coordinated active
conversation with its reciprocal scene link, or append an exact speech/action
beat through the existing Plan path. Earlier speech interruptions are validated
and projected only after target admission. Explicit control times leave clocks
unchanged; no event or belief is inferred.

The explicit `current-time preview` request binds an exact fingerprint and full
canonical-string story point. It generates one existing consequence Plan for
the world and every active scene and validates their complete candidate together.
World typed updates also accept an exact numeric `current_time` mapping. The
preview writes nothing and cannot apply its result; publication uses the existing
separately reviewed outcome workflow. Requested backward or cross-timeline points
remain subject to ordinary scene intervals and source invariants. No events,
knowledge, movement or lifecycle transitions are inferred. See
[CURRENT_TIME.md](CURRENT_TIME.md).

`scene-control preview` adds coordinated lifecycle and movement Plans with exact
record IDs and times. It preserves the pinned independent-event versus scene-bound
movement distinction, inclusive handoffs, carried-object membership and close-only
clock behavior. Complete candidate placement checks require ordinary unbound
entity state; profile/string lookalikes provide no inferred placement. Unlike a
generic consequence preview, an invalid coordinated candidate is refused before
Plan export. See [SCENE_CONTROL.md](SCENE_CONTROL.md).

Topology paths/reachability, location graphs, map catalogues and anchors remain
author/all-time source queries and reject story-horizon parameters. Geometry
reads default to static source and accept an explicit optional horizon.
Canonical open routes and directed location-target portals supply hop
edges. Position-target portals expose their literal destination without inventing
a matching location or travel edge. A missing mode
filter allows any authored mode per edge. Weighted paths sum only one explicitly
selected route metric within one exact unit; absent metrics and portals supply no weighted edge.
With no unit specified, multiple successful units require explicit selection.
Totals are decimal strings and are exact over the parsed YAML number's canonical
decimal spelling; source decimal literals still follow YAML parser precision.
Capability declarations remain explicit; authoring does not enable them or edit
the world automatically.

Geometry supports local-planar and EPSG:4326 maps, with dimensions selected by
map bounds and `z_policy`: forbidden is 2D, required is 3D, and optional-level
accepts either declaration but requires matching placement/query coordinates.
Axis order is east/north locally and longitude/latitude for EPSG:4326; geographic
units are degree. Points, lines and closed polygons share finite coordinates of
magnitude at most 9007199254740991. Decimal geometry calculations use f64. Bbox
queries compare shape bounds using intersects/within; nearby queries use actual
shape distance including filled interiors. Distances in geographic maps remain
Euclidean coordinate degrees, not geodesic distances. Results are inclusive,
ID-ordered and map-scoped. Map origin/scale and exact anchor correspondences are
retained as authored data; conversion prose does not define an executable
transform or route cost. The browser shows structured results, not a drawn map.

Locations and routes may declare ordered `spatial_history` arrays after explicit
`spatial-history-v1` opt-in. Closed members have a local ID, aspect, timeless or
inclusive interval validity, a typed value and optional earlier replacement ID.
Placement/geometry target one complete map/shape value; parentage can explicitly
select a null root. Availability and each route metric are independent fields.
Corrections retain the exact source validity, aspect and metric target; unrelated
overlap and correction forks are refused. Containment must remain acyclic at
all interval boundaries, including the instant after an inclusive end.

Author history reads retain every member without a horizon and suppress replaced
members at an explicit horizon. Route inspection and bbox/nearby can apply the
same exact history; geometry moves are considered before map filters and complete
result limits. Static fields remain the fallback outside applicable history;
missing values stay absent. Source and correction-chain citations identify the
winning values. Optional `order`/`timeline` require `tick`; integer time values
remain decimal strings. Ordinary paths continue to use static route declarations.
The existing one-record writer replaces the complete history array or clears it
with `[]`. Author reads and edits do not themselves grant character evidence.
Separate character spatial-history reads require an admitted canonical owner
and admitted placement-map or parent endpoints, apply exact validity and remove
only admitted predecessors. They return stable member identities without source
indexes or citations. Captured writing frames derive their own scene-local scope;
the direct spatial-history selector accepts no scene grant. Binding a private
work adds no spatial grants. Reader presentation does not execute structured
spatial queries; the pinned reader has the same minimal admitted-unit boundary.

Overlay reads are distinct author queries at an exact story horizon. Static
memberships remain applicable at every declared timeline; time-bounded overlays
use explicit same-timeline inclusive start/end coordinates. Audience and view
filters match literal authored labels before membership, horizon, count and
limit checks. They do not authenticate a user, imply character knowledge or
provide learned evidence. Exact explicit IDs unavailable in the selected scope
produce the same NotFound error as missing IDs. An authorized but out-of-time
or out-of-location overlay returns an ordinary empty result. Overlay transport
retains the author fingerprint/revision and decimal-string time coordinates.
All three new source kinds use the shared reviewed single-record writer.

Chronology reads are author/all-time and cannot disclose dates through character
views. Exact IDs select calendars/eras; integer strings preserve signed i64
coordinates. Independent timeline anchors require exact day/tick/order matches,
with no tick-to-day rate or interpolation. Interval queries list only declared
anchors, retaining none/unique/ambiguous outcomes across selected timelines.
Source annotations and query operands support era inputs and inclusive partial
dates. Only explicitly complete days convert. Finite approximations participate
only in overlap searches and never establish anchors; conflicts, relative
references and durations remain noncomparable. Isolated calendars compare only
within their own basis. Era-bound filters test each annotation independently
against the era and date predicate. Queries read source directly, retain stable
ID ordering and fail when complete results exceed the limit. Declared operational
objects cannot own nested chronology; arbitrary state/metric/condition names and
opaque metadata retain their meaning. New domains must extend their typed schema
checks as they land. See `CHRONOLOGY.md` for source bounds, unavailable results
and arithmetic limits.

Parentage and union queries are author reads at an explicit inclusive story
horizon. Canonical asserted/confirmed parentage supplies graph edges; canonical
declared/formed unions supply current memberships. Future transitions and other
timelines contribute no current relationships. Draft/retired records contribute
no graph edges or memberships, and their history query returns unavailable.
Union membership does not infer parentage, marriage labels or kinship labels.
Ancestry and descendant queries retain one cited shortest path per reachable
character; path search follows parentage in either direction with deterministic
ties. Depth/hop limits default to 8, accept 0–32, and refuse incomplete traversal.
Result limits default to 100, accept 1–1000, and refuse truncation.

Organization queries follow explicit parent links and current affiliations.
Former roles appear only with `include_former=true`; affiliation never follows
from a person's location or family. The hierarchy reflects visible reparenting,
retains dormant organizations and stops at dissolved ancestors. Organization
legacy lists include visible active, dormant and dissolved linked legacies.

Legacy queries keep claims separate from tenure holders, preserve legal and
de-facto tenures, and return explicit succession links without appointing a
successor. Tenure vacancy intervals include their final tick/order and become
expired afterward. Terminal tenure and claim rows remain available as authored
history. Vital queries exclude unknown/withheld and uninitialized records before
budgets, return an explicit unknown result without affirmative evidence, and
preserve all known histories. Conflicting current vital states remain ambiguous;
no winner, age or elapsed duration is inferred. Direct history reads of
unknown/withheld vital records are unavailable.

The CLI exposes `generational family`, `ancestors`, `descendants`, `path`,
`history`, `organization`, `legacy`, `organization-legacies` and `vital`;
matching GET routes live under `/api/generational/`. All require `tick`, accept
optional `order` and `timeline`, and use ordinary author ID/title/alias resolution.
`history` covers all eight generational kinds. The browser's Generations tab
shares the existing story horizon, offers an explicit author/character viewpoint
and disables irrelevant query fields.
Typed requests for these kinds support their literal fields, initialization,
complete transition-list replacement and canonical/draft/retired status.
They require declared `generational-core-v1`; authoring does not enable it
automatically. Reciprocal claim changes can use explicit operations in one
reviewed saved outcome, with complete candidate validation. Reciprocal changes
are not generated automatically.

The optional `generational-knowledge-v1` capability requires
`generational-core-v1`. A knowledge record may carry one closed
`claim.genealogy` assertion for parentage, union, organization, affiliation,
tenure, claim or vital status. Ordinary knowledge transitions supply learning
time and belief state. Source validation checks the literal payload, endpoint
kinds, inclusive validity, bounded learned labels and evidence eligibility.
Typed knowledge create/update requests preserve this assertion through the
existing preview and atomic single-record save workflow.

Character reads use `generational learned VIEWER OPERATION` and
`/api/generational/learned/OPERATION`. The exact viewer ID is required; only that
viewer's canonical knowledge learned by the horizon is considered. Accepted,
suspected, uncertain and remembered assertions contribute; rejected and forgotten
assertions do not. Relationship queries additionally filter the assertion's
inclusive validity interval. `assertions --held-history` and endpoint `history`
retain held assertions outside that interval. Contradictory or uncertain claims
remain separate; no canonical truth lookup resolves them. Learned paths use
admitted parentage only. Union assertions do not add graph edges, and learned
legacy reads do not infer succession.

Author and character viewpoints also expose `labels`, `discover`, `search` and
`context`. Character names come exclusively from explicit learned labels attached
to held assertions, including assertions outside their validity interval.
Endpoint selectors accept exact IDs or an unambiguous learned label; canonical
titles and aliases never provide a character fallback. The viewer itself always
requires an exact character ID. Discovery accepts character, organization,
legacy and event endpoint kinds and an optional prefix of at most 64 characters.
Search accepts one to eight word prefixes of at most 32 characters each and
does not inspect character-unadmitted prose or outer claim statements.
Author search examines visible literal fields and transitions, excluding source
prose and record header titles. Generational context selects a character subject.

The existing depth and result bounds apply; context additionally accepts a
complete-rendering budget of 80–1,000,000 characters, default 12,000. Results
fail if complete traversal, results or context would exceed a requested bound.
Character successes and failures omit global fingerprint/revision, source paths
and source blobs. Canonical source mutations that do not change admitted
evidence leave character transport results unchanged. The original nine author
queries retain canonical semantics when learned assertions are present.
Authored context briefs and exact captured writing frames use the independent
composition contract described above. Separate existence/governance
character reads preserve explicit unknown/unassigned semantics as described
above; they do not expose a canonical typed episode or infer policy applicability.

Verified execution is Linux with Rust 1.99.1. Windows/macOS execution and
performance comparisons have not been verified. Frozen core regression values
retain the original Python provenance but do not constrain future schema design.
