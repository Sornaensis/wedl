# Native semantic and hybrid retrieval

The existing `wedl-rust/v1` API supports three explicit search modes:

| Mode | Behavior |
| --- | --- |
| `lexical` | Literal-word SQLite FTS search. With no horizon or thread selection, omitting a mode retains the established route and response. |
| `semantic` | Local latent semantic analysis (LSA): normalized TF-IDF text vectors projected through a real truncated singular-value decomposition, then ranked by cosine similarity. |
| `hybrid` | Independently rank lexical and semantic candidates, then combine their ranks with equal-weight reciprocal-rank fusion. |

`fts`, `vector`, capitalization changes, and whitespace around a mode are not aliases. Public Rust callers can use `retrieval::Mode::{Lexical, Semantic, Hybrid}` and its strict `FromStr` implementation.

This implementation downloads no model, calls no external embedding service, and adds no numerical dependency. LSA learns associations from the admitted training corpus. It does not provide the broad language knowledge of a pretrained neural embedding model. A query whose terms have no usable projection can have no semantic matches.

## Native APIs and interfaces

Author search:

```rust
compiler::search_with_mode(world, query, limit, kind, mode)

compiler::search_with_selection(
    world, query, limit, kind, mode,
    &compiler::AuthorSearchSelection { at, thread_ids },
)
```

Character search:

```rust
character::search_with_mode(world, viewer, query, at, limit, mode)
```

The existing `compiler::search` and `character::search` functions remain lexical. The existing explicit lexical mode delegates directly to them. An author selection with both fields omitted delegates to `search_with_mode` unchanged, including its result shape and index behavior. A horizon is an exact native `StoryTime`; a supplied thread selection is `Some(Vec<String>)`, not an implicit scope inferred from a scene, work, or viewer.

CLI examples:

```sh
wedl-rs --repo WORLD search "vessel crossing" --mode semantic --limit 20
wedl-rs --repo WORLD search "vessel crossing" --mode hybrid --kind object
wedl-rs --repo WORLD character search CHARACTER_ID "vessel crossing" --tick 10 --mode hybrid
```

The author `/api/search` and character `/api/character/search` HTTP routes accept the same optional `mode` field. The browser search controls expose all three choices. Character requests retain the exact viewer-ID and explicit story-time requirements.

## All-time author corpus

Author semantic and hybrid search retain the current **all-time source** boundary. Each candidate contains a source record's ID, title, and Markdown body. The training text is its title followed by its body. Hypotheses are excluded from both candidates and training. An optional exact kind filter applies before lexical statistics, vocabulary selection, IDF values, fitting, candidate ranking, and input budgets.

Only title and body contribute searchable text. Source paths, blob identifiers, aliases, tags, opaque metadata, assertion fields, typed lore fields, and other frontmatter values are not silently appended to either lane. Author result rows continue to identify their source records with their title, source path, and blob identifier.

This default source search does not apply a story-time horizon, current canonical corrections, dated-description selection, or character entitlement. A body can describe future, retired, or draft material. Explicit dated selection is described below; specialized native projections remain the way to ask structured domain questions.

An empty selected author corpus returns an empty result with effective dimension zero. It never fits a model from the query. A source-validation failure prevents both new modes before their fit or lexical search.

## Explicit author horizon

`AuthorSearchSelection.at = Some(at)` selects a request-local **author-as-of** corpus of prose, descriptions and independently dated core fields. The timeline must be declared, and comparisons retain the exact signed 64-bit tick and signed 32-bit order. No active scene, world clock, character, or reader grant is inferred. The selected horizon appears as decimal-string `at.tick` and `at.order` in the response.

Admission happens before query matching, input bounds, lexical statistics, vocabulary selection, IDF, and semantic fitting. All three modes receive the same admitted document list. Eligible nonmatching documents still contribute corpus statistics. A future body, private description member, other-timeline interval, or excluded kind contributes no text or training bytes. The whole source must still pass native validation, including excluded records.

Ordinary source candidates retain their record title as timeless identity and include the Markdown body only when the following explicit source boundary permits it:

| Owner kind | Body admission at a selected horizon |
| --- | --- |
| Event | At or after `time`, on the same timeline. |
| Scene or conversation | At or after the explicit `time.end`. An aggregate without an end remains absent, even after its current cursor or last turn. |
| Environment | Inside `time.start` through optional `time.end`, inclusive, on the same timeline. |
| Other ordinary source | At or after the earliest explicitly timed direct or lifecycle transition, when present; otherwise timeless. |

These are authored prose boundaries. They do not extract dates from words, interpret a body as structured state, or independently date every claim in a summary. Titles can therefore remain searchable before their bodies become eligible. Editing a future title may legitimately change identity statistics; editing only its excluded future body cannot change admitted text, scores, or model dimensions. Author source identifiers and citations continue to describe the current supplied snapshot.

The generic dated source lane excludes generational history kinds, lore concepts, assertions, existence episodes, and knowledge linked through `claim.assertion_id`. Those domains keep their existing typed native projections; their payloads and raw aggregates are not flattened into this prose search. Explicitly selected description sections on these owners can still contribute through their own dated section boundary. Context briefs require an explicit selection in their own workflow and are absent from this broad search. Hypotheses remain excluded throughout.

Explicit owner labels must intersect audiences `{author, public}` and perspectives `{author, ordinary}`. Native owner applicability, including canonical concept or existence-episode validity, is retained. Installed `descriptions-v1` sections independently require those same audience and perspective intersections and applicable validity. Eligible same-owner replacements suppress earlier sections only after this admission. A private or future section cannot suppress, train on, or otherwise influence an admitted section.

Every selected section is a separately bounded candidate containing the owner title, its own heading, and Markdown. It has a stable member key and returns `documentKind: "description"`, `sectionId`, `heading`, and an exact source-section citation. Ordinary title/body rows return `documentKind: "source-prose"` and `bodyIncluded`. All dated rows return `documentKey`; distinct rows may share the same record `id`. Tags, aliases, paths and arbitrary metadata do not become searchable text.

### Independently dated core fields

These candidates join the same admitted corpus **before** matching, bounds,
lexical statistics or semantic fitting. They use the existing exact horizon,
kind and thread controls through native, CLI, GET and browser search.

| `documentKind` | Searchable fields | Admission |
| --- | --- | --- |
| `environment` | Literal condition keys/scalars and sensory strings | Inclusive environment start/end. |
| `transition` | State or relationship status, metrics, facets and note | Each direct knowledge/relationship or story-point lifecycle transition's own time. Earlier history remains searchable. |
| `event-effect` | Explicit target ID, state key, operation and literal value | Event time. Profile-bound state effects are excluded entirely before text or budgets. |
| `knowledge-claim` | `claim.statement` only | Canonical ordinary knowledge, from its earliest authored transition; timeless if there are no transitions. Linked assertion knowledge remains excluded. |
| `scene-observation` | Observation text | Active/closed scene, from observation `at` through inclusive optional `until`. |
| `conversation-turn` | Literal speech text | Active/closed conversation, from the turn's own `at`. |
| `conversation-action` | Literal action text | Active/closed conversation, from the beat's own `at`. |
| `conversation-recollection` | Summary, interpretation, emotional impression, exact referenced speech and authored remembered quote text | Active/closed conversation, from that member's `at`; forgotten members are excluded. |

An open scene or conversation can expose its dated members while its aggregate
body remains absent. Observations retain their own expiry; speech, actions and
memories remain author history after the conversation ends. A later forgetting
does not remove an earlier authored knowledge transition or remembered account.
An exact memory reference to an action does not convert that action into speech.
Authored approximate quotes retain their own words without canonical replacement.

Record audience/perspective labels use the same author union gate as prose.
Observation and turn audiences govern character perception; they do not withhold
an admitted author's source account. Environment, event and transition source
documents retain authored status, including drafts, as the source search does;
their inclusion does not assert that an effect occurred. Knowledge claims require
canonical status, and scene/conversation evidence requires active/closed status.

Structured rows add owner `status`, exact `at` and optional `until` (both nullable),
`memberId` when authored, and a citation to the original source position. Filtering
never renumbers `effects[i]`, `transitions[i]`, `lifecycle.transitions[i]`,
`observations[i]`, `turns[i]` or `recollections[i]`. Environment citations name
`conditions/sensory`, and claim citations name `claim.statement`.

Text is assembled only from these fields; related records' current titles and
whole frontmatter objects are not appended. Integers are rendered without a
floating-point conversion. Field strings are bounded before copying, followed
by the shared complete-corpus bounds. Huge future/private fields and excluded
profile values cannot affect admitted scores or consume retrieval text budgets.
Optional null prose/status fields contribute no words. Literal nulls inside
conditions or metric values remain data. Existing raw relationship metric/facet
values retain their literal YAML payloads without new type coercion or defaults.
Authored member IDs are included in the candidate byte bound before emission.

This is the native adaptation of the pinned timed-document contract. Character
search, its fixed public training prior and the default all-time title/body
corpus remain unchanged. Search supplies neither character learning nor inferred
state changes.

## Exact thread grouping

`AuthorSearchSelection.thread_ids = Some(ids)` requires **1–32 sorted unique exact declared thread IDs**. Empty lists, duplicates, labels, aliases, case changes, and undeclared IDs fail. Membership matches any selected thread. A world record's thread declarations are not that record's memberships.

Threads are grouping labels applied as a stable subsequence **after both lanes rank the complete admitted corpus and before the output limit**. They do not remove training material, alter BM25 statistics, change lane ranks or scores, or grant knowledge. Description results use their owner's memberships. A thread with no matching rows returns an honest empty subsequence; it does not bypass a failed or oversized fit.

The pinned implementation groups a configured ranked candidate window. Native retrieval already ranks complete bounded lists, so its explicit adaptation uses that complete admitted window: select matching thread rows, preserve their original order and scores, then take the requested limit. The response records `candidateWindow: "complete-admitted-corpus"`. There is no fallback to another lane or corpus when grouping yields few results.

With only a thread selection, native all-time text and ranking semantics are preserved. In particular, legacy lexical kind filtering uses global all-time BM25 statistics and negative BM25 result scores; a thread-only lexical request retains those same statistics and scores in a private in-memory table. The complete all-time lexical corpus is therefore charged before kind and thread grouping on this compatibility path. All-time semantic/hybrid and every explicit dated mode apply kind admission before statistics and budgets. Thread-only retrieval adds no dated-description lane to the established all-time corpus.

Selected author responses add:

| Field | Meaning |
| --- | --- |
| `scope` | `author-as-of` with an explicit horizon, otherwise `author-all-time`. |
| `at` | Exact selected story time, or `null`. |
| `threadIds` | Exact supplied IDs, or `null`. |
| `projection` | `native-author-dated` for explicit dated retrieval. |
| `rankedTotal` | Matching ranked rows of the selected kind before thread grouping. |
| `total` | Rows after thread grouping and before the output limit. |
| `candidateWindow` | `complete-admitted-corpus`. |

Explicit dated lexical scores use the shared ranker's positive `-BM25` convention and lane metadata. Lexical mode does not fit LSA and reports effective dimension zero in the static retrieval settings. The no-selection response remains exactly the established response.

## Character candidates and training prior

Character semantic and hybrid candidates are exactly the existing `Evidence::documents()` projection for the selected viewer and story time. The same safe item identities and values that lexical character search can return are used by the new modes. No source title, ordinary record body, source path, blob identifier, global fingerprint, revision identifier, or author-only metadata is added to a character candidate. Candidate text and queries are projection targets; they never train the LSA vocabulary, IDF values, components, or effective dimension.

The character LSA basis comes from a separate, explicitly public prior. It contains native description sections that satisfy all of these rules:

- The owner is an unqualified `world`, `location`, or `object` record with canonical, active, or closed status. Explicit owner `valid` or `time` fields disqualify it.
- If the owner declares `audience`, that list admits `public`. If it declares `perspectives`, that list admits `ordinary`.
- The section explicitly declares a timeless validity, an audience containing `public`, and perspectives containing `ordinary`.
- Within this eligible timeless public set, a section replaced by another eligible section is suppressed using the native description correction relation. Ineligible private or dated sections do not suppress eligible public training text.

Only the selected section's own heading and Markdown enter training. Owner titles, inherited bodies, aliases, metadata, dates, and identifiers do not. The prior is fixed by that public source selection: changing the querying viewer, story horizon, private knowledge, or candidate set does not alter its training statistics.

An otherwise unknown owner may contribute eligible public training text. That contribution **does not make the owner or section a recall candidate** and does not grant character knowledge. Consequently, changing selected public prior text can legitimately change character semantic rankings, while changing excluded private or future material cannot alter the prior or the admitted candidate corpus.

If no eligible public prior exists, character semantic and hybrid search return an explicit unavailable error, including when there are no candidates. They do not train on private candidates or silently return lexical-only results. A nonempty prior that contains no usable features has a zero semantic projection. With a usable prior and no admitted candidates, the result is honestly empty.

This public native-description prior is a deliberate source-model adaptation. The pinned Python implementation uses selected public ordinary **body sections** on the same limited owner kinds. Rust retains its rule that ordinary bodies do not become character prose or training through a compatibility adapter.

## Numerical model

The native model performs these steps deterministically:

1. Split text into runs of Unicode alphanumeric characters, underscores, ASCII apostrophes, or ASCII hyphens. Convert tokens with Rust's Unicode lowercase operation and remove the pinned English stopword set.
2. Count retained unigrams and adjacent retained-token bigrams. Rank vocabulary terms by descending training-document frequency, then descending total training frequency, then the term itself. Retain at most 8,192 terms.
3. Assign smoothed inverse-document frequency `ln((1 + N) / (1 + df)) + 1` and logarithmic term frequency `1 + ln(count)`. Normalize each resulting sparse TF-IDF row.
4. Form the training document Gram matrix `A Aᵀ`. A deterministic cyclic Jacobi eigensolver obtains its eigensystem. Recover right singular components with `Aᵀu / sqrt(lambda)` and retain the largest components in stable order.
5. Project the query and all admitted candidates through the resulting components, normalize their vectors, and compute exact cosine similarities over those vectors.

`N` is the number of distinct training strings. Repeated training strings do not multiply their fitting weight. Their source occurrences and bytes still count toward admission. Candidate duplicates remain separate evidence items with distinct stable keys.

The requested dimension is 192. The effective dimension is `min(192, max(1, min(N - 1, F - 1)))`, where `F` is the selected feature count. One-row or one-feature corpora use the bounded normalized TF-IDF identity projection. Numerically zero singular components remain zero; they do not introduce arbitrary learned directions. A featureless corpus uses an empty-feature matrix shape and produces zero query projections.

The eigensolver uses `f64`, fixed iteration order, at most 32 sweeps, and an explicit off-diagonal convergence tolerance. Nonfinite values or nonconvergence return an unavailable-fit error. There is no hash-vector substitute, random projection substitute, lower-dimensional retry, or model-service fallback.

This is functional LSA parity rather than identical floating-point output. The pinned implementation uses randomized truncated SVD with a fixed seed and seven iterations. The native eigensolver, numerical precision, and tie order can produce different scores. The native tokenizer uses Unicode lowercase, not Python's full Unicode casefold; it does not apply Unicode normalization. For example, accented upper/lowercase letters match, composed and decomposed spellings can tokenize differently, and `Straße` is not automatically equivalent to `STRASSE`.

## Ranking and output

Both new modes rank complete admitted candidate lists before applying the requested result limit. Neither lane takes candidates from the other lane. The new lexical lane uses the established literal-word `AND` expression, SQLite `unicode61`, and BM25; it weights author titles by four and body text by one. Character titles are empty. Lexical statistics are computed over only that request's admitted candidates.

Semantic candidates are sorted by descending cosine and then their stable key. As in the pinned implementation, **zero and negative cosine scores remain ranked candidates** when the query has a nonzero projection. They are similarities, not confidence values. A zero query projection produces an empty semantic lane.

Hybrid fusion adds `1 / (60 + rank)` for each lane in which an item appears; ranks start at one. A result supported by both lanes receives both contributions. Final ties use the stable document key. Full lane ranks are independent of the requested output limit, so requesting fewer results does not change their scores. Unlike the pinned configurable candidate windows, this bounded native implementation ranks the complete admitted lists.

When an out-of-vocabulary query has genuine lexical matches, hybrid can return those matches with an empty semantic lane. This is a successful zero-projection outcome. An unavailable or over-budget semantic model instead fails the hybrid request.

New-mode results add:

| Field | Meaning |
| --- | --- |
| `mode` | `semantic` or `hybrid`. |
| `retrieval` | Static LSA/provider/solver/fusion settings and the training-derived effective dimension. It includes no corpus hash, training count, source identity, or character-global fingerprint. |
| `total` | Size of the ranked candidate union before the requested output limit. |
| Row `score` | Cosine for semantic mode; reciprocal-rank fusion score for hybrid. Higher sorts first. |
| Row `lexicalRank`, `lexicalScore` | Nullable lexical rank and positive `-BM25` score. |
| Row `semanticRank`, `semanticScore` | Nullable semantic rank and cosine score in `[-1, 1]`. |

Author rows retain source identity fields; character rows retain their existing safe kind, record/member identity, and projected value. Hybrid keeps the lexical snippet when a row has a lexical match. Otherwise, the snippet is the first 300 Unicode characters of admitted candidate text.

## Complete admission and resource bounds

Queries remain nonblank, at most 4,096 UTF-8 bytes and 64 whitespace-delimited words. Result limits remain 1–100. All selected input is admitted before a fit, zero-query shortcut, or result truncation:

| Bound | Maximum |
| --- | --- |
| Training occurrences plus candidate documents | 1,024 |
| Distinct training texts | 512 |
| Query plus training text and candidate key/title/text bytes, including dated member-ID bytes | 524,288 UTF-8 bytes, including one conservative separator byte per candidate |
| Selected vocabulary | 8,192 features |
| Distinct feature keys across processed unique texts | 65,536 |
| Sum of distinct feature entries across processed unique texts | 131,072 |
| Summed feature-key Unicode character count | 1,048,576 |
| Dense right-component allocation | 16,777,216 bytes |
| Prospective query and target vectors | 4,194,304 bytes |
| Sparse component-building plus all query/target projection work | 268,435,456 selected-nonzero × dimension units |
| Sparse Gram work | 268,435,456 training-row × training-nonzero units |
| Jacobi work index | `32 × N³ ≤ 4,294,967,296`; at most 32 sweeps |

The sparse work bound charges component construction and candidate/query projection together. Repeated candidate texts still count for their actual repeated projections. The Gram and eigenvector matrices each have at most `512 × 512` `f64` values. Work indexes are implementation bounds, not elapsed-time or FLOP promises.

Adapters check selected source fields before constructing large owned inputs; the shared core checks its complete supplied inputs before dense matrices. Excluded records, private description sections, and withheld character candidates contribute neither fit statistics nor retrieval budgets. Ordinary whole-world source validation remains a separate prerequisite.

Oversize inputs fail as a whole with a generic bound error. The implementation does not truncate the corpus, silently reduce dimensions, or return a partial success. A small result limit does not reduce the complete input admitted by the query.

## Source and cache behavior

Semantic and hybrid fits, lexical tables, query vectors, and candidate vectors live only for the request. Every explicit author horizon or thread selection, including lexical mode, also uses request-local tables. They read the supplied `World` snapshot and write no source, model file, author FTS index, or persistent cache. Repeated requests recompute from the current admitted inputs. There is no stale-model identity or warm-cache path to bypass admission.

The established lexical author route still uses its exact-snapshot disposable SQLite index and existing fingerprint/revision checks. Selecting a new mode neither creates nor modifies that index. Character retrieval continues to use request-local admitted evidence.

## Pinned evidence and verification

The functional reference is [`Sornaensis/wedl@f0e885ccebc279eff65a5db2984d01ebc6647b2b`](https://github.com/Sornaensis/wedl/tree/f0e885ccebc279eff65a5db2984d01ebc6647b2b). Relevant primary contracts are:

- [`vectors.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/vectors.py): feature selection, TF-IDF, LSA fitting, projections, and input admission.
- [`search.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/search.py): separately admitted search lanes, cosine ranking, public training selection, and reciprocal-rank fusion.
- [`profiles.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/profiles.py) and [`lexical.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/lexical.py): provider defaults, dimensions, fusion configuration, and stopwords.
- [`search-profiles.md`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/docs/guides/search-profiles.md): fixed public character prior, training-only semantics, and unavailable-model behavior.
- [`thread_filter.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/thread_filter.py) and [`test_thread_search_filter.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/tests/test_thread_search_filter.py): exact normalized declared thread IDs, stable post-ranked ANY grouping, no rank/training changes, and horizon admission before corpus statistics.
- [`query.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/query.py), [`description_runtime.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/description_runtime.py), and [`conversation.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/conversation.py): explicit author horizons, body bounds, source description selection, and separate hypothesis opt-in.
- [`api_schemas.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/api_schemas.py): search horizon and scope response fields and bounded catalogue thread selection. The native `wedl-rust/v1` shape remains explicit rather than adopting another protocol's aliases.

Native numerical tests check eigensystem reconstruction, orthogonality, determinism, nonconvergence rejection, Unicode token boundaries, tiny/featureless/OOV behavior, actual nonliteral LSA recall, candidate independence from training, stable ties, rank-fusion arithmetic, and complete bounds. Author tests check unchanged lexical delegation, corpus filtering before statistics and budgets, hypothesis exclusion, metadata exclusion, exact source validation, repeated/limited-result stability, and no index writes. Character tests cover public-prior selection, corrections, unknown-owner training without recall grants, candidate privacy, and explicit unavailability. Transport tests exercise strict mode parsing and direct/HTTP/CLI parity.

Dedicated author-selection tests add exact large clocks and order ties, foreign timelines, future-body rank/fit invariance, inclusive environment bounds, closed versus open aggregates, explicit transition timing, scoped description corrections and provenance, unchanged all-time delegation, stable thread subsequences and scores, complete-window bounds before grouping or output limits, strict thread validation, and source/cache immutability.
