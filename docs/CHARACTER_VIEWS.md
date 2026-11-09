# Character evidence views

The in-progress `wedl-rust/v1` contract provides separate character context,
search, conversation and narrative reads. They are author-selected projections over
explicit dated evidence in a valid snapshot. The local browser still has full
author access; selecting a character is not authentication.

## Entry points

| CLI | GET route |
| --- | --- |
| `character context CHARACTER_ID --tick T` | `/api/character/context?character=ID&tick=T` |
| `character search CHARACTER_ID QUERY --tick T [--mode lexical\|semantic\|hybrid]` | `/api/character/search?character=ID&q=QUERY&tick=T` with optional `mode` |
| `character conversation CHARACTER_ID CONVERSATION_ID --tick T` | `/api/character/conversation?character=ID&id=CONVERSATION_ID&tick=T` |
| `character assertions CHARACTER_ID TARGET_ID --tick T` | `/api/character/assertions?character=ID&target=TARGET_ID&tick=T` |
| `character descriptions CHARACTER_ID OWNER_ID --tick T` | `/api/character/descriptions?character=ID&owner=OWNER_ID&tick=T` |
| `character section CHARACTER_ID OWNER_ID SECTION_ID --tick T` | `/api/character/section?character=ID&owner=OWNER_ID&section=SECTION_ID&tick=T` |
| `character lore CHARACTER_ID --tick T` | `/api/character/lore?character=ID&tick=T` |
| `character concept CHARACTER_ID TARGET_ID --tick T` | `/api/character/concept?character=ID&target=TARGET_ID&tick=T` |
| `character links CHARACTER_ID OWNER_ID --tick T` | `/api/character/links?character=ID&owner=OWNER_ID&tick=T` |
| `character spatial-history CHARACTER_ID OWNER_ID --tick T` | `/api/character/spatial-history?character=ID&owner=OWNER_ID&tick=T` |
| `character existence CHARACTER_ID TARGET_ID --tick T` | `/api/character/existence?character=ID&target=TARGET_ID&tick=T` |
| `character governance CHARACTER_ID TARGET_ID --tick T` | `/api/character/governance?character=ID&target=TARGET_ID&tick=T` |
| `character brief CHARACTER_ID BRIEF_ID --tick T` | `/api/character/brief?character=ID&brief=BRIEF_ID&tick=T` |
| `character species CHARACTER_ID INDIVIDUAL_ID <selection flags> --tick T` | `/api/character/species` |
| `character species-affected CHARACTER_ID COMMON_ID <selection flags> --tick T` | `/api/character/species-affected` |
| `character magic CHARACTER_ID --belief-target TARGET_ID --tick T` | `/api/character/magic` |
| `character social CHARACTER_ID LANGUAGE_ID --tick T` | `/api/character/social` |
| `character environment CHARACTER_ID --context CONCEPT_ID --tick T` | `/api/character/environment` |

All take optional timeline and order. The default order is the end of the tick,
`2147483647`; pass an exact order to distinguish beats within one tick. Public
tick and order coordinates are decimal strings. Character and conversation
selectors must be exact IDs; titles and aliases never participate in resolution.
CLI/HTTP reads use the same service. Unknown or duplicate HTTP parameters fail.

The five family GET routes also require `character` and `tick`. Species requires
`id`, `common_profile`, `individual_profile` and `individual_link`; CLI selection
flags use the hyphenated names in [SPECIES.md](SPECIES.md). Social requires
`language`; environment requires `contexts` as a JSON string array. Magic
requires `beliefTargets` as a JSON string array of one to eight distinct exact
assertion or knowledge IDs. Species composition, social and environment accept
that array optionally; affected discovery does not. CLI uses repeated
`--belief-target` and `--context` options. Full closed selections and bounds are
specified in [SPECIES.md](SPECIES.md), [MAGIC.md](MAGIC.md),
[SOCIAL.md](SOCIAL.md) and [ENVIRONMENT.md](ENVIRONMENT.md).

The character transport envelope has exactly `protocol` and `result`. The
result identifies its character, effective time and character perspective.
Context, search, conversation and learned assertions use
`explicit-authored-evidence`; shared narrative reads use `explicit-record-admission`.
It contains no global source fingerprint,
revision, file path, blob hash or source-array index. Author routes retain their
normal source metadata and have no parameter that switches their perspective.

## Admission rules

| Evidence | Admitted content |
| --- | --- |
| Plain belief | Own scoped canonical knowledge record's latest transition at/before the horizon, unless forgotten; statement, state, confidence, acquisition and exact time |
| Generational belief | Own scoped typed assertion, explicitly learned and currently applicable, with latest held state; literal payload, learned labels, validity, confidence and safe provenance |
| Linked assertion belief | Own scoped literal statement with a prior affirmative learning transition and latest non-forgotten state; a later rejection remains a held account |
| Description section | An independently known owner and a section whose audience, perspective and validity admit it at the horizon; only admitted replacements suppress predecessors |
| Conversation beat | Speech or action at/before the horizon that the character could perceive at its origin |
| Recollection | Own latest recollection for the conversation at/before the horizon, unless forgotten |
| Scene observation | Observation at/before the horizon, visible to the character at origin, and not past an explicit inclusive until |

Only active/closed scenes and conversations supply evidence. Inclusion preserves
stable record/member IDs and the original evidence times. Belief and observation
lists use deterministic time/ID ordering; conversation groups use stable
conversation IDs. Description lists preserve eligible member order within each
owner, and generic context groups their owners by stable ID. Hidden members
contribute no output position, source index or search tie breaker.

Knowledge roots may declare optional `audience` and `perspectives` arrays.
Audience admits `public` or the exact viewer ID; perspective admits `ordinary`
or the exact viewer ID. Omission adds no constraint; an empty array withholds
the record. These labels are checked before any of the three knowledge forms
contributes a belief, proof or known source ID. Author labels supplied by a
caller cannot widen character admission.

The shared known-record set starts with the viewer. An admitted ordinary
knowledge record adds its own ID and its explicit `claim.object.entity` target.
Typed genealogy and linked assertion knowledge add their own admitted knowledge
IDs, without teaching their referenced canonical records or endpoints. Subjects,
literal ID-looking words, cause/source pointers and transitive concept links
grant nothing in these operations. Explicit writing scenes use the additional
local grants described in [WRITING_FRAMES.md](WRITING_FRAMES.md). Every target
retains its own scope checks. Concepts and briefs also require their own
canonical status and applicable validity. Canonical assertion,
existence-episode and hypothesis payloads remain unavailable through this gate.
See [NARRATIVE_ADMISSION.md](NARRATIVE_ADMISSION.md) for the complete contract.

Selected-scene [Writing frames](WRITING_FRAMES.md#literal-present-facts) additionally
project physically present listed objects, active environments targeting the scene
or its admitted location, and the viewer's own outgoing relationship state.
These are literal observable facts at that frame's exact horizon. A canonical
event can change an admitted object's observable state without exposing the
event's identity or prose. This component grants no permanent knowledge and does
not change ordinary character context, search or narrative reads.

Character existence reads return an explicit empty unknown result because the
pinned source has no typed learned episode payload. Character governance returns
the viewer's held learned assertion statements for an independently known,
admitted location or overlay, with jurisdiction unknown, basis null and no
parties. It does not classify those words using current policy profiles or
canonical jurisdiction. Historical existence pointers and collective Party
projections grant no extra knowledge.

### Presence and audience

Participant from/until bounds and parent intervals are inclusive. Each beat and
observation checks membership at its own time, so joining later grants no earlier
evidence and leaving does not erase admitted history. Open active scene presence
stops at its current cursor; an explicitly authored end applies as written.
Conversation membership is independent of the linked scene and its location.

Audience `public` or `participants` admits present members, and an exact character
ID admits that present character. Missing, null, empty or author-only audiences
admit no recipients. Present speakers/actors perceive their own beats regardless
of the recipient list. Delivery and addressee fields do not infer acoustics or
expand access. Source validation rejects absent explicit recipients.

An observation with no until remains a dated observation; one with an until is
absent after that exact endpoint. Neither observations nor audible beats create
knowledge transitions, recollections or inferred current state.

### Beliefs and memory

A plain belief includes its statement even if the author considers it false. The
projection omits claim truth, claim keys, raw claim structure, transition notes,
causes and source references. It includes no canonical state or relationships.

Typed generational claims use the shared learning/applicability gate. Accepted,
suspected, uncertain and remembered states are held; rejection or forgetting
removes them. Generic context/search require current inclusive applicability
and expose only the admitted literal, labels and safe evidence identifiers.
Their author key/statement, source notes and canonical endpoint names stay out.
Dedicated generational history/name/search views additionally retain held
assertions after validity expiry. Canonical histories never create a belief;
see [GENERATIONAL_KNOWLEDGE.md](GENERATIONAL_KNOWLEDGE.md).

An assertion-linked claim contains exactly `key`, `statement` and `assertion_id`.
Accepted, suspected, uncertain or remembered establishes affirmative learning;
an initial rejection alone does not. A latest rejection after such learning
retains the literal statement, while a latest forgotten transition omits it.
Canonical assertion corrections, values, status, titles, applicability and
evidence never rewrite or supplement those learned words. The dedicated target
is an exact assertion ID or knowledge ID, not the canonical assertion's subject.
See [LEARNED_ASSERTIONS.md](LEARNED_ASSERTIONS.md).

Recollections may occur after the conversation ends. The latest remembered
record supplies its authored summary, confidence, interpretation, emotional
impression and remembered quotation text. A latest forgotten record removes the
current recollection; it does not erase the separately admitted audible history.

`exactTurns` contains only speech IDs audible by that recollection's time. A
remembered quote retains its own authored text and fidelity. Its `sourceTurnId`
is returned only if that speech was audible; `speakerId` is returned only when
it matches the already-admitted source speaker. Hidden references never fetch
or expand transcript text, and an approximate quote is never replaced with the
canonical wording.

### Ordinary source text

Untimed body Markdown is excluded, including public body headings and the
character's own profile prose. Root titles, aliases, tags, provenance and
`section_audiences` do not supply dated evidence. Explicit `descriptions` use
their own source grammar and admission rules. Their admitted heading and exact
Markdown enter context and search, with stable owner/section identity, validity
and a safe section target. Hidden source indices and author citations stay out.
An admitted replacement can suppress its predecessor; a private or future
replacement cannot. Context adds a `descriptions` array only when it is nonempty.

Generic character lore reads project independently known concepts with their
literal typed values and used types. Entity references and links survive only
when the endpoint is independently known and admitted. Their titles, untimed
bodies, provenance, source indices and author citations stay out. Spatial history
likewise requires an admitted owner and independently admitted map/parent endpoints
before correction suppression. It adds no static fallback or path inference.

Separate character species compose/affected, social/language and environmental
reads now use independently admitted concepts and dated sections. Character magic
selects explicit learned reports without canonical role composition. Optional
belief targets are exact assertion or knowledge IDs; they select literal held
accounts without current canonical subject/classification joins. Family links
expose only admitted targets and root field names, without hidden source positions.
Exact [writing frames](WRITING_FRAMES.md) additionally derive scoped scene roots,
compose [required briefs](BRIEFS.md) and include explicitly selected families.
Their character component keeps these evidence rules and complete bounds.
Ordinary context/search does not select a scene or family implicitly. Action
capture and manuscript/reader evidence remain future work.

`character brief VIEWER BRIEF --tick T` and `/api/character/brief` read one
independently known brief's complete required closure. They grant no scene roots.
If any required material or scope reference is unavailable, the whole brief is
unknown with an empty list. The brief's own prose has explicit scope and validity;
it does not widen the ordinary untimed body boundary. Character pin values use
safe record/member identities without author citations or global metadata.

## Search modes and admitted candidates

All three search modes use the same existing admitted candidate corpus: ordinary
belief statements, typed generational assertion kinds and explicit learned
labels, linked learned words, admitted section headings/Markdown, audible beat
text, observations and personal memory prose. The result kinds include
`belief`, `generational-belief`, `assertion-belief`, `description`, `speech`,
`action`, `observation` and `recollection`. Current author titles, untimed bodies,
canonical assertion values and source metadata never become character search
text. No mode implicitly selects a scene, family or additional known identity.

Admission precedes matching, scoring, candidate counts, snippets and result
limits. Search rows retain kind, record ID, member ID, snippet, score and the
same admitted evidence `value`. A semantic association can rank an existing
admitted item; it cannot create a new belief, fetch a hidden source, or replace
a remembered quotation with canonical transcript wording.

`character::search` remains the original lexical API.
`character::search_with_mode(world, character, query, at, limit, retrieval::Mode)`
adds explicit modes. Omitted mode and explicit `lexical` delegate to the original
implementation and preserve its output bytes, including score and snippet
behavior. CLI and the closed GET route accept `lexical`, `semantic` or `hybrid`;
no other spelling or source-scope parameter is accepted.

| Mode | Ranking and output |
| --- | --- |
| `lexical` | Existing literal-term AND search through in-memory SQLite FTS5/unicode61, with ascending native BM25 score and original response shape |
| `semantic` | Descending cosine similarity after projection into a local latent semantic analysis model; no external embedding service or generated content |
| `hybrid` | Equal-weight reciprocal-rank fusion of independently completed lexical and semantic lanes, adding `1 / (60 + rank)` for each participating lane |

Queries remain nonblank, at most 4096 UTF-8 bytes and at most 64
whitespace-separated terms. Limits remain 1–100, default 20. Lexical terms are
quoted and FTS operator syntax is escaped in both the original search and the
hybrid lexical lane. Each complete lane is ranked before the requested output
limit; changing the limit preserves the earlier prefix and its scores. Ties use
the existing `(kind, record ID, member ID)` order.

Semantic and hybrid results add `mode` and `retrieval`, which describes the
fixed provider, algorithm, limits and effective projection dimensions. It
contains no global fingerprint, revision, training count, source IDs or source
paths. Each row adds nullable `lexicalRank`, `lexicalScore`, `semanticRank` and
`semanticScore`. Ranks are one-based; the explicit lexical-lane score is positive
`-BM25`. Standalone lexical scores retain their original negative BM25 values.
The semantic score is the actual cosine, including zero or negative values.
These scores describe retrieval order, not belief confidence, truth or a
guarantee that every result answers the query.

When a semantic query projects to a nonzero vector, all admitted candidates
retain their finite cosine scores, including zero and negative scores. A zero
query projection yields an empty semantic lane. Hybrid may still return genuine
lexical matches in that case. `total` counts the complete ranked candidate union
before the result limit, never hidden source documents. Semantic-only snippets
contain up to the first 300 Unicode scalar values of admitted text; hybrid uses
the existing FTS snippet for lexical matches. The accompanying evidence value
remains the original admitted projection.

## Independent public training prior

The character model trains only on explicit public, timeless description
sections. Its training selection is independent of the viewer, horizon, query
and admitted candidates:

1. The owner must be a world, location or object record with an eligible
   canonical/active/closed status valid for its kind. Retired owners are excluded.
2. An owner with any `valid` or `time` field is excluded conservatively. The
   world's `current_time` cursor does not qualify the owner's timeless source.
3. Optional owner `audience` and `perspectives` must admit `public` and
   `ordinary`. Omission adds no owner constraint; an explicit empty or unrelated
   label list withholds the owner from training.
4. The section must have native `valid.kind: timeless`, an explicit `public`
   audience and an explicit `ordinary` perspective. Dated intervals, future
   sections and character-specific labels cannot train the model.
5. Only eligible corrections suppress their eligible timeless predecessors.
   The retained training text is exactly heading, newline and Markdown.

Character, scene and conversation descriptions do not train this prior, even
when the viewer can read them. Root body prose, titles, aliases, provenance,
learning notes, canonical assertions and target evidence do not train it either.
Public prior sections on an unknown owner may supply word associations, but
they never become candidates merely because they trained the model. Returned
items must independently pass the normal evidence admission rules.

The local engine uses word unigram/bigram TF-IDF and a bounded truncated
singular-value projection. Training vocabulary and inverse-document-frequency
weights come only from the prior; the query and target texts are projected into
that fixed basis. Adding or changing an admitted target therefore cannot refit
the model or change an unchanged target's semantic score. Rust uses Unicode
lowercasing for its finite tokenizer; this is a native implementation, not a
claim of byte-identical Python/scikit-learn vectors.

This explicit-description prior is a Rust source adaptation of the pinned
upstream ordinary-public-body training prior in
[`vectors.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/vectors.py).
It keeps this project's existing rule that untimed body prose supplies no
character evidence. The candidate and ranking separation follows the same
pinned [`search.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/search.py)
boundary. No source version, provider dependency, compiled character index or
model cache is introduced.

A missing eligible prior produces an explicit error even when there are no
admitted targets. It never silently falls back to lexical search. A nonempty
featureless prior retains the pinned inert-feature behavior and yields an empty
semantic query projection. A trainable prior with no admitted targets yields an
honest empty result. Fit failures and exceeded complete-input/work bounds also
fail explicitly.

New-mode guards permit at most 512 distinct training texts, 1024 combined
training/candidate occurrences and 512 KiB of charged UTF-8 input. The byte
charge includes the query, every eligible prior heading/newline/Markdown, and
candidate stable keys/text plus the shared separator allowance. Occurrences and
bytes are charged before training deduplication; excluded source is not charged.
The adapter checks lengths before copying owned model input. The shared engine
also bounds feature accumulation, up to 8192 selected features, 192 requested
dimensions and numerical projection/solver work. Exceeding a guard refuses the
whole retrieval rather than clipping a corpus or changing modes. Both new modes
remain request-local and write no persistent index.

## Complete context packet bounds

Context assembles the complete admitted result, then measures compact inner JSON
in Unicode scalar values. It refuses overflow rather than truncate evidence.
CLI/service/HTTP maxCharacters is 256–100000 (default 16000); the direct library
function accepts 1–1000000 for callers needing a different bound. Pretty-printing
and the transport envelope are outside this budget. Oversize errors depend only
on admitted content.

## Failure and validation boundary

A missing, wrong-kind, title-selected or inaccessible conversation produces the
same `character view is unavailable` NotFound error. A conversation is admitted
only if it has at least one admitted beat or a retained recollection. Invalid
character selectors use the same generic error. Parameter errors report the
submitted input rule, without hidden source details.

The service validates the entire source snapshot before reads. Invalid source
causes a generic validation failure; detailed diagnostics belong to author
validation. The unchanged-output guarantee applies to valid snapshots whose
admitted evidence, eligible public training prior and effective timeline stay
the same. It covers complete
payloads, errors, scores, counts, snippets, ordering and packet bounds when
private/future content or hidden metadata changes. It is not an authenticated
multiuser boundary or a guarantee about execution timing.

Tests compare full context, transcript and search JSON across such edits and
exercise actual CLI/HTTP envelopes. Boundary cases cover inclusive joins/exits,
observation expiry, forgotten evidence, inaudible memory references and distant
conversation participants. Scene/query rules are in [SCHEMA.md](SCHEMA.md).
`tests/character_retrieval.rs` additionally checks exact lexical compatibility,
independent model training, private/future source invariance, limit prefixes,
selected-transition behavior, timeless-prior corrections, missing/featureless
models, exact large story times, complete input bounds and absence of cache
writes. Shared transport fixtures live in `tests/support/retrieval.rs`.
