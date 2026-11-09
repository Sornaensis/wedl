# WEDL Rust plan

Updated 5 November 2026. This plan describes the native v1 implementation and
open correctness observations.

## Direction

Build a small Rust story/world tool around Markdown source, exact ordinal time,
disposable SQLite search, and one shared library behind CLI and a local browser.
Target functional parity with the implemented behavior in the pinned Python
project. Exact source/wire compatibility and matching internal architecture are
not goals. [PARITY.md](PARITY.md) defines the comparison and evidence.

**Keep everything on v1 until this entire plan is complete.** These labels stay
fixed throughout implementation:

| Surface | Fixed label |
| --- | --- |
| Cargo package | `1.0.0` |
| Source schema | `wedl/v1` |
| Private manuscript schema | `wedl-author/v1` |
| JSON API | `wedl-rust/v1` |
| SQLite cache | `wedl-rust-fts/v1` |
| Source fingerprint | `source-v1` |
| Saved retcon witness | `wedl-retcon-witness/v1` |
| Reader presentation | `wedl-reader-presentation/v1` |
| Trusted operator policy file | `wedl-reader-policy/v1` |

These v1 labels do not certify exhaustive upstream conformance or stable
contracts. Breaking source, API and cache changes are allowed within these
labels. Update the implementation, specifications and fixtures directly;
rebuild disposable caches when needed. Do not create separate versions for
increments, or add migrations, version adapters, legacy field aliases or dual
readers.

Keep one Cargo package. Add a small module when it owns actual behavior; avoid
plugin registries, generic workflow engines, background services, and new crates
without a concrete need. Tests should cover rules and failure cases, not mirror
implementation details.

## Current architecture

| Module | Responsibility |
| --- | --- |
| `source`, `model` | Current-schema records, exact source bytes, snapshots, Git reads, world creation |
| `state_schema`, `runtime` | Typed state and deterministic temporal queries |
| `story` | Story-point conditions/lifecycle, explicit causes, timed environments |
| `scene` | Inclusive participation, scene overlap, dated observations and conversation audibility |
| `character` | Admitted evidence for context, transcript and in-memory search |
| `navigation` | Author whereabouts/prominence, timeline points/spans, causal graphs, story trails, pair interactions and selected memberships |
| `spatial`, `spatial_extra` | Explicit containment, routes/portals, bounded hop/weighted/reachability queries, exact metric totals and scoped overlay membership |
| `geometry` | Local/geographic 2D/3D maps, point/line/polygon placements, literal anchors and complete bbox/nearby reads |
| `spatial_history` | Exact authored placement/parentage/route histories, correction chains, containment boundaries and borrowed effective values |
| `chronology` | Exact cycle/table calendar days, partial civil/era formatting, exact conversion and explicit interval-to-story anchors |
| `chronology::annotation` | Validated date claims, inclusive intervals, uncertainty, author lists, stored-claim relations and typed date predicates |
| `generational` | All eight canonical history kinds, exact author horizons, family/organization graphs, holdings/claims and disclosed vital evidence |
| `generational::knowledge`, `generational::discovery` | Separately learned assertions, exact evidence admission, names, discovery/search and bounded author/character context |
| `lore_types`, `lore` | Closed typed profiles/values, exact source references, scoped author concepts and explicit links |
| `lore_state` | Explicit state profile bindings, whole-object state validation and scoped replay with exact provenance |
| `species` | Separate author/character common and individual composition, affected discovery and native dated evidence |
| `magic` | Author role composition with native dated evidence, and independently learned character explanations |
| `description` | Explicit dated Markdown sections, owner/member scope, local corrections and cited author history |
| `assertion` | Current typed predicates, classified propositions, attribution, replacement graphs and scoped author reads |
| `learned_assertions` | Closed linked claims, exact acquisition/held-state admission and literal character words independent of canon |
| `narrative` | Shared acquired-record scope, character sections/lore/spatial history, and safe existence/governance adapters |
| `briefs` | Authored context briefs, exact required references and complete admitted transitive closures |
| `writing_frames` | Paired-source work and explicit scene/POV/time selections, admitted choices and separate character/author context |
| `writing_frames::present` | Physically present object state, active scene/location environments and the viewer's own directed relationships without causal prose or source metadata |
| `current_time` | Explicit world/all-active-scene cursor plans and complete consequence previews |
| `scene_control` | Coordinated active scene lifecycle, inclusive attendance handoffs, listed carried-object continuity and explicit movement-event Plans |
| `action_drafts`, `action_store`, `action_requirements` | Private intention/questions, immutable human proposals, exact review, guarded private writes, saved previews and separate cited literal magic comparisons |
| `consequences` | Bounded explicit typed plans, complete in-memory candidates, exact source/diffs and final validation |
| `historical` | Literal verified commit/tree/blob reads, bounded historical captures and closed inert citation syntax |
| `edit_drafts` | Retained patch/body buffers, separate save/base pins, exact saved review and terminal target/receipt publication |
| `revision_review` | Three original admitted worlds, exact historical citations and bounded source/state/scene conflicts |
| `manuscript` | Exact private segment/work/reveal-plan source, independent fingerprints, ordered occurrences, bounded original reads and guarded single-document saves |
| `reveal` | Pure cumulative occurrence admission after ordinary scope, audience, perspective, story-time and correction filtering |
| `reader`, `reader_config` | Explicit installed public/simulation policies, pinned original sources, admitted-only search/pagination, revocation and isolated reader transport |
| `retcon_impact` | Admitted typed reverse occurrence graphs, original/candidate citations and exact input witnesses |
| `retcon_reviews` | Immutable original analysis, private decisions/CAS and separate live plus original first-parent currentness |
| `event_consequences` | Exact-time contribution exclusion, delayed histories/current horizon and grouped source/semantic changes |
| `action_outcomes`, `publication` | Separate outcome review/approval, terminal applied receipts, exact-source verification and recoverable publication with reader fencing |
| `social`, `environment`, `family_context` | Exact author/character family contexts, projected typed links, explicit belief targets and independently admitted evidence |
| `existence` | Independent authored episodes, interval/evidence validation and shared canonical/character-endpoint admission |
| `governance` | Six explicitly bound typed policy roles, direct jurisdiction, linked assertions and collective Party projections |
| `validation` | Source consistency and implemented domain checks |
| `compiler` | Fingerprinted all-time SQLite FTS plus admitted dated prose/core fields and post-ranked thread grouping |
| `retrieval` | Bounded local TF-IDF/LSA, independent lexical/semantic rankings and reciprocal-rank fusion over caller-admitted text |
| `record_edit`, `authoring` | Typed record requests, validated previews and atomic single-record saves |
| `authoring_schema`, `completion` | Repository-free native Plan/field guidance and shell completion generated from the CLI parser |
| `authoring_tools` | Complete validated-source choices and editable native Plans sharing editor metadata and existing preview |
| `scene_control`, `conversation_control` | Explicit coordinated author requests producing ordinary consequence Plans |
| `service`, `main`, `http`, `static` | Shared operations, CLI, fresh local-author reads, guarded publication and separate reader transport |

## Correctness rules

- Canonical data is UTF-8 Markdown/YAML. Reject duplicate keys/IDs, unsupported
  schemas, inconsistent parsed/raw values, and source symlinks. Keep prose bytes
  intact unless the author replaces them.
- Story time is a declared timeline plus i64 tick and i32 order. Endpoints are
  inclusive; different timelines are not comparable. Public integer state and
  time values use decimal strings so browsers preserve their precision.
- Calendar days have their own authored axis. Convert only through declared
  epochs; match story anchors exactly, including order. Never interpolate using
  tick gaps. Cycle math handles negative years, table gaps stay unavailable and
  different timelines retain independent anchors.
- Record date annotations preserve precision and uncertainty. Compare only on
  a shared basis; conflicts, relative references and durations do not supply
  dates. Finite approximations can establish overlap only. Chronology source
  edits must not change character evidence responses or temporal replay.
- Date searches use inclusive intervals and preserve uncertainty. Approximate
  claims participate only in overlap searches; no approximate/conflicting input
  supplies an exact story anchor. Era-bound filtering is an independent overlap
  condition. Complete result limits fail instead of silently truncating.
- Replay authored canonical effects in deterministic order. State values obey
  their declared types. Causes, membership, knowledge and relationships remain
  explicit; do not infer facts from prose.
- Family histories fold only visible canonical entries at exact inclusive
  horizons. Union membership follows its historical payload and never creates
  parentage or character knowledge. Corrections retain their source evidence;
  lineage/path bounds fail when they cannot provide a complete answer.
- Tenure transfers clear only the source holder; a claim never appoints a
  holder. Inclusive vacancy expires without reopening. Vital reads exclude
  unknown/withheld evidence and retain conflicting known states explicitly.
  Typed historical references never expose an uninitialized generational target.
- A story point's stored lifecycle and current eligibility are separate. Reading
  an eligible point never activates it or creates events. Conditions and
  dependencies are validated, and dependency cycles are rejected.
- Source browsing and default search are author all-time operations. Explicit
  dated author search applies prose/section/core-member gates before training and
  bounds; selected threads group complete ranked candidates without changing
  their scores. Character privacy is not implied by an author horizon or packet.
- Character operations are separate, exact-ID reads over explicitly admitted
  dated evidence. Admission precedes serialization, search, ranking, counts and
  budget checks. Valid private/future changes that preserve admitted evidence
  must leave complete character responses unchanged. A selected-scene Writing
  frame additionally exposes admitted observable state even when its causal
  event is private; the cause itself remains withheld. Do not include global
  source metadata or untimed prose.
- Typed lore values obey the world's current profile declarations. Only declared
  entity leaves supply references; strings and enum values remain literal.
  Profile changes validate every dependent concept, initial state, event effect
  and state predicate, for every valid source status. Concept visibility precedes
  resolution, references, counts and limits.
  Used type shapes preserve scalar meaning without exposing unused alternatives.
  Ordinary non-concept endpoints retain the author source boundary. Existence
  episodes additionally require canonical status, applicable validity, their
  own labels and an independently admitted character endpoint. Explicit
  links, profiles and concepts do not supply state effects or character learning.
- A brief retains authored prose and exact required record/member pins. Admit
  the whole transitive closure before result limits; an unavailable required pin
  withholds the whole brief. Cycles terminate through stable exact-reference
  deduplication. Pins do not teach their targets. Brief owners independently pass
  canonical status, scope and validity, including when referenced by typed lore
  or when supplying dated sections.
  Full-author composition uses one union of author/public audiences and
  author/ordinary perspectives across the whole closure. Ordinary body pins
  keep the same time gates as dated prose; identity alone cannot expose a future
  event or an unfinished aggregate. Story-point pins resolve lifecycle members.
- A writing frame explicitly fixes the source fingerprint, nullable work binding,
  scene, POV, exact story time and lens. A work binds one private fingerprint and
  the same canonical/private revision; its closure never supplies prose, scene,
  POV, time or grants. Only author choices disclose work labels. Stale frames
  fail without silently repinning. A selected
  present scene grants its admitted roots only to that frame; ordinary character
  reads retain their existing scope. Character prompts and author margins stay
  separate, and complete Unicode-scalar packet bounds never truncate evidence.
  Literal present object/environment and outgoing relationship values preserve
  admission and exact types without causal prose or inferred qualitative state.
  Browser history and unchanged active refresh retain original frame pins and
  require native revalidation; they never silently recapture source.
- Setting shared current time is a separate explicit plan. Include the world
  and every active scene cursor, then validate the complete candidate. Leave
  non-active scenes, events and learning unchanged. Use the existing reviewed
  outcome publisher; a preview grants no application authority.
- Coordinated scene controls use exact IDs and times, coalesce one update per
  record and validate one complete candidate. Inclusive handoffs end at the prior
  same-tick order; minimum-order handoffs are refused. Replay actual placement
  evidence for active cast and listed objects. A close changes only its selected
  scene; movement without a scene creates only the explicit event.
- Private action drafts retain their original source pin and own saved horizon.
  Their actor/target pointers and prose grant no knowledge. Exact current source
  and prior-file hashes guard private saves; required answer presence and a
  fresh explicit human sufficiency confirmation guard resolution readiness.
  Literal magic requirements are separate recaptured reads, never persisted
  approvals. Learned-only comparisons remain unknown; author comparisons do
  not infer outcomes, costs or consequences.
- Human proposals append immutable exact history, current ordered answer
  snapshots and explicit plans. Original draft base, proposal context and
  current save bindings remain separate. Every unsuperseded context contributes
  to effective staleness; a newer parallel alternative cannot hide an older
  stale one. Appending requires a fresh complete human review. Candidate plans
  construct against one captured base and validate together; exact sources,
  diffs and diagnostics are complete bounded results. Preview and private save
  never approve or publish canonical consequences.
- Applying a saved outcome requires its own exact complete review and explicit
  approval. Canonical files and terminal private receipt share a guarded journal;
  pending publication fences current captures, and generation changes reject
  overlapping captures. Recovery restores exact before images only. Retained
  receipts verify source bytes and bind the complete reviewed plan/report.
  Optional explicit semantic checks use their saved horizon; required checks
  must pass, advisory checks remain informational, and neither authenticates
  approval. Same-horizon deltas do not infer causal attribution.
- Historical private reads preserve original selected commit/path/blob bytes.
  Action applicability compares to that selected canonical snapshot; edit
  currentness compares separately to one live capture and can be unavailable.
  Private saves retain citation syntax without knowledge or implicit current
  references. Saved outcome review separately verifies the exact original
  owner/member/path/blob and binds its raw-source digest. Current-only consumers
  remain fenced.
- Retained edits preserve unfinished patch/body text. Original base and current
  save/file pins stay separate; canonical review requires exact saved bytes and
  current target identity. Explicit approval publishes the target and terminal
  receipt through the same journal. Body mode alone controls prose replacement.
- Revision review admits each original world independently at the exact horizon,
  aligns stable owner/member IDs and compares shared relative order. Excluded
  future source members do not affect source labels, positions or counts. Original citations
  never borrow a merged view, and equal values with changed provenance are not
  state conflicts. Independent state replay retains the full native local-author
  semantic scope rather than inheriting narrative owner visibility. Unsupported
  complete admission or bounds refuse the report.
- Explicit state bindings replace ordinary leaf validation while preserving
  declarations and exclusivity. Set replaces a complete typed object; clear
  means absence. Generic state/context omit bound values and citations; explicit
  lore state reads preserve source indexes and last set/clear provenance.
- Species composition follows only explicitly selected, declared entity links
  among admitted concepts. Common and individual statements remain separate.
  Unknown, null and empty values stay distinct. Cycles or traversal exhaustion
  refuse a partial result. Common-source edits never rewrite individual source.
- Magic composition assigns each selected profile to one explicit role. Only
  selected root entity fields supply links. Restrictions and exceptions retain
  their own source, exact values and uncertainty; no role wins or computes a
  cost, rule outcome or effect. Scope filtering precedes edges and work bounds.
- Explicit description sections have their own intervals and literal scopes.
  Their earlier same-owner corrections preserve exact applicability and labels;
  dated reads suppress admitted predecessors while history retains them. Body
  prose is separate. Sections and assertions grant no character knowledge.
- Family context readers require an exact author horizon. Social language/sense/form
  selection preserves separate spellings and explicit memberships. Environmental
  links expand only from selected roots, without recursive dependency calculation.
  Both retain native typed values and separate assertion/section evidence; no
  canonical account teaches a character or supplies an inferred outcome.
- Character family composition starts from independently admitted concepts.
  Project typed values before discovering links, traversing, counting or checking
  bounds; hidden source positions cannot affect the result. Explicit belief
  targets select literal learned streams separately from canonical subjects,
  classifications and corrections. Character magic uses those learned targets
  only. Native unknowns remain explicit; writing context never interprets a
  restriction as an action requirement or computes an outcome.
- Predicate declarations validate every retained assertion value. Classifications
  and attribution stay authored; replacement edges preserve exact subject,
  predicate and validity, remain acyclic and never choose between branches.
  Admission and typed projection precede replacement suppression and limits.
  Historical evidence syntax does not authenticate a blob or a live reference.
- Spatial topology reads are explicitly author/all-time. Parent links do not
  imply travel. Canonical open routes and directed location portals supply hop
  edges; position portals do not snap to colocated locations. Weighted paths sum
  one explicit route metric within one exact unit; portals and missing metrics
  supply no weighted edge. Geometry stays map-scoped and anchors are literal
  point correspondences. Do not infer costs, unit/coordinate transforms or
  geodesic distance. Overlay reads apply exact inclusive authored intervals and
  presentation scope before membership and budgets; they do not grant character
  evidence. Separately authored history changes only its named field within exact
  inclusive validity; static source remains the fallback. Corrections retain
  their source chain, and parentage stays acyclic at starts and end successors.
  Explicitly dated geometry and route inspection use these values; ordinary
  paths, reachability and containment inspection keep static semantics.
- Compile into a separate complete SQLite file, then replace the old cache.
  Validate before writes; source fingerprints invalidate stale caches.
- Semantic and hybrid retrieval are explicit modes; lexical remains unchanged.
  Character targets use the existing admitted corpus, while a separate fixed
  public prior alone determines LSA terms, IDF and components. Only eligible
  timeless native world/location/object sections train that prior, without
  granting recall. Missing prior or exhausted bounds fails without substituting
  a search mode. Both hybrid rankings precede final limits; scores are relevance,
  not truth or confidence. See [RETRIEVAL.md](RETRIEVAL.md).
- An edit preview is pure. Apply binds the replacement to the source fingerprint
  and exact candidate hash, validates it, and atomically creates or replaces one
  file. WEDL writers share an OS lock. Git staging and commits remain user operations.
  This single-file edit path has no retry journal. Separately approved saved
  outcomes use the bounded [multi-file publication contract](OUTCOMES.md).

## Implemented features

- Source/Git loading, core validation, typed temporal state, knowledge and
  relationships, conversations, author context and SQLite FTS.
- Story-point lifecycle, conditions and dependencies; explicit causal history;
  timed environments. These reads have CLI, HTTP and browser interfaces.
- Scene and conversation interval/participant validation, inclusive scene
  overlap checks, dated author scene queries, and one shared audibility rule.
  Dated character context, search and conversation views have distinct CLI,
  HTTP and browser actions. Character search builds a temporary in-memory FTS
  corpus from admitted evidence. Full-response regression tests cover private
  and future edits, search scores/counts, and context budget failures.
- Minimal world creation, typed creation and partial updates for characters,
  locations, objects, events, knowledge, scenes, conversations, routes, maps,
  anchors, portals, overlays, lore concepts, assertions, existence episodes, context briefs,
  all eight generational kinds, and one
  shared writer for these and body edits. Requests preview by default, preserve
  omitted metadata and prose, and reject unknown fields. Event/knowledge shapes and references are checked;
  knowledge uses one `claim.truth` field. Scene/conversation requests use the
  validated nested shapes and explicit status edits, defaulting new records to
  planned/draft. Location edits accept parent_id and point/line/polygon placements; routes use
  declared endpoints, direction, modes, availability and typed metrics. Map edits
  validate local/geographic 2D/3D declarations, origin/scale and bounds against
  existing placements. Anchor, portal and overlay requests share exact reference,
  scope and time validation. All 207 authoring requests demonstrate the flows,
  including world chronology/profiles/predicates/state definitions, record annotations, explicit concept links
  and family histories through the same writer. Spatial, lore and
  generational extensions require explicitly declared world capabilities. No new
  writer or workflow engine is needed.
- Canonical location ancestry/direct children, usable directed route/portal adjacency,
  deterministic hop/weighted paths and mode-filtered reachability, exact decimal metric totals and complete
  result/expansion bounds. Weighted ties prefer fewer hops then route-ID sequences;
  unknown weights and mixed units never become implicit costs.
- Local-planar and EPSG:4326 map catalogue, fixed 2D/3D coordinates and authored
  point/line/polygon placements. Bounding-box reads compare geometry envelopes
  using intersects/within; nearby reads use Euclidean coordinate-space distance,
  including 3D faces and degenerate boundaries. Coordinate magnitude is bounded
  to 9007199254740991 and calculations use f64. Directional anchors expose literal
  point correspondences and optional conversion prose. Static/time-bounded
  overlays expose scoped membership at exact horizons. CLI, HTTP and browser
  controls share these reads. The pinned target supplies no generalized executable
  coordinate-transform engine; one is not inferred from anchor declarations.
- Location/route spatial histories use closed local members, exact inclusive
  or timeless validity, independent named fields and retained correction chains.
  Validation rejects conflicting overlap, wrong references/capabilities and
  containment cycles at any boundary, including expiry and order rollover.
  All-time history reads retain corrections; dated reads expose applicable
  unsuperseded members. Dated geometry discovers moved-in placements before
  filtering and limits. Route inspection exposes effective values with exact
  field/correction citations; absent static metrics remain absent. The existing
  writer replaces/clears complete history arrays. CLI, HTTP and browser reads
  share borrowed source values without creating modified source snapshots.
  Separate character history reads now require an independently admitted owner
  and map/non-null parent endpoints before correction suppression. Explicit
  writing scenes now supply scoped roots and optional specialized family context;
  work-bound Writing context and separate reader admission are implemented below.
- Exact calendar catalogue/date/conversion and forward/reverse story-anchor
  queries, including cyclic rules, finite table calendars, leap/intercalary
  months, negative years, isolated calendars, era display and inclusive bounds.
  Calendar and anchor declarations validate as one world-field replacement.
  CLI, GET API and browser share the reads and preserve exact integer strings.
- Validated record chronology annotations covering civil/era partial dates,
  ranges, approximations, conflicts, relative references and durations. Complete
  lists filter declared calendar/era references; stored pairs compare inclusive
  intervals with explicit unknown reasons. Annotations share the existing writer
  for all twenty-seven non-world kinds other than hypothesis. Calendar edits validate their dependent
  annotations. CLI, HTTP and browser expose the reads without adding annotations
  to character evidence or altering story replay.
- General chronology searches support on-date, before, after, overlaps and
  between predicates, with owner/calendar/era filters and authored or era-bound
  matching. Partial civil/era inputs format locally; only explicitly complete
  days convert to a target calendar/era. Interval queries return every declared
  anchor with none/unique/ambiguous cardinality, provenance and optional timeline
  selection. Strict typed JSON extends ordinary date flags to open ranges,
  approximations and conflicts. Namespace checks reject chronology on declared
  operational objects while preserving arbitrary state/metric/condition keys
  and opaque metadata.
- Parentage and union source histories, closed transition/payload validation,
  exact canonical references, strict causes, correction chains/forks and
  authored parentage cycle/duplicate checks. Family, ancestor, descendant,
  kinship-path and history reads share library/CLI/HTTP/browser interfaces with
  cited complete results. The writer supports creation, full-history replacement,
  status changes and endings while preserving omitted prose/metadata. Full
  character-response invariance is tested when valid family histories are added.
- Organizations/affiliations, legacies/tenures/claims and vital histories now
  share exact history validation and queries. Historical renames/reparenting,
  former affiliations, inclusive vacancy expiry, explicit caused transfers,
  claim reciprocity and disclosed vital ambiguity have source-backed behavior.
  Nine CLI/GET/browser reads cover all eight kinds; all support reviewed
  one-record edits. Cross-record claim pairs use explicit coordinated operations
  in the existing reviewed consequence workflow. These author histories
  also preserve complete character-response invariance.
- Seven typed learned generational assertion kinds validate independently of
  canonical truth. Learning, latest held state and inclusive applicability control
  admission; four exact provenance forms validate at first learning. Learned
  names retain disagreements and expire on rejection/forgetting, with no
  canonical fallback. Separate CLI/GET/browser operations expose learned graphs,
  beliefs, labels, discovery, search and complete context. Author discovery and
  context complement canonical reads. Generic character context/search share
  the admission gate, and all typed claims use the existing one-record writer.
  Requests 36–44 include mistaken beliefs, rejection and a separate replacement.
- Structured lore profiles expose eight closed type forms: string, boolean,
  number, null, entity, enum, array and object. Concepts use exact profile IDs,
  authored values, inclusive/timeless validity and presentation scopes. Five
  CLI/GET/browser author reads expose the registry, catalog, concepts and outgoing
  links, with exact field/source citations and complete-or-error bounds. Typed
  concept references withhold invisible targets while retaining original source
  indexes; ID-looking prose stays literal. Enum metadata distinguishes numeric
  and string alternatives under the existing exact integer codec. World profile
  replacement, concept edits and common links use the reviewed writer. Compatible
  profile changes supply no defaults, and invalid dependents prevent publication.
  Requests 64–71 exercise these rules, retirement and explicit link clearing.
  Current profile definitions have no version/history compatibility machinery.
  Author/character family composition and their shared admission gate are
  described below.
- World state profile bindings target existing keys with exact current profile
  IDs. Initial values, whole-object set effects and state predicates share the
  profile validator; clear retains absence and set operations remain atomic.
  Every historical/future/noncanonical effect validates before definition edits
  publish. Ordinary state/context withhold bound values; the exact-horizon
  `lore state` CLI/GET/browser read exposes projected values and original source
  citations, including a clear effect without inventing a value. World `state_keys`
  and `state_profile_bindings` use the reviewed complete-field writer.
- Author species composition selects common/individual profiles, root link
  fields and separate display-role fields explicitly. It follows admitted typed
  references in source order, deduplicates shared ancestors, preserves uncertainty
  and never chooses a winning trait. Affected-individual lists require a confirmed
  explicit chain. Depth 16, 64 records, 65536 link visits and result limits refuse
  partial output. Declared-type checking prevents empty literal arrays from
  masquerading as links. CLI, GET and browser expose both operations. Requests
  72–79 cover these records, bound-state replay and a common-description change
  without rewriting individual records. This adds no source kind or capability.
  Native dated accounts and independently admitted character composition are
  implemented below.
- Author magic composition assigns exact current profile IDs to tradition,
  practice, realm and artifact roles. Up to sixteen explicit root link fields
  per role feed a cited depth-first graph; shared records deduplicate while
  visible source edges retain their original indexes. Independently selected
  restrictions and exceptions preserve exact typed values, unknown/null/empty
  distinctions, source prose and provenance. A cycle, depth beyond 16, more than
  64 visited targets or more than 1024 visits causes the whole read to fail.
  CLI, closed GET and browser controls share the optional author horizon and
  scope. Requests 80–87 add four
  profiles, three explicitly dated practices and a common-tradition edit using
  the existing writer. No source kind, capability or inferred magic mechanics
  are added. Native dated explanations and separately learned character reports
  now compose into writing context. Separate action capture and literal
  requirement comparison and reviewed explicit outcome publication are implemented;
  explicit semantic consequence checks are a separate reviewed-plan selection;
  no inferred magic resolution is introduced.
- Dated Markdown sections use a closed common `descriptions` array with local
  IDs, independent inclusive validity and explicit scopes. `descriptions-v1`
  works independently of lore. Earlier same-owner corrections preserve exact
  validity/labels; unrelated alternatives and correction forks remain separate.
  Author list and exact-section reads retain source indexes and complete limits.
  Reviewed array replacement/clearing never extracts or dates the existing body.
- Current world predicates use any of the eight finite root types. Author
  assertions validate typed subjects/values, explicit classification, required
  claim attribution, exact validity and closed historical evidence syntax.
  Replacement chains preserve subject/predicate/validity and remain acyclic;
  all-time history retains source while dated reads suppress only projected
  admitted predecessors. Hidden root references or attributors withhold a row;
  object/array projection can prune hidden children while retaining the row.
  CLI, GET and the Narrative browser tab expose predicate and assertion reads
  alongside descriptions. Requests 88–97 cover reviewed edits, exact null/large
  integer values, description and assertion corrections, chronology and a
  compatible predicate extension. Assertions added the 22nd non-world kind.
  Linked learning and character section admission are implemented below;
  private manuscript source, work order and paired-source Writing selection are
  implemented separately. Explicit reveal plans and the separate reader service
  are implemented below.

- Social author context selects exact language/sense IDs, literal forms and up to
  eight explicit culture/religion/membership roots. It preserves native intervals,
  overlapping spellings, homonyms, scripts and registers without inferred fluency.
  Environmental context selects one to eight habitat/resource/livelihood/trade/
  infrastructure/technology/era roots and their direct typed/timed links. It keeps
  literal quantities, competing accounts, unknowns and cited edit targets.
  Assertion/description evidence comes only from explicit owners. Both readers
  require a horizon and refuse incomplete results beyond 32 records, 512 evidence
  items or 262144 compact JSON bytes including a newline. CLI, closed GET and the
  Narrative browser share the behavior. Requests 98–125 and two current profile
  packs exercise reviewed publication and independent character evidence.

- Authored existence episodes use `existence-v1` with the generational core,
  independently of lore. All five episode kinds preserve explicit intervals,
  overlapping alternatives and syntax-checked, unverified historical evidence.
  Exact character reads apply endpoint/member scope before complete-result
  bounds; history retains all authored timelines. Episodes do not change vital
  history, age or character knowledge. Common annotations/sections and reviewed
  create/update support make existence the 23rd editable non-world kind.
- Governance queries select an exact current profile and six explicit nullable
  role bindings. Native typed values distinguish known null from unbound/missing
  fields. Every selected typed reference must be admitted before a whole policy
  is returned. Jurisdiction uses exact locations or direct static/timed overlay
  membership; supersedes is a cited pointer without precedence. Independent
  policies retain all four authored bases and native linked assertion evidence,
  including corrections and attribution. Collective Party projections support
  characters, organizations and concepts without widening legacy endpoint rules.
  CLI, closed GET and Narrative browser reads share both author contracts.
  Requests 126–142 leave the complete sequence at 100 records, 25 profiles and
  nine predicates at that checkpoint. Character admission is implemented below.

- Linked assertion knowledge now uses an exclusive `{key, statement, assertion_id}`
  claim and ordinary transitions. Affirmative learning admits literal words;
  later rejection stays held, forgetting withholds them, and canonical changes,
  validity or corrections never rewrite them. Optional knowledge scopes apply
  to plain, generational and linked claims before character output or admission.
- One private character scope derives self, scoped held knowledge IDs and exact
  plain `claim.object.entity` acquisitions. Without an explicit writing scene,
  subjects, literal words, typed
  assertion/genealogy targets and transitive links grant no canonical records.
  Character sections, lore values/links and spatial history share this scope;
  hidden endpoints filter before correction, counts or limits. Current author
  titles, untimed bodies, source metadata and source-array indexes are omitted.
  Generic character context/search add only admitted sections and learned words.
  Existence remains unknown without typed learned episodes; character governance
  retains learned claims with unknown jurisdiction and no inferred Party/basis.
  Nine explicit character CLI/GET/browser reads require an exact horizon and
  accept no caller-supplied scope labels or record grants. Requests 143–158 bring
  that checkpoint to 112 records without changing the 25 profiles/nine predicates.

- Authored `context-brief` records add the 24th editable non-world kind. Exact
  record, section and transition pins form an ordered required closure, with
  nested cycles and duplicate targets handled without partial output. Author
  projections reuse native dated folds; character projections use independently
  acquired evidence and literal learned words. Unavailable required material
  returns one generic unknown result before item bounds. Requests 159–175 cover
  nesting, a cycle, revised prose, exact members, hidden pins and mistaken reports.
  That 175-request checkpoint leaves 125 records with 25 profiles and nine predicates.
- Stateless writing frames retain exactly fingerprint, nullable work, scene, POV, time and
  author/character/dramatic-irony lens. Resolve, choices and complete context
  share native, CLI, closed GET and tab-local browser controls. Explicit selected
  scene presence grants admitted location, current participants, objects,
  environments and scoped relationship endpoints. Character prompts remain
  separate from author margins; dramatic character content equals the standalone
  character result. Source changes require explicit reselection. Reads never
  modify source or cursors. Specialized family context is implemented below.
  Work selection validates the complete private closure against its exact
  fingerprint without injecting prose or deriving other selections. Author
  choices disclose minimal work IDs/labels and the pin; character/dramatic
  choices disclose no work catalogue. Current capture fences the complete
  result with canonical/private rechecks and publication generation; historical
  capture uses the same original commit for both sources. Unbound character
  reads do not open unrelated private files. Writing has no occurrence selector.

- Species author compose/affected and magic author compose attach native
  assertions and dated sections for their actual selected owners when those
  capabilities exist, preserving lore-only reads. Character species compose and
  affected lists, social language/culture context and direct environmental
  context operate over independently known typed concepts. Their output omits
  current titles, untimed bodies, author citations and source-array positions.
  Explicit assertion/knowledge selectors add separately learned literal streams;
  canonical corrections never classify or rewrite those reports. Character
  magic uses only explicit learned targets, preserving the pinned source split.
- Optional closed family selections compose all four families inside the exact
  writing frame. Selected roots must resolve, native unknown linkage remains
  explicit, complete limits refuse truncation and the dramatic character half
  equals the standalone character half. Direct CLI/GET/browser actions and
  deliberate per-family browser capture share those contracts. Requests 176–203
  cover delayed acquisition, forgetting, exact spellings, hidden dependencies,
  native corrections and mistaken reports. That 203-request checkpoint leaves 141 records
  with 25 profiles, nine predicates and 24 editable non-world kinds.
- Explicit semantic and hybrid modes extend author and character search across
  native, CLI, closed GET and browser interfaces. Local normalized TF-IDF and
  bounded deterministic SVD supply cosine relevance; hybrid independently ranks
  FTS and semantic candidates, then combines ranks with equal-weight RRF at k=60.
  Default author search retains its all-time title/body corpus and excludes hypotheses;
  character search retains its exact dated evidence and separate public prior.
  New modes fit in memory and write no cache or source. Requests 204–207 use the
  existing writer to add a public prior, literal ferry report, dated notice and
  independent acquisition. All 207 requests leave 144 records with the same
  25 profiles, nine predicates and 24 editable non-world kinds.

Explicit author horizons now rank eligible source prose, description sections,
environment conditions, core transitions, unbound event effects, ordinary
knowledge statements, observations and conversation accounts after their own
applicability/kind gates and before complete corpus bounds. Raw profile-bound
values and required briefs keep their separate domain workflows. Optional exact
thread IDs use ANY membership after ranking;
the complete native candidate window precedes grouping and the final output
limit, preserving scores and training. Both selected paths are read-only and
cache-free; omitted selections preserve previous all-time output exactly.
CLI/GET/browser expose all three modes and the same selection grammar.
See [RETRIEVAL.md](RETRIEVAL.md).

`current-time preview` builds one explicit existing consequence plan for
`world.current_time` and every active scene's `time.current`. Planned, closed and
abandoned scenes remain unchanged. Candidate validation decides whether an exact
backward or timeline-changing request is valid. The complete report counts its
plan, source diffs and diagnostics, preserves exact equal-time source bytes, and
never applies changes. CLI/GET/Writing controls export the same plan for reviewed
Actions and Consequences. See [CURRENT_TIME.md](CURRENT_TIME.md) and the executable
[Writing guide](../examples/writing/README.md).

Private action drafts now retain exact original provenance, saved horizons,
ordered questions and explicit readiness outside canonical source. Guarded native
saves bind current source and prior private bytes; the Actions view prepares,
reviews, saves and exports the same requests. Magic requirement captures compare only
explicit admitted author operands; learned-only character/dramatic comparisons
remain unknown. Bounded human proposals now retain immutable answer snapshots,
exact raw YAML plans and explicit supersession. Fresh review binds the complete
proposed draft. Active context drift is disclosed as effective staleness without
rewriting original provenance. Complete consequence previews reuse the typed
record editor, validate all candidate records together and show exact source,
hashes, line diffs and diagnostics. Ordinary forward/mutual references work;
construction requiring a new profile binding or target kind from another
operation remains subject to the existing captured-base codec boundary.
Saving, comparison and preview never execute consequences. A separate application
binds an explicit `approved:true` decision to the complete outcome review.
The shared writer lock, bounded recovery journal and generation fence protect
native current captures. Publication retains canonical source and one terminal
private receipt together; exact retries write nothing, and explicit recovery
restores the before state. Read-only verification distinguishes whole-source
drift from changed receipt files. Optional immutable plan analysis now checks
seven explicit predicates and compares typed state, knowledge, relationships,
story-point lifecycle/eligibility and outcome links at the same horizon in both
worlds. Required non-passes block reviewed application; advisory results remain
informational. Applied verification separately reevaluates retained checks on
current source. No citation-only delta or inferred causal attribution is added.
See [ACTIONS.md](ACTIONS.md), [CONSEQUENCES.md](CONSEQUENCES.md),
[OUTCOMES.md](OUTCOMES.md) and [SEMANTIC_CONSEQUENCES.md](SEMANTIC_CONSEQUENCES.md).

Retained general edit drafts now preserve unfinished exact patches and body
buffers, use separate base/save/file pins, and apply an explicitly reviewed
single-target candidate plus terminal receipt through the existing publisher.
Current and historical private action/edit list/detail reads retain original
source bytes and full commit/blob metadata. Required action/proposal evidence
arrays retain authored pointers on save; saved outcome review verifies their
original source and binds its complete byte digests. Standalone three-way revision review
compares independently admitted original worlds and reports five conflict
families at one horizon, with complete bounds and no merged candidate.
See [EDITS.md](EDITS.md) and [REVISION_REVIEW.md](REVISION_REVIEW.md).

Typed retcon impact now follows exact source occurrences over independently
admitted originals, with complete depth/item/work bounds and separate candidate
plan provenance. Saved retcon documents preserve original report, item digests
and admitted-input witness; title/decisions/commentary are mutable under current
source/private CAS. Currentness compares one bounded live capture and complete
original first-parent history, so uncommitted relevant edits stale immediately
and committed change then revert stays stale. Historical private bytes remain
readable with an unknown comparison. Witness values preserve exact numeric tokens
in canonical JSON literal strings. Explicit manuscript source links, POV references
and work occurrences now join the graph through an independently supplied private
snapshot. Admitted private bodies are opaque witness values; prose is not scanned
for dependencies. Original private citations retain source provenance on both
base and candidate rows.

Event-local analysis retains the focus event and unrelated same-time writes,
excludes its own effects and exactly caused transitions at that coordinate, and
separates delayed history and current-horizon supersession. Grouped candidate
analysis separates source sections from semantic value changes and retains mixed
or uncaused sections without event attribution. Both use the full native
local-author semantic scope and filter unbound scalar entity effects by target
admission at the selected horizon. Initial values, entity sets and bound
profiles retain their existing semantics.
See [RETCON_IMPACT.md](RETCON_IMPACT.md), [RETCON_REVIEWS.md](RETCON_REVIEWS.md)
and [EVENT_CONSEQUENCES.md](EVENT_CONSEQUENCES.md).

The live author browser now saves private sources and applies exact approved
action/edit candidates through the existing writer. Canonical reads capture fresh
bounded source; independent private and historical reads retain their original
selectors. Retcon queue/detail capture live currentness without rewriting originals.
Historical author sessions remain immutable and read-only. See
[BROWSER_PUBLICATION.md](BROWSER_PUBLICATION.md).
Implemented behavior remains subject to the coverage limits in `PROGRESS.md`.

## Manuscript foundation

Private `wedl-author/v1` Markdown under `author/segments/` and `author/works/`
now has immutable IDs, closed field rules, exact source/body bytes and a separate
fingerprint. Segments carry explicit static source links, optional full placement
and optional POV. Works preserve authored chapter/list order; explicit reuse
produces distinct `(work, chapter, segment index)` occurrences sharing one segment.
No placement, source link or private prose adds canonical state, learning or reader
admission. Empty works are valid; each authored chapter contains at least one segment.

List/detail inspect structurally valid current or original committed private source
independently of canonical validation. Complete work resolution pairs matching
canonical and private captures. Preview/save validate the complete candidate graph
and require canonical, whole manuscript and original file pins. Native/CLI reads,
author GET routes and guarded manuscript POST saves share these contracts. Historical
source stays read-only; downloads preserve exact source until explicitly edited.
See [MANUSCRIPT.md](MANUSCRIPT.md) and the executable
[manuscript example](../examples/manuscripts/README.md).

Retcon review reconstruction and currentness now pair original private and canonical
sources at each comparison. A new original capture requires live manuscript source
to match its committed base. Newly incoming private dependencies matter. Whole private
fingerprints serve capture/CAS only; unrelated private changes do not stale selected
inputs. Unchanged verified original bytes can retain formerly valid links after a
canonical or referenced-segment removal, producing stale evidence; changed/new invalid private source makes
the comparison unknown.

## Explicit reveals and reader presentation

Private reveal plans under `author/reveals/` bind one exact work. Ordered milestones
select authored chapter/segment occurrences and cumulatively admit explicit records,
description sections or exact text. Reused segments retain distinct reading positions;
story placement never advances reader progress. The shared private writer validates
the whole candidate including plans, and exact source inspection remains separate
from reader authority. Reveal plans do not invent retcon dependency edges.

Pure projection intersects trusted canonical scope with ordinary audience/perspective,
owner and story-time admission. Description corrections suppress eligible predecessors
before the reveal match. A section-only reveal does not supply its owner's title/body
or siblings. Native privacy deliberately carries private heading restrictions through
descendants unless an explicit nearer audience mapping overrides them. Character
projection retains its existing learned-owner rules and excludes ordinary raw bodies.

The isolated loopback reader service starts without authority. A trusted installed
policy fixes repository identity, exact original commit, work, plan, scope, generation,
reading ceiling, story horizon, text policy and cursor secret. Reader requests select
within it; they supply no grants. Author simulation has a separate policy and intersects
its author permissions with the canonical scope. Revocation/replacement is checked
again at emission, including when source work finishes in a worker thread.

Only admitted minimal items supply output, Unicode-folded search, counts and pagination.
Opaque HMAC cursors bind every relevant selection and policy input. Complete source,
dataset and serialized response bounds precede pagination. No private work/plan labels,
prose, source metadata or hidden totals enter reader responses. Original source reads
never fall back to live files. The dedicated client uses only reader page/search routes;
the author Manuscripts tab separately exposes reveal source and authorized simulation.
See [REVEAL_PLANS.md](REVEAL_PLANS.md), [REVEALS.md](REVEALS.md),
[READER.md](READER.md) and the executable [reader example](../examples/readers/README.md).

## Typed core authoring and hypothesis workflow

All twenty-eight non-world kinds now support the existing typed create/update
pipeline, with update-only world declarations. New environment, relationship,
story-point and hypothesis fields use complete candidate validation. World
registry removals cannot orphan live references; map/list replacement and
kind-specific common labels retain literal author values. Affordance duplicates,
empty strings, scalar conditions and optional raw relationship defaults survive.
No reciprocal relationship, metric default or story-point activation is inferred.

[Hypotheses](HYPOTHESES.md) has bounded author-only all-time list/detail and
closed create/adopt-existing/reject requests that export ordinary Plans. Adoption
links eligible existing records without creating facts or knowledge. The browser
retains exact source pins, complete previews, downloads and explicit transfer to
existing review buffers. Optional author search adds a separately bounded lexical
lane; canonical results and model inputs stay unchanged, including dated and
semantic/hybrid searches. Thread selections suppress the hypothesis lane.

## Author navigation

[Core navigation](NAVIGATION.md) now supplies character whereabouts and
full-cohort prominence, all-status timeline points and spans, upstream/downstream/
both causal tracing, dated story-point trails and scene conditions, explicit
pair event participation, and selected thread membership. The same reads serve
the library, CLI and browser. Exact time, independent endpoint admission, complete
limits and source preservation remain the correctness boundaries.

## Admitted catalogue

[Admitted catalogue](CATALOGUE.md) now supplies author all-time/as-of and exact
character-in-scene pages and requested labels. Admission precedes filtering,
search, facets and pagination. NFC/casefold name ordering, full-cohort author
prominence, selected thread intersections and source-bound continuation share
the native API, CLI and GET/browser controls. Search is complete within explicit
native corpus bounds and never silently truncates before pagination.

## Saved action evidence and hypothesis links

[Outcome review](OUTCOMES.md#original-evidence) now resolves draft and selected
proposal citations against authenticated original Git worlds, exact members,
paths and blob identities. Ordered raw-owner digests bind the complete review
and applied receipt. First application rechecks the proof; terminal verification
retains its publication-only scope. [Private hypothesis links](ACTIONS.md) are
required explicit arrays, validated against existing hypotheses when saving or
reviewing, with no canonical or character grants.

## Static authoring guidance and completion

[Authoring reference](AUTHORING_REFERENCE.md) exposes the native Plan JSON Schema,
all editable kind fields/defaults and transition vocabularies. Editor and history
validation share the guidance metadata. The library, repository-free CLI and
closed author GET route return the same static result without source pins.
The Consequences browser inspects record fields and downloads the reference.
Source-aware validation and reviewed publication remain separate.

[Completion](COMPLETION.md) derives Bash and PowerShell suggestions from the
built CLI parser: commands, options and fixed choices. Completing a line does
not invoke WEDL or inspect authored files. Option values and literal positionals
must not be mistaken for nested commands.

## Contextual authoring helpers

The shared authoring context returns all eligible record/member choices,
capability-aware starter families, exact current time, and lossless typed YAML
declarations from one validated capture. Complete row and byte bounds refuse
partial results. Exact member enumeration is shared with native brief references.

Source-pinned scaffolds use the existing editor's field metadata and native Plan
preview. Existing-record starters must be byte-identical no-ops while retaining
body data, absent/empty distinctions and direct-source fields. Description and
spatial-history starters expose existing members and a fresh local ID; the
organization starter requires explicit identity/title and uses native current
time. Owner/capability mismatches and unrepresentable typed edits are refused.
The CLI, author HTTP and browser share these reads. Browser insertion into the
Plan editor is explicit; source/selector/tab changes discard stale results.

## Remaining work

Static guidance, shell completion and contextual authoring helpers now cover the
named helper inventory. The integrated review added literal present Writing
facts, automatic browser-frame restoration/revalidation and a startup selection
race fix. **Estimate: 0 further named feature iterations** against the inspected
pinned target. Restoration and compiled-artifact mismatch diagnosis remain
independently unbounded; a newly demonstrated gap may reopen feature work. The target remains
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`.

| Remaining area | Iterations |
| --- | ---: |
| Confirmed feature inventory and integrated review | 0 further named iterations |
| Earlier source-restoration and compiled-artifact mismatch observations | Diagnosis cannot yet be bounded |

[CORRECTNESS_REVIEW.md](CORRECTNESS_REVIEW.md) records the pinned callable evidence,
current native boundaries and completed behavior. The nine upstream helper
families share native schema/context/scaffold operations. Generating guidance or
a starter performs no write. Keep review focused on source admission, exact
retention, complete bounds and useful interfaces. Migrations, adapters and
release infrastructure remain outside scope.

Work-bound Writing, exact post-ranked thread grouping, explicit shared-time
plans, all twenty-eight non-world typed editors and established world/common
fields are complete for their specified native contracts. Writing has no required
work-occurrence selector. Hypothesis list/detail, create/adopt-existing/reject
Plans and opt-in lexical discovery keep possibilities separate from canonical
state and retrieval. Literal object affordances are editable data. Separate
coordinated controls now provide scene creation/advancement/closure, presence
handoffs and held-object membership through ordinary consequence Plans. They
reconcile active placement state and exact-time event participation without
inventing learning or rewriting existing events. See [SCENE_CONTROL.md](SCENE_CONTROL.md).
[Conversation controls](CONVERSATION_CONTROL.md) now create the reciprocal scene
link and append exact speech/action beats with ordinary Plan review; explicit
time leaves clocks unchanged.

Structured dated search now composes independently admitted effects, transitions,
conditions, knowledge, observations and conversation payloads. Full-author brief
admission uses one complete union closure, ordinary pinned bodies retain their
time gates, and lifecycle transition pins use their actual nested source path.
Semantic comparisons filter future unbound scalar-entity effects before replay,
preserving original ordinals and using that same replay for derived eligibility.
Set/profile operands and initial values retain their established native behavior;
the pinned consequence projector does not establish generalized recursive pruning.
Keep intentional character raw-body exclusion and conservative private-heading
inheritance explicit. No standalone public historical-citation resolver was
found. The actual saved-outcome consumer now verifies original revision,
owner/member, path and blob; private hypothesis links are implemented separately.

Preserve canonical facts independently of mistaken beliefs and learned names.
Each slice includes source rules, validation, queries, authoring and useful
interfaces. [PARITY.md](PARITY.md) records the pinned evidence and assumptions.
Existing guarded writers and reviewed multi-record publication now back the
browser preview/approval/application controls. Top-level optional-field
removal may remain a documented direct-source edit. No new writer or release
infrastructure is needed.

Ordinary body prose remains excluded from character views. Explicit dated
sections require known-owner and member admission before context or search.
Audibility and observations never create beliefs or memories. Open verification
observations in [PROGRESS.md](PROGRESS.md) still prevent correctness closure.

Before extending a feature, remove obsolete assumptions from this plan and edit
fixtures to match the current v1 rules. Keep unsupported behavior explicit.
Measure performance only when a real workload exposes a bottleneck.

## Evidence

Use focused source/time/type/authoring tests and the four maintained example
worlds. Retain the 348 recorded core regression results where those semantics
remain intended; they were originally captured from Python at upstream commit
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`. They are not a promise of future
Python compatibility. See `PROGRESS.md` for current results, `SCHEMA.md` for source
rules and `AUTHORING.md` for the request contract.
`CHARACTER_VIEWS.md` defines the implemented admission boundary and its limits.
`RETRIEVAL.md` defines local LSA, public training, independent rankings and bounds.
`GENERATIONAL.md` defines the implemented family/history contract and bounds.
`LORE.md` defines the typed profile, scoped concept and explicit-link boundary.
`SPECIES.md` defines author/character common and individual composition and bounds.
`MAGIC.md` defines author role composition and separate literal learned reports.
`DESCRIPTIONS.md` and `ASSERTIONS.md` define dated sections, propositions and corrections.
`SOCIAL.md` and `ENVIRONMENT.md` define author/character contexts and their bounds.
`EXISTENCE.md` and `GOVERNANCE.md` define authored episodes, explicit role bindings,
collective Party projections and the remaining admission boundary.
`LEARNED_ASSERTIONS.md` and `NARRATIVE_ADMISSION.md` define independent learned
claims, acquired-record scope and the shared character projections.
`BRIEFS.md` and `WRITING_FRAMES.md` define required closures, exact frame selection,
scene-derived admission and separate character/author packets.
`SCENE_CONTROL.md` defines coordinated lifecycle/movement Plans, carried-object
continuity and candidate-only placement reconciliation against pinned authoring.
`ACTIONS.md` and `CONSEQUENCES.md` define private history, human review, explicit
candidate construction and separate reviewed publication. The applied lifecycle,
combined source/private outcome, exact replay and recovery are additionally
grounded in the pinned action-resolution code and tests.
`SEMANTIC_CONSEQUENCES.md` records the explicit-check and same-horizon comparison
contract, grounded in pinned consequence expectation/projection/delta code and
tests. `EDITS.md` and `REVISION_REVIEW.md` record the retained-edit and
three-original contracts, their historical-source evidence and explicit admission
limits. `RETCON_IMPACT.md`, `RETCON_REVIEWS.md` and `EVENT_CONSEQUENCES.md`
record typed dependency graphs, immutable review witnesses/currentness,
exact-time event exclusion and grouped source changes against the pinned runtime
and tests. Native scope limits remain stated separately from parity claims.
