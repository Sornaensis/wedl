# Correctness and scope review

Reviewed 4 November 2026 and extended 5 November 2026 against
[`f0e885ccebc279eff65a5db2984d01ebc6647b2b`](https://github.com/Sornaensis/wedl/tree/f0e885ccebc279eff65a5db2984d01ebc6647b2b).
The integrated feature pass reviews the completed author helpers and pinned
callable inventory. It adds missing selected-scene Writing facts and automatic
browser-frame restoration, and corrects a startup selection race. The subsequent
[anomaly investigation](ANOMALIES.md) identifies the exact older archive behind
the preserved build failure and a reproducible propagation mechanism, while
leaving the historical writer unattributed. See
[PROGRESS.md](PROGRESS.md) for measured verification and the estimate below for
the remaining diagnostic work.

## Integrated feature review

The retained upstream checkout was rehashed against its recorded Git blob IDs,
SHA-256 digests and sizes: all 883 regular blobs matched. This cycle did not
refetch upstream. Fresh inspection of the pinned dispatch and mounted routes
counted 124 CLI leaves and 109 mounted command contracts, plus the inspected
spatial-explorer, Writing, reader and client-family consumers. The nine author helper families
are covered by the native schema/context/scaffold operations below. This is a
bounded callable-scope review, not an execution of the complete Python suite or
proof of every upstream branch.

Two concrete Writing omissions were found and supplied:

- Pinned `context.py` includes present object/environment details and the
  viewer's own outgoing relationship state. The new native
  [literal present component](WRITING_FRAMES.md#literal-present-facts) follows
  existing canonical replay, explicit scene placement and character admission.
  It retains exact typed values without canonical titles, causal prose, source
  indexes or global metadata. A private canonical cause may change an admitted
  observable fact; it does not become evidence. Ordinary character reads stay
  unchanged. Independent review corrected candidate-versus-output row accounting
  and historical set-operation accounting before integration. More than 10,000
  redundant effects cannot disclose a withheld final value or inflate a small
  final set's detail count.
- Pinned `docs/guides/writing-frames.md` and
  `src/wedl/static/writing_frames.mjs` require automatic Back/Forward/reload
  restoration and source-refresh revalidation. The browser now stores only the
  exact accepted frame in its own history key, restores original pins and uses
  native resolve before dependent actions. Malformed/stale frames remain
  unusable; edits and newer requests fence late responses. An unchanged accepted
  active frame is revalidated after refresh without repinning or a new entry.

The browser review also reproduced a late initial status response overwriting
an already edited or used horizon. Startup defaults now apply only to untouched
selection groups, including the retcon queue revision. Real browser regressions
cover all 15 horizon groups, use without editing, partial edits, unsent Plans,
private pins and normal untouched initialization. The expanded scene-context
browser check also reproduced mobile overflow from a long unbroken authored
world title. The heading now wraps within its container; JSON output was already
bounded to the page.

The pinned reader consumer exposes minimal record/section/text units, not a
typed family/spatial query engine. Its governance runtime explicitly excludes
authority adjudication; learned governance and existence retain their native
unknown boundaries. Only the human action resolver executes. Historical
citation syntax covers record/section/transition references, whose confirmed
saved-outcome consumer already authenticates original evidence. The earlier
vague advanced-domain backlog is now recorded as explicit
[native boundaries](COMPATIBILITY.md#native-boundaries), with no additional
named implemented target established by this review.

## Previous contextual authoring pass

[Authoring context and starters](AUTHORING_REFERENCE.md#story-choices) consolidate
the nine upstream helper families into shared native reads. Context uses one
validated source capture and exact native brief-member enumeration. It retains
current time, timelines, profiles, predicates, bindings, state constraints and
metric declarations with explicit presence and lossless YAML scalar types.
Complete row/byte limits refuse truncated metadata.

Scaffolds share the existing editor field metadata, exact source fingerprint and
native Plan preview. Every returned existing-record starter must prove a
byte-identical no-op; unknown/protected fields and exact Markdown body remain
retained. Description/history members retain absent-versus-empty distinctions
and receive a fresh local ID. Organization creation uses explicit ID/title and
native current time. Family owner and capability gates remain enforced.

CLI/HTTP/browser share those operations. The browser keeps its unsent Plan until
explicit replacement and rejects responses with inconsistent pins, incomplete
row counts, wrong family modes or invalid member owners. Request epochs prevent
old completions from restoring stale output or enabling newer pending reads.
These helpers do not publish, compile or create caches. Full scaffolds refuse
source values that cannot round-trip the current typed patch codec.

## Previous static authoring pass

[Authoring reference](AUTHORING_REFERENCE.md) supplies a self-contained native
Plan JSON Schema, all 29 editable kind entries, literal creation defaults and
native transition choices. Metadata is shared with editor and history validation.
The CLI runs without a repository; the shared author GET route omits source pins
and remains available when a running session's source becomes unavailable.
The browser reads, selects and downloads the same reference without touching an
unsent Plan. Its independent epoch prevents older responses from restoring a
departed result or clearing a newer pending read.

The schema describes closed JSON shapes, optional/non-null distinctions and
exact canonical signed ordinal strings. UTF-8/aggregate byte bounds, duplicate
keys, retained YAML and JSON operands, native ID syntax and world-dependent
validation remain explicit native checks. Schema acceptance never approves or
publishes a change.

[Completion](COMPLETION.md) derives its entire command/option/fixed-choice tree
from Clap. It tracks option values and literal positionals without invoking WEDL
or completing authored paths. Both emitted shells receive runtime verification;
PowerShell's no-match fallback is handled through a documented public-API no-op
completion rather than filesystem suggestions.

## Previous integrated correctness pass

- Character catalogue search previously described observations, speech, actions
  and recollections as description sections that did not exist. It now cites
  the owner record and original source field, retaining array indexes before
  admission or sorting. Actual descriptions retain section targets and learned
  beliefs retain learned citations. Internal source-index metadata does not
  enter ordinary character context, transcript or search JSON. The new
  [citation regressions](../tests/integrated_admission_review.rs) include hidden
  preceding members and a real description-section control.
- The ordinary `--revision` source loader previously inherited Git trace and
  configuration variables and allowed lazy fetching. An isolated successful
  read appended 477 trace bytes to a new sentinel; another missing-blob read
  fetched from a local bare remote and created four pack files. The loader now
  matches the historical reader's environment isolation and disables lazy fetch,
  prompts, replacement objects and optional locks. Three
  [subprocess regressions](../tests/source_git_environment.rs) failed before the
  fix and now cover successful/refused reads, live source and external trace
  destinations, unrelated injected configuration, and missing objects without
  fetch or object-store writes. These findings do not explain older files
  returning to their exact previous bytes and timestamps.
- Status and capability warnings no longer describe implemented consequence,
  revision, Writing, manuscript or broad reader workflows as planned. Overall
  validation remains `complete:false`; full domain conformance is not certified.

Independent cross-review found no additional issue in either fix. The bounded
admission review also checked complete cursor bindings, all-time structured
evidence, scope before ranking/budgets and original action-evidence capture.

## Previous saved-action pass

[Outcome review](OUTCOMES.md#original-evidence) resolves draft and selected
proposal citations in their original Git worlds. Exact record/member, path and
blob checks bind complete raw-owner digests into ordered evidence, the review
and applied receipt. Original captures share complete bounds and require valid
native worlds; live files and Git replacement refs cannot repair missing or
mismatched originals. First application repeats the checks under the writer lock.
Terminal replay and verification retain the approval-time evidence digest while
keeping their existing exact-source-publication scope.

[Private hypothesis links](ACTIONS.md) retain ordered unique record references
as private reasoning. Current saves/reviews require actual hypotheses; structural
historical reads preserve dangling links. Changes invalidate readiness bindings
and obey existing proposed/stale/terminal rules. Linking creates no canonical
facts, context admission or character knowledge. Browser import/edit/save/review
supports the field and checks the complete evidence proof before approval.

## Previous catalogue pass

[Catalogue](CATALOGUE.md) applies author all-time/as-of or exact character/scene
admission before labels, filters, search, facets, totals and pages. Requested
unavailable labels disclose only their requested identity. Learned assertion
cards use the remembered statement without author headings, aliases or prose.
Typed profile and era metadata retain their independently admitted meaning.

NFC/casefold ordering keeps canonical Unicode spellings equivalent. Prominence
reuses navigation's complete admitted cohort without requiring a journey
response. Continuation binds exact source, lens, horizon, filters, sorting,
page size, search provenance and the complete selected result. The browser
clears stale results/cursors on selector, horizon, tab or source changes.

Review caught two all-time boundaries: interval-bearing typed content must stay
on the selected timeline, and finite observations/environment conditions must
remain searchable after their historical intervals end. All-time collection
now handles those intervals explicitly rather than substituting a far-future
dated read. Search stays complete within native corpus bounds.

Additional admission checks keep hidden effect text outside search allocation
and budgets, and withhold unfinished scene/conversation aggregate prose even in
all-time search. Learned belief cards retain only their learned statement and
evidence while supporting explicitly selected canonical thread memberships.

## Previous navigation pass

[Navigation](NAVIGATION.md) adds exact author whereabouts and full-cohort
prominence, authored timeline points/spans, causal directions/advisories,
story-point trails and explicit scene conditions, pair event interactions, and
selected thread membership. Named links and contributing evidence are admitted
before projection. Result overflow is refused without truncation.

Review additionally closes the operative location-profile consumer and applies
scoped state/condition admission in story-point navigation. Hidden evidence must
not determine an admitted point's eligibility or leak through its explanation.
The browser retains exact coordinates, invalidates delayed reads after selector
changes, and prevents late startup data from replacing an edited or used horizon.


## Previous conversation and boundary pass

- General record/Plan YAML now receives a streaming expansion preflight before
  constructing an expanded value. The old raw-byte limit alone allowed a small
  alias-bearing request to allocate many copies of a large scalar before typed
  validation. Expanded UTF-8 strings, including keys and alias copies, are
  bounded to 1 MiB; values/keys to 131,072; depth to 128. Ordinary bounded aliases
  remain accepted. Retained-edit limits are unchanged.
- Writing-frame imports now reject malformed UTF-8. Previously, the browser's
  replacement decoding could change an invalid timeline byte into a different,
  valid declared timeline. A browser reproduction confirmed that substitution;
  the corrected decoder rejects it before issuing a resolve request.
- [Conversation controls](CONVERSATION_CONTROL.md) create an active conversation
  with its reciprocal scene link or append a speech/action beat through one
  existing Plan. They validate exact presence and times, retain previous
  structured values and Markdown body, support earlier-speech interruption, and leave clocks unchanged under explicit
  coordinates. Character projection withholds hidden interruption targets.
- Browser/help documentation now describes current-source capture and guarded
  publication. The old fixed-snapshot/restart claim was stale.

The conversation/navigation passes introduced no new dependency, writer, version
or approval mechanism. Catalogue adds `unicode-normalization` and its `tinyvec`
dependency for NFC name collation. All source/protocol labels remain v1 and the
Cargo package remains 1.0.0.

## Scope method

The review compared the pinned top-level CLI dispatch, public HTTP operation
families, fixed authoring dispatcher and the relevant implementations/tests.
The earlier review freshly fetched the exact commit and rehashed each of 883
extracted source/documentation blobs against its pinned Git tree entry. This
cycle reused that retained checkout and reverified every blob. The callable
inventory contains 124 CLI leaves and 109 mounted command contracts, alongside
spatial-explorer, Writing and reader routes. Native inventory has 155 shared
operations and 58 top-level CLI families at that review; these are differently grouped
interfaces, not comparable completion percentages. This is a bounded interface
and implementation audit, not a proof of every optional domain or a rerun of
the Python suite.

The principal inventories are `src/wedl/cli.py` (dispatch),
`command_parser.py` (CLI shapes), `api_contract.py` and `api_router.py`
(public routes), and `authoring.py` (coordinated author intents).
The complete author dispatcher additionally delegates to
`generational_authoring.py`; all eight named generational intent variants compile
literal record creation or explicit history/knowledge replacement. Existing
typed editors and atomic Plans cover those outcomes. Conversation create/append
were the remaining coordinated intents and were supplied in the preceding pass.

## Callable author helper inventory

The pinned parser and dispatcher implement `completion bash` and
`completion powershell` (`command_parser.py:223–236`, `cli.py:252–255`,
`completion.py:232–244`). Direct dispatch produced both shell registration
scripts; that upstream check verified generation, not execution inside both
shells. The current native [completion](COMPLETION.md) now covers commands,
options and fixed choices without reading authored records.

Nine schema and nine scaffold commands are also callable through HTTP. Their
actual outputs distinguish static request schemas from contextual discovery:

| Family | Guidance and starter behavior | Pinned implementation |
| --- | --- | --- |
| Changeset | Concrete request/operation schemas; optional state-key, reference and metric context; valid existing-record starter | `changeset.py:526–586`, `consequence_schemas.py:321–382` |
| Generational | Kind/intent/field/transition catalogue; current-time organization starter | `generational_authoring.py:382–410` |
| Lore | Profiles, predicates, state bindings and record choices; retained existing-record edit | `lore_authoring.py:69–93` |
| Briefs | Timelines, owners and exact section/chronology/transition choices; existing brief edit | `brief_authoring.py:58–100` |
| Assertions | Predicates, timelines and record choices; existing assertion/knowledge/world edit | `assertion_authoring.py:55–70` |
| Descriptions | Timelines and non-hypothesis owners; retained source/sections and next member ID | `description_authoring.py:76–81,122–135` |
| Spatial history | Timelines and location/route owners; retained source/history and next member ID | `spatial_history_authoring.py:76–81,130–143` |
| Existence | Validated lore-style discovery; existing episode edit | `existence_authoring.py:54–64` |
| Governance | Validated lore-style discovery; existing world/concept/assertion edit | `governance_authoring.py:53–63` |

This is one concrete JSON-schema provider, one static variant catalogue and
seven contextual metadata providers, plus their starters. The native static
Plan/schema and kind/field/default/transition reference, validated contextual
metadata and general editable starters are now implemented. Shared native
operations consolidate the upstream interfaces while preserving their concrete
owner, source and preview requirements.

Native interfaces may consolidate the families. Guidance must match current
decoders; discovery must use one validated author source; starters must retain
exact pins, body bytes and absent/empty distinctions and pass existing preview
validation. Reject inappropriate owners and incomplete results. Generating a
starter performs no publication, compile or cache write. Reuse the existing
CLI/HTTP/browser and reviewed writer; no new version or migration is needed.

## Confirmed navigation behavior now implemented

All pinned paths below are beneath `src/wedl/`. They name actual callable
behavior, rather than speculative features or compatibility machinery.

| Capability | Pinned evidence | Current Rust implementation |
| --- | --- | --- |
| Character whereabouts | `query.py:1029–1248`, prominence `540–655` | Explicit initial/set/clear/reaffirmation journeys, last-known location, active-scene/offstage/unlocated groups and prominence over the complete admitted canonical/retired cohort before selection. |
| Timeline points and spans | `query.py:726–810`; HTTP `api_contract.py:415` | All-status authored events, story-point lifecycle points and scene/conversation spans with exact ordinal metadata and independently admitted named links. |
| Causal directions and advisories | `query.py:1316–1409`; CLI `command_parser.py:452–458` | Upstream/downstream/both traversal, plot/outcome asymmetry and cross-location cause advisories, with admission before graph edges and focus-name resolution. |
| Story-point trail and scene context | `query.py:1258–1313`, `semantics.py:246–283` | Dated transition/causing-event trails and declared outcomes. An explicit native scene condition uses selected scene context without filtering the candidate list. |
| Pair interactions | `semantics.py:287–304`; CLI `cli.py:480` | Canonical events explicitly containing both named participants, retaining authored roles; co-presence alone is insufficient. |
| Selected thread memberships | `query.py:164–187`, `thread_filter.py:84–143`; HTTP `api_contract.py:435` | Requested-record intersections with only selected threads; unavailable/hypothesis records and unselected memberships are omitted. |

## Completed saved-action behavior

| Capability | Pinned evidence | Current implementation |
| --- | --- | --- |
| Saved outcome historical evidence | `action_resolution.py:115–125,208` | Resolve original revision, owner/member, path and blob during saved outcome review; bind ordered whole-owner source digests into approval and receipt. |
| Private action hypothesis links | `action_drafts.py:138–169`, `author_data.py:44` | Retain validated unique existing noncanonical references as private reasoning links without facts or character grants. |

Catalogue page/labels from `core_catalogue.py:84–146,239–265,308–374,501–597`
and HTTP `api_contract.py:402–403` were implemented in the preceding pass. Native current
source fingerprints replace the pinned implementation's current-HEAD selector;
profiles remain current native IDs and search uses existing native retrieval.
This integrated pass found no further named domain-semantic gap in those slices;
the contextual author-tooling helpers above are now implemented.
The completed navigation checks cover exact coordinates, same-tick boundaries,
future/hidden mutations, complete-bound refusal and unchanged source.

## Deliberate native boundaries

Explicit stable IDs/full times, current profile IDs, source-backed complete reads,
native current-source fingerprints/receipts, separate hypothesis scores and
separate Writing packets versus optional ranked search remain accepted native
choices. They do not excuse the derived reads above.

Only human action resolvers execute in the pinned implementation
(`action_resolution.py:94–95`); declared external adapter syntax does not establish
missing executable resolver work. The pinned Writing frame has no occurrence
selector. Existing reader presentation implements the inspected public and
author-simulation operations; a general coordinate-transform or broader domain
reader engine was not established as additional work.

Optional source fields explicitly outside typed requests, including conversation
`topics`, retain their documented direct-source boundary. The pinned editable
conversation schema recognizes topics; no separate topic-specific read was
found in its conversation response or reader. No migrations, format adapters,
release preparation or CI/CD work is included.

## Restoration observations

The later forensic work in [ANOMALIES.md](ANOMALIES.md) separates the build
archive's established old-source identity and Cargo reuse from its unknown
producer. It also records the source-writer audit, limited native observation,
explicit restoration control, coherent cross-executor control and kernel tracing
restriction. No common cause for the two observation families is established.

Earlier reviews recorded unrelated source/private-file restoration. The latest
recorded restored work retained its original hash and nanosecond mtime but had
a different inode. The earlier frozen single-record CLI failure recorded an
unrelated location returning to older bytes without a browser or rebuild.
The original failing artifact directories are absent from the current working
environment, so this pass cannot independently reinspect those traces. Retained
review notes do not identify the responsible writer.

Inspected historical paths read Git objects and contain no checkout/reset or
worktree restoration. Explicit recovery journal targets also exclude private
manuscript work files. The earlier browser harness's readiness check accepted any
successful response on its port, which weakens attribution but does not explain
the frozen CLI observation. Retained failures lack a complete attributable
process/syscall trace. Clean traces belong to different executions.

One isolated overlapping-executor probe kept its deliberate edit/deletion
through all observers. That negative probe neither reproduces the failure nor
establishes an external cause. No speculative production workaround was added.
Source fingerprint drift detection and subsequent refusal are supported;
unqualified preservation/durability in every execution is not established.
Further diagnosis needs the failing execution's responsible writer, not another
clean rerun.

## Remaining estimate

| Work | Comparable iterations |
| --- | ---: |
| Further named feature iterations against the inspected pinned inventory | 0 |
| Earlier source-restoration and compiled-artifact mismatch diagnosis | Unbounded |

The confirmed feature inventory and its integrated review are complete for this
estimate. This does not establish exhaustive upstream conformance or diagnose
the earlier observations. New concrete findings can reopen feature work; the
unresolved writer/artifact mechanism cannot be assigned a defensible iteration
count from clean executions alone.
