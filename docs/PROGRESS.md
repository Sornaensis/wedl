# Implementation progress

Updated 5 November 2026. The anomaly investigation identifies the exact older
library generation behind the preserved test failure and demonstrates how Cargo
can reuse that inconsistent archive. Its original producer and the earlier
source-restoration writer remain unidentified. **Estimated remaining work: 0
further named feature iterations** against the inspected pinned inventory;
causal diagnosis remains unbounded. All v1 labels, Cargo 1.0.0 and dependencies
are unchanged.

## Current anomaly investigation

The complete findings and evidence limits are in [ANOMALIES.md](ANOMALIES.md).
The source archive now also includes an adjacent
[investigation evidence directory](../../investigation/README.md).

- Compiler metadata and exact source reconstruction establish three generations:
  the saved archive predates timeline `sourceField`/`definitionYaml`; earlier
  incremental output includes the addition; adjacent/later metadata also includes
  the recorded formatting edits. All four differing source identities were
  reconstructed exactly. The frozen failing test still reproduces.
- An isolated deliberate old-archive substitution preserves the current mtime
  and Cargo fingerprints. Cargo then reuses it: a new test receives the old
  value while the existing CLI retains the new value. Incremental compilation
  is disabled throughout. A fresh target passes from the same source. The
  standalone reproducer also passed in a second new directory. This explains
  propagation, without attributing the original archive mutation.
- The local investigation wrapper ignored caller-supplied `CARGO_TARGET_DIR`.
  It now honors that setting, with override/default checks recorded. The wrapper
  is outside the WEDL package, and no retained trace makes it the historical cause.
- Source/read/write audits and a bounded native probe found no current restoration
  path. An identified helper deliberately reproduced the signature of original
  bytes and mtime with a new inode as an instrumentation control. The original
  source-failure artifacts remain unavailable. The kernel refused tracing,
  preventing syscall-level attribution; inotify observations are explicitly limited.

| Check | Result |
| --- | --- |
| Artifact inspection | 98 embedded library source records inspected; three exact source generations identified; all 43 monitored original/evidence inputs unchanged. |
| Original failure replay | Frozen test exits 101 again on missing `timelines[0].definitionYaml`; no rebuild or weaker assertion. |
| Cargo propagation control | Two successful controlled runs; in each, a newly linked test receives the old value while the existing CLI retains the new value. A fresh target passes from the same source. No WEDL code or dependencies involved. |
| Native writer/read observation | 26 checks across 19 CLI calls; normal writes change only their targets with fresh mtimes; historical reads retain explicit deletion/corruption with zero watched mutation events. |
| Metadata-restoration control | Explicit timestamp setter plus rename reproduces original bytes/mtime with a changed inode and later ctime. Intentionally injected and identified, not spontaneous. |
| Concurrent executor control | Both observers see two states and nine events; shared lock is respected; exact edits/deletion survive observer exits and a fresh call. |
| Runtime/source agreement | All 1,158 build/test/static/example hashes still match the previously tested source. No application code, registered tests, dependencies or v1 contracts changed. |

The preceding 1,823-test native suite and 216-check browser results remain tied
to that unchanged implementation. They were not rerun for documentation-only
changes. Neither a passing control nor the wrapper correction supplies the
missing historical writer attribution.

## Previous integrated review (5 November 2026)

- The pinned callable audit rehashed all **883 retained regular blobs** against
  their Git object IDs, SHA-256 digests and sizes. It regenerated **124 CLI
  leaves and 109 mounted command contracts**, alongside the inspected
  spatial-explorer, Writing, reader and client-family consumers. This cycle
  reused the retained upstream checkout; it did not refetch or run the complete
  Python suite. The implemented author helpers cover the named provider and
  starter families. See [CORRECTNESS_REVIEW.md](CORRECTNESS_REVIEW.md).
- [Literal present Writing facts](WRITING_FRAMES.md#literal-present-facts) now
  include physically present listed objects, active scene/location environments
  and the viewer's own outgoing relationship state. They use native replay,
  independent admission, exact integer/type values and complete result limits.
  The character component omits causal prose, titles and source metadata, grants
  no learning, and stays separate from the author margin. Ordinary character
  context/search are unchanged.
- [Browser history](WRITING_FRAMES.md#browser-history-and-refresh) now restores
  exact six-field frames through Back, Forward and reload, then revalidates their
  original canonical/private pins. Refresh revalidates only an unchanged,
  previously accepted active frame. Malformed or stale frames stay unusable;
  edits and newer requests discard late replies. No restoration repins or stores
  prose, families or unrelated drafts.
- A delayed initial status response previously overwrote edited or already used
  selections. Startup defaults now protect whole horizon groups and the retcon
  queue revision. A separately reproduced mobile overflow from a long unbroken
  authored world title is fixed by wrapping the heading inside its container.
- Independent native review corrected absent-candidate row accounting and
  redundant historical-set operation accounting before integration. Limits apply
  to admitted final details, with a separate placement traversal guard. A valid
  source fixture containing **10,001 effects** established the latter case's
  reachability. Independent history/startup review found no further concrete
  defect in the frozen implementation.

### Verification recorded for the integrated review

| Check | Result |
| --- | --- |
| Complete native suite | **1,823 passed; zero failed, ignored or filtered**, across **169 native executables**, plus a successful zero-test doctest (**170 summaries**). No compiler warnings. Actual Bash and PowerShell 7.4.22 completion checks ran. |
| New native coverage | **10 added** in [writing_present.rs](../tests/writing_present.rs), included above. All 17 existing Writing frame tests also pass. Focused pre-integration run: 27 passed. |
| Present-fact boundaries | Exact extrema, typed values, canonical replay order, recursive containers/cycles, operative profile leaves, missing/empty/null values, hidden-data isolation, 10,001-operation set histories, complete row/detail/byte limits and separate placement traversal bounds. |
| Browser startup | **12 grouped checks**, covering all **15 horizon groups**, queue revision, edited/used selections, a one-coordinate edit, normal untouched initialization, original pins and unsent Plans. The previous page failed 13 preservation boundaries. |
| Browser history | **68 checks**: real Back/Forward/reload, private work pins, exact signed/large coordinates, multiline UTF-8 timelines, independent tabs, malformed state, draft entries, unchanged/stale Refresh and delayed replies after edits or view changes. |
| Browser present context | **30 checks**. Rendered character/dramatic components and author margin equal native CLI output. Hidden object changes preserve the complete character component; stale frames are refused until explicit recapture. |
| Integrated contextual helpers | Replayed the prior **106-check** context/scaffold browser suite against the final binary, including exact downloads, real Plan agreement, source pins, unsent editor retention and stale-response guards. |
| Browser traffic | **216 assertions/groups across four runs**, **122 API entries, all GET**, zero script errors. The only HTTP refusals are three expected stale Writing frames (404) and one stale scaffold (409). |
| Fixture preservation | All **24 fixture files** preserve bytes and nanosecond mtimes within their read-only phases. The history, present-context and contextual-helper suites each make one explicit fixture edit, then verify preservation of that new image. |
| Visual review | Final compiled page inspected at desktop **1280px** and mobile **390px**. Expanded context and long authored world title fit without document overflow; exact tick `9007199254740993` is retained. |
| Final-source agreement | All **1,158 build/test/static/example hashes** match the pre-suite freeze. The complete build used a new target directory with incremental compilation disabled. All four final browser runs used that exact binary; its hash stayed unchanged. |
| Source/package preservation | All **873 example files**, **173 previous test/support files**, `Cargo.toml` and `Cargo.lock` remain byte-identical. Reader assets are unchanged. Five files are added: the native component, its native tests and three portable browser regressions. |
| Formatting and static checks | `cargo fmt --all -- --check`; author/reader JavaScript, all three browser regressions and the Python action example parse. **897 author/16 reader IDs**, all **386 labels**, and local Markdown file links pass. |
| Archive | Complete **1,213-file** source tree. ZIP CRC and every member are verified against final source. |

The final binary SHA-256 is
`7f180b4afa7604c910233d6efd0b97bbfa9a99e9044ca9b44d45e8e50c3a05c7`.
Earlier catalogue, action/publication, navigation and conversation browser runs
remain historical evidence; they were not rerun this cycle. The full native
suite includes their registered native regressions.

**Remaining estimate at that review: 0 further named feature iterations** against
the inspected pinned inventory. At that point, the earlier source-restoration and
compiled-artifact mismatch mechanisms remained unidentified and independently
unbounded. A clean build and
preserved files in these executions do not explain those earlier observations
or establish exhaustive upstream conformance. No speculative workaround or
weaker native assertion was introduced for them.

## Previous contextual authoring pass (5 November 2026)

- [Author context](AUTHORING_REFERENCE.md#story-choices) returns complete record,
  member and timeline choices, available starter families, native current time,
  profiles, predicates, bindings, state-key constraints and relationship metrics.
  Exact member enumeration is shared with native brief references. Presence and
  YAML scalar types remain intact; bounded results fail rather than truncate.
- [Editable starters](AUTHORING_REFERENCE.md#editable-starters) use one exact
  source fingerprint, the editor's field metadata and existing Plan preview.
  Existing-record starters must prove byte-identical no-ops with retained body
  and direct-source fields. Description/history member guidance preserves
  absent/empty arrays and supplies a fresh local ID. Organization creation uses
  explicit identity/title and the native current time.
- Family owner and capability rules are enforced by the same native helpers
  behind CLI, HTTP and browser. Invalid source, stale fingerprints, wrong owners,
  unavailable capabilities and unrepresentable typed values are refused.
  Generating a starter does not publish, compile, create caches or add defaults
  to an existing record.
- The browser downloads exact choices/Plans and preserves its unsent editor until
  explicit replacement. Source/tab/selector changes invalidate the appropriate
  results, and old responses cannot restore stale output or clear newer pending
  reads. Independent review additionally tightened complete row counts, exact
  family modes, member ownership and timeline checks in returned context.

### Verification recorded for the contextual authoring pass

| Check | Result |
| --- | --- |
| Complete native inventory | **1,813 passed; zero failed, ignored or filtered**, across **168 native executables**, plus a successful zero-test doctest (**169 summaries**). The final clean build emitted no compiler warnings. |
| New registered tests | **24 added** in three executables: eight context discovery tests, ten scaffold tests and six CLI/HTTP tests. Included in the full count. The eight existing completion tests also pass against the expanded command grammar using actual Bash and PowerShell 7.4.22. |
| Core boundaries | Exact signed coordinates and YAML scalar types, declaration presence, absent/empty members, complete result budgets, owner/capability gates, source validation, byte-identical existing-record previews and explicit organization creation pass. |
| Independent review | Four shipped worlds and **26 record kinds** pass independent context/scaffold/real-Plan/no-op/title-edit probes; native positive cases cover the remaining assertion, context-brief and existence kinds. **44/44** closed request decoder cases pass. All **642** inspected shipped fixture files retain exact bytes and nanosecond mtimes. |
| Browser contextual helpers | **106 checks**, **34 API entries, all GET**, and **11 CLI reads** plus one explicit fixture initialization. Context, family/owner choices, generated Plans and downloaded JSON agree with native results. |
| Browser failure boundaries | Zero page errors or failed requests. The sole HTTP error is the deliberate stale-source refusal (409). Incomplete/malformed context or scaffolds remain unusable; delayed responses cannot restore stale output or enable a newer pending read. Exact large ticks, source pins and unsent Plan text remain intact until explicit replacement. |
| Source preservation | All **seven browser fixture files** retain exact bytes and nanosecond mtimes during each read-only phase. One explicit title edit exercises stale-source refusal before preservation of the new source image. No author store, cache or generated organization file appears. |
| Visual review | Contextual controls and generated output inspected at desktop **1280px** and mobile **390px**, with no mobile document overflow. Tick `9007199254740993` remains fully visible and exact. |
| Source/package preservation | All **873 example files**, **169 previous test/support files**, `Cargo.toml` and `Cargo.lock` remain byte-identical. The existing completion fixture changes only for the expanded CLI grammar. Reader assets remain unchanged. |
| Final-source agreement | All **1,153 build/test/static/example hashes** match the snapshot captured before the suite. The clean build used a new target directory with incremental reuse disabled; the final browser replay used its exact resulting binary, unchanged before and after. |
| Formatting and static checks | `cargo fmt --all -- --check`, author/reader JavaScript and the Python action example parse. **897 author/16 reader IDs**, all **386 labels**, and **567** local Markdown file links pass. |
| Archive | Complete **1,208-file** source tree; ZIP CRC and every member verified against final source. |

That pass's final browser replay used binary SHA-256
`588353ce1db3da9638c76f506e5879e2f6b767e6f9bdc1019ad422457f5be71b`.
Earlier catalogue, action/publication, navigation and conversation browser suites
below remain historical evidence and were not rerun this cycle.

### Preserved compiled-artifact mismatch

The first full run in the reused target directory failed the exact timeline
`definitionYaml` round-trip assertion even though the focused run and CLI
returned that field. Replaying the saved test reproduced the failure. Its
linked context implementation and the library archive lacked the timeline
serialization present in the saved CLI and both retained matching incremental
objects. Source hashes remained unchanged. The failing executable, library,
objects, Cargo metadata, source snapshot and replay evidence are preserved in
`wedl-build-mismatch-evidence.zip` alongside the source deliverable.

No production fix or weaker assertion was introduced for that mismatch. The
unchanged source passed the complete isolated build and browser checks recorded
above. The archive/content disagreement is demonstrated; the responsible writer
or mechanism is not identified. An incremental-directory cleanup warning from
a different test in the first run does not establish its cause. This evidence
does not explain the earlier exact-byte/mtime source-restoration observations.
Their diagnosis remains independently unbounded. The **one-cycle estimate at
that time** covered the integrated review recorded above; it did not bound the
restoration or artifact-mismatch diagnosis.

## Previous static authoring guidance and completion (5 November 2026)

- [Native authoring reference](AUTHORING_REFERENCE.md) supplies a self-contained
  Plan JSON Schema, field/default/status guidance for 28 creatable kinds and the
  update-only world, and native generational transition choices. The reference
  uses the same field metadata and vocabularies as editor/history validation.
- Repository-free CLI and shared HTTP reads return identical guidance without
  source pins or writes. A running author session can read it even when its
  current story is invalid or unavailable. The browser selects record guidance
  and downloads the complete reference while preserving the unsent Plan and
  exact horizon. Independent request epochs discard old responses.
- [Shell completion](COMPLETION.md) derives commands, aliases, options, arity and
  fixed choices from the actual Clap grammar. It never invokes WEDL or reads
  authored data. Review corrected Bash Readline fragment/quote handling and
  PowerShell preservation of a literal following an earlier expression.
- Schema acceptance remains structural guidance. Native decoding and preview
  still enforce UTF-8/aggregate bounds, duplicate keys, native IDs, retained
  YAML/JSON and world-dependent rules. Source-bound discovery and editable
  scaffolds remain the next slice.

### Verification recorded for the static authoring pass

| Check | Result |
| --- | --- |
| Complete native inventory | **1,789 passed; zero failed, ignored or filtered**, across **165 native executables**, plus a successful zero-test doctest (**166 summaries**). No compiler warnings or errors. |
| New registered tests | **19 added** in three executables: six schema/editor metadata tests, five CLI/HTTP tests and eight completion tests. Included in the full count. |
| Focused regression runs | **118 passed** across ten executables covering schema/editor/history, CLI/HTTP and completion. Completion used actual Bash and PowerShell 7.4.22; both runtime checks also ran in the full suite. |
| Independent schema validation | The emitted schema passes Draft 2020-12 meta-validation, **118 decoder/schema cases** and **2,246 exact ordinal boundary/random cases**. Nine structural accepts rejected by native decoding match explicitly documented native-only checks; no unexpected mismatch. These are separate from the native test count. |
| Independent shell review | **22 real interactive Bash Readline insertions** and **five additional PowerShell cases** pass, with zero WEDL invocation. The final PowerShell regression retains the exact current-token preservation assertion. Host expression contexts that bypass the native callback remain outside its guarantees. |
| Browser reference | **40 checks**, **11 API entries, all GET**, and **three CLI reads** plus one explicit fixture initialization. CLI, HTTP, rendered kind guidance and downloaded JSON agree exactly. All 29 record kind choices are present. |
| Browser failure boundaries | Zero page errors, HTTP failures or failed requests. Delayed responses cannot restore departed output or enable a newer pending request. Malformed/incomplete references keep download disabled. Unsent Plan text and exact tick `9007199254740993` remain unchanged. |
| Source preservation | Browser reads preserve every fixture file's exact bytes and nanosecond mtime without creating an author store or runtime cache. CLI/HTTP regressions additionally cover absent repositories, injected Git configuration, invalid/deleted source and closed query/method refusals. |
| Visual review | Reference controls, kind guidance and exact horizon inspected at desktop **1280px** and mobile **390px**, with no mobile document overflow. |
| Source/package preservation | All **873 example files**, **167 previous test/support files**, `Cargo.toml` and `Cargo.lock` remain byte-identical. Existing reader assets are unchanged; the author page gains the reference panel. |
| Final-source agreement | All **1,147 build/test/static/example hashes** match the snapshot captured before the full suite. The final browser replay uses the same resulting binary. |
| Formatting and static checks | `cargo fmt --all -- --check`, author/reader JavaScript and the Python action example parse. **877 author/16 reader IDs**, all **382 labels**, and **563** local Markdown file links pass. |
| Archive | Complete **1,202-file** source tree; ZIP CRC and every member verified against final source. |

The final browser replay used binary SHA-256
`595c308da36e5ad4cac331c98f4232639f113357a1fdcdd2a98c63780b25b813`.
Its startup verifies the exact fixture fingerprint and world title. Earlier
catalogue, action/publication, navigation and conversation browser suites below
remain historical evidence and were not rerun this cycle. The earlier failing
restoration artifacts remain unavailable; this clean run does not identify their
responsible writer or close that observation. The two-cycle estimate covers the
remaining contextual helpers and review, with restoration diagnosis separately
unbounded.

## Previous integrated correctness and scope review (5 November 2026)

- [Character catalogue citations](CATALOGUE.md) now identify actual owner records
  and source fields for observations, speech, actions and recollections. Original
  array indexes survive admission and sorting; descriptions retain real section
  targets. Ordinary character JSON gains no source-index metadata.
- Revision source reads remove inherited Git environment overrides and explicitly
  disable lazy fetching. Isolated reproductions established trace-file writes
  and missing-object fetches before the fix. New subprocess regressions preserve
  complete fixture bytes and mtimes on successful and refused reads. These
  defects do not explain the earlier exact-byte/mtime restorations.
- Status and capability warnings now describe implemented consequence/revision,
  Writing, manuscript and reader behavior accurately while retaining partial
  validation and noncertification.
- The [fresh pinned scope inventory](CORRECTNESS_REVIEW.md) found nine schema and
  nine scaffold helpers plus Bash/PowerShell completion. Those functions remain
  required; existing typed editors, static guides and example generators do not
  replace them. Native helpers can consolidate the upstream interfaces and reuse
  existing codecs, source captures and reviewed publication.

### Verification recorded for the integrated review

| Check | Result |
| --- | --- |
| Complete native inventory | **1,770 passed; zero failed, ignored or filtered**, across **162 native executables**, plus a successful zero-test doctest (**163 summaries**). No compiler warnings or errors. |
| New registered tests | **Five added** in two executables: two catalogue citation tests and three Git environment/missing-object tests. Included in the full count. |
| Focused regression runs | **98 passed** across ten executables covering catalogue/character admission, source contracts and action history. The final citation control also passed before the full run. |
| Independent review | Both production fixes received a separate read-only cross-review. No additional issue was found. Original source ordinals remain internal except for intended catalogue citations. |
| Browser catalogue | **178 checks**, **35 API entries, all GET**, and **28 CLI reads**. Exact native/HTTP/rendered agreement now includes observation, speech, action and recollection citations following hidden siblings. Existing pagination, admission, delayed-response and stale-source assertions remain. |
| Browser failure boundaries | Zero page errors. The sole HTTP error is the deliberate stale-source refusal (409). Initial harness input order was corrected to enter a query before selecting the disabled relevance option; no behavioral assertion was weakened. |
| Source preservation | All **18 browser fixture files** retain exact bytes and nanosecond mtimes through reads. One explicit fixture title edit tests refresh, followed by preservation of the new source image. No author store or cache is created. Git regressions additionally inspect fixture and object-store contents/mtimes. |
| Visual review | Catalogue controls/results inspected at desktop **1280px** and mobile **390px**. Tick `9007199254740993` remains fully visible and exact; no mobile document overflow. |
| Fresh scope evidence | Exact pinned commit retrieved; **883 source/documentation blobs** rehashed against Git tree entries. Inventory covers **124 CLI leaves / 109 mounted command contracts**, plus spatial-explorer, Writing and reader routes. No Python full-suite claim. |
| Source/package preservation | All **873 example files**, **165 previous test/support files**, static assets, `Cargo.toml` and `Cargo.lock` remain byte-identical. Five production files change and two regression files are added. |
| Final-source agreement | All **1,142 build/test/static/example hashes** match the snapshot captured before the full suite. The final browser replay uses the same resulting binary. |
| Formatting and static checks | `cargo fmt --all -- --check`, author/reader JavaScript and the Python action example parse. **869 author/16 reader IDs**, all **381 labels**, and **547** local Markdown file links pass. |
| Archive | Complete **1,195-file** source tree; ZIP CRC and every member verified against final source. |

The final browser replay used binary SHA-256
`7dd09e09f92a5f566b7f15645e0676187efa17f4b5354f3a9c661f2d7df23645`.
The current catalogue browser harness checks its exact fixture fingerprint and
world title at startup. Earlier action/publication, navigation and conversation
browser suites below remain historical evidence and were not rerun this cycle.
Original failing restoration artifacts are no longer present in the working
environment; retained notes and later clean runs cannot identify their writer.
The updated estimate accounts for concrete callable helpers, not a percentage
inferred from test counts or source size.

## Previous saved-action and correctness pass (5 November 2026)

- [Outcome review](OUTCOMES.md#original-evidence) resolves draft and selected
  proposal citations against authenticated original Git worlds. Record/member,
  owner path and blob must match exactly. Ordered complete-owner byte digests
  bind the report, review and applied receipt; current files cannot substitute.
- Historical captures share complete record, source and YAML budgets across at
  most 64 revisions. Empty evidence remains Git-free. First application rechecks
  originals before writer state and under the lock. Terminal replay/verification
  retain the approval-time digest and their existing publication-only scope.
- [Private hypothesis links](ACTIONS.md) are required ordered unique record
  arrays, checked against actual hypotheses during current save/review. Stored
  dangling links remain inspectable. Links obey existing phase rules and enter
  private review bindings without adding context, facts or character knowledge.
- Browser import/edit/save/load supports the links. Outcome approval checks the
  exact ordered proof and digest, and publication checks the submitted proof
  even after tab changes. Mobile action coordinates now stack so large exact
  ticks stay fully visible.

### Verification recorded for the saved-action pass

| Check | Result |
| --- | --- |
| Complete native inventory | **1,765 passed; zero failed, ignored or filtered**, across **160 native executables**, plus a successful zero-test doctest (**161 summaries**). No compiler warnings or errors. |
| New registered tests | **27 added**: nine evidence core, nine independent evidence admission and nine hypothesis-link tests. All are included in the full count. |
| Focused regression run | **164 passed** across 13 executables, including existing action/history/publication/reader/revision coverage. |
| Correctness regressions | SHA-1/SHA-256 original commits, exact owner/member/path/blob identity, replacement refs, missing objects, invalid original worlds, symlinks, raw whole-owner bytes, selected-only ordered evidence, complete shared bounds, receipt binding and private-link isolation pass. |
| Action/publication browser | **247 checks**, **73 API entries**, **9 browser POSTs** and **34 CLI reads**. Native/HTTP/rendered agreement, nonempty links/evidence, malformed-proof refusals, approval invalidation, delayed acknowledgement and terminal receipt proof pass. Existing publication/lost-response assertions remain. |
| Browser failure boundaries | Zero page errors. The sole HTTP error is the deliberate missing-original 404; the sole failed request is deliberate manuscript-response loss. Three malformed evidence proofs leave approval disabled. A temporarily withheld original loose Git blob is explicitly restored by the harness. |
| Source integrity | Review/refusal stages preserve canonical/private bytes and nanosecond mtimes. Approved publication changes only reviewed canonical paths and private documents. Original evidence hashes differ from live actor bytes as intended. Exact CRLF text and tick `9007199254740993` remain preserved. |
| Visual review | New hypothesis controls and applied evidence reviewed at desktop **1280px** and mobile **390px**. Full mobile action ticks remain visible, with no document-level horizontal overflow. |
| Source/package preservation | **872 of 873 example files** remain byte-identical; the action request generator adds required empty `hypothesisRefs`. **160 of 162 previous test/support files** remain byte-identical; two draft fixtures gain the field, and the unknown-field case now uses unsupported `hypothesisLinks`. `Cargo.toml` and `Cargo.lock` are unchanged. |
| Final-source agreement | All **1,140 build/test/static/example hashes** match the snapshot captured before the full suite. Final browser replay uses that same resulting binary. |
| Formatting and static checks | `cargo fmt --all -- --check`, author/reader JavaScript and the updated Python example parse. **869 author/16 reader IDs**, all **381 labels** (376 author, five reader), and **542** local Markdown file links pass. |
| Archive | Complete **1,193-file** source tree; ZIP CRC and every member verified against final source. |

The final action/publication browser replay used binary SHA-256
`aeea9a5dc0fadaefcdeeea694f44f0db54e421f38de51140b5a9abaa23f16e69`.
Required v1 field fixtures were updated directly. Initial new fixture omissions
were corrected without weakening behavioral assertions. The mobile clipping
finding was fixed before the final replay. Earlier catalogue/navigation/conversation
browser suites below remain historical evidence and were not rerun this cycle.

## Previous catalogue and correctness pass (5 November 2026)

- [Admitted catalogue](CATALOGUE.md) supplies pages and ordered requested labels
  under author all-time/as-of or exact character-in-scene views. Admission precedes
  filters, search, facets, totals and pagination. Unavailable labels disclose no
  title or kind; learned cards use remembered statements and learned evidence.
- Kind/profile/era/name filters, NFC/casefold ordering, complete-cohort author
  prominence and selected thread intersections share one bounded native read.
  Lexical, semantic and hybrid search use the complete admitted corpus within
  explicit native limits. Selected-thread grouping follows ranking.
- Continuation binds exact source, lens, horizon, selectors, page size, search
  provenance and complete selection. Overflow refuses the whole read. Browser
  selector/horizon/tab/source changes clear results and cursors; delayed responses
  cannot restore stale output or clear a newer request's pending state.
- Independent review fixed cross-timeline all-time admission, finite historical
  observations/environments disappearing from all-time search, hidden effects
  consuming text limits before admission, unfinished aggregate prose entering
  search, pinned slug matching, and learned-card selected thread memberships.

### Verification recorded for the catalogue pass

| Check | Result |
| --- | --- |
| Complete native inventory | **1,738 passed; zero failed, ignored or filtered**, across **157 native executables**, plus a successful zero-test doctest (**158 summaries**). No compiler warnings or errors. |
| New registered tests | **38 added** across three executables: 14 core catalogue, 19 independent admission and five CLI/HTTP tests. All are included in the full count. |
| Focused regression run | **85 passed** across six executables, including 47 existing author/character search regressions. Existing assertions were preserved. |
| Correctness regressions | Closed requests, Unicode/slug matching, exact timelines/scenes, acquisition/forgetting, learned/private metadata isolation, finite all-time evidence, hidden effect limits, complete facets/ranking, cursor bindings and complete-result bounds pass. |
| Catalogue browser | **138 checks**, **31 API entries, all GET**, and **24 CLI reads**. Native/HTTP/rendered agreement, continuation/reset, all three views/search modes, requested labels, thread selection, stale pins and overlapping requests pass. No page errors; the only HTTP error is the deliberate stale-source refusal (409). |
| Browser source preservation | All **17 fixture files** retain exact bytes and nanosecond mtimes through read-only stages. One explicit fixture title edit tests live source refresh; subsequent reads preserve that new source image. No private author store or cache is created. Tick `9007199254740993` and its order remain exact. |
| Visual review | Catalogue controls/results inspected at desktop **1280px** and mobile **390px**. Complete coordinates remain visible, with no document-level horizontal overflow. |
| Source/package preservation | All **873 example files** and **159 previous test/support files** remain byte-identical. Existing locked dependency versions and Cargo package 1.0.0 remain; only `unicode-normalization` 0.1.26 and its `tinyvec` 1.13.3 dependency are added. |
| Final-source agreement | All **263 build/test/static source hashes** match the snapshot captured before the full suite. The final browser replay uses the same resulting binary. |
| Formatting and static checks | `cargo fmt --all -- --check`, the author inline script and actual external reader script parse successfully. **868 author/16 reader IDs**, all **380 labels** (375 author, five reader), and **534** local Markdown file links pass. |
| Archive | Complete **1,189-file** source tree; ZIP CRC and every member verified against final source. |

The final catalogue browser replay used binary SHA-256
`7c08f607a16b03ed9622bffa3fbc390f0cd35d1e39743c0843179c7bcb3a8716`.
Review findings were corrected before the full run. The delayed-response harness
keeps interception active until both held responses are explicitly released;
the final unchanged browser pending-state assertions pass. No behavioral assertion
was weakened. Earlier browser suites below are retained historical evidence and
were not rerun this cycle.

## Previous navigation and correctness pass (5 November 2026)

- [Core navigation](NAVIGATION.md) now exposes whereabouts, richer timelines,
  causal directions and advisories, story-point trails, pair interactions and
  selected thread memberships through the same native service and browser.
  Exact coordinates, complete-result limits and admission gates are shared.
- [Whereabouts](WHEREABOUTS.md) retains explicit initial/set/clear/reaffirmation
  journeys, last-known location and timed scene presence. Prominence uses the
  full admitted cohort before character selection. Placement also recognizes
  the exact declared operative entity leaf in a bound location profile; prose,
  sibling profile fields and scene membership never invent movement.
- [Timeline and causality](TIMELINE_NAVIGATION.md) retain authored event status,
  lifecycle points and scene/conversation spans, with dated or explicit all-time
  views. Upstream, downstream and full connected-component reads preserve exact
  causes and provide continuity advisories without admitting future endpoints.
- [Story navigation](STORY_NAVIGATION.md) adds transition/outcome trails and
  explicit scene predicates. Interactions require literal shared event
  participation and retain all authored roles; thread intersections expose only
  requested records and selected threads.
- Scoped story evaluation checks typed operands and effects before replay.
  Hidden evidence cannot leak through negation, equality or replay constraints;
  visible future event/relationship predicates retain their existing boolean
  semantics. Aggregate trail and interaction payloads are bounded during
  construction as well as at final serialization.
- Browser reads discard stale responses and preserve an edited or already-used
  horizon when bootstrap completes late. Timeline, tick and order stay exact,
  including values beyond JavaScript's safe-integer range. Narrow-screen time
  inputs show the full coordinates.

### Verification recorded for the navigation pass

| Check | Result |
| --- | --- |
| Complete native inventory | **1,700 passed; zero failed, ignored or filtered**, across **154 native executables**, plus a successful zero-test doctest (**155 summaries**). No compiler warnings or errors. |
| New registered tests | **73 added** across six navigation executables: 20 whereabouts, nine timeline, 11 causal, 20 story, nine relation and four adapter tests. These are included in the full count. |
| Correctness regressions | Typed placement/equality operands, withheld effects and exclusive-group replay, future predicates under negation, exact scene presence, causal time gates, complete cohort ranking, count limits and aggregate byte bounds pass. |
| Navigation browser | **57 checks**, **27 API entries, all GET**, and **18 CLI reads**. Native/HTTP/rendered agreement, selected-cohort ranking, all three causal directions, dated/all-time views, scene conditions and delayed-response invalidation pass. Zero page errors; the only HTTP error is the deliberate complete-bound refusal (400). |
| Browser source preservation | All **16 fixture files** retain exact bytes and nanosecond mtimes. No private author store is created. Tick `9007199254740993` and its explicit order remain exact. |
| Visual review | Navigation controls/results inspected at desktop **1280px** and mobile **390px**. The mobile horizon shows complete coordinates and has no document-level horizontal overflow. |
| Source/package preservation | All **873 example files**, `Cargo.toml` and `Cargo.lock` retain their baseline bytes. Of **153 previous test/support files**, **152 remain byte-identical**; only existing HTTP operation constructors were adapted to the new optional fields, with assertions unchanged. |
| Final-source agreement | All **256 build/test source hashes** match the snapshot captured before the final full suite. Browser replay uses the resulting final binary. |
| Formatting and static checks | `cargo fmt --all -- --check`, the author inline script and actual external reader script parse successfully. **839 author/16 reader IDs**, all **366 labels** (361 author, five reader), and **524 local Markdown file links** pass. |
| Archive | Complete **1,182-file** source tree; ZIP CRC and every member verified against final source. |

The final navigation browser replay used binary SHA-256
`d2a251ab412d8e4ff932fb0685e052b2ceb1892029cf0d53dc1bc74f9c6144bf`.
Focused review findings were corrected before the final full run. No behavioral
assertion was weakened. Earlier browser publication/conversation results below
are retained historical evidence; those browser suites were not rerun this cycle.

## Previous correctness and conversation pass (4 November 2026)

- General typed record and Plan YAML is bounded before expanded allocation:
  1 MiB raw input, 1 MiB expanded UTF-8 strings including keys and alias copies,
  131,072 values/keys and depth 128. Existing safe aliases and retained-edit
  bounds remain supported.
- Writing-frame import refuses malformed UTF-8 instead of replacing invalid
  bytes and potentially selecting a different declared timeline.
- [Conversation controls](CONVERSATION_CONTROL.md) construct an active
  conversation plus reciprocal scene link, or append speech/action beats through
  existing Plans. Exact presence/order, earlier-speech interruption, prior
  structured values, body bytes and explicit unchanged clocks are covered.
  Imports retain exact literals and source pins; Actions copying is explicit.
- [CORRECTNESS_REVIEW.md](CORRECTNESS_REVIEW.md) records the pinned public-family
  audit and actual remaining navigation, admitted catalogue and saved action
  evidence/link behavior. It replaces the vague public-citation-resolver question
  with the specific original-evidence consumer.
- The restoration review confirms the retained observations but identifies no
  responsible writer. The isolated executor overlap probe was negative; no
  speculative runtime change was introduced.

### Verification recorded on 4 November 2026

| Check | Result |
| --- | --- |
| Complete native inventory | **1,627 passed; zero failed, ignored or filtered**, across **148 native executables**, plus a successful zero-test doctest (**149 summaries**). No compiler warnings or errors. |
| Focused regression run | **175 passed** across 14 executables covering conversation controls/interruption, YAML bounds, character retrieval, dated search, retcon/reveal, edits and consequences. These are included in the full count. |
| New registered tests | **28 added**: 10 conversation planner tests, five transport tests, eight interruption/admission tests and five YAML expansion tests. All **148 previous test/support files** remain byte-identical. |
| Conversation browser | **183 checks**, **20 API entries, all GET**, and **32 CLI reads**. Creation, speech/action append, interruption, exact native/HTTP agreement, imports/downloads, guarded Actions copy and stale/delayed-response invalidation pass. Zero page errors; the only HTTP error is the intentional stale-pin 409. |
| Conversation source preservation | All **nine fixture files** retain exact bytes and nanosecond mtimes. No private author store or runtime write cache is created. CRLF title/beat/text/timeline literals, omitted versus false interruption, and tick `9007199254740993` remain exact. |
| Writing UTF-8 regression | Malformed input is refused before any resolve request and leaves the original timeline selected. The valid Unicode control succeeds. Source bytes/mtimes remain unchanged and there are no page errors. |
| Existing publication browser regression | **185 checks**, **nine browser POSTs**, **34 CLI reads**, and **65 API entries** pass on the same final binary. No page errors or HTTP failures; the deliberately dropped response remains the sole request failure. Exact approval, writes, receipts and uncertain-response recovery assertions are unchanged. |
| Visual review | Conversation controls/results inspected at desktop **1280px** and mobile **390px**; readable controls and no mobile horizontal overflow. |
| Source/package preservation | All **873 example files**, previous test/support files, `Cargo.toml` and `Cargo.lock` retain their baseline bytes. No package/dependency/version change. |
| Formatting and static checks | `cargo fmt --all -- --check`, the author inline script and actual external reader script parse successfully. **810 author/16 reader IDs**, all **351 labels**, and **510 local Markdown file links** pass. |
| Archive | Complete **1,166-file** source tree; ZIP CRC and every member verified against final source. |

Both browser suites and the UTF-8 regression used binary SHA-256
`6970e03d9669352906261bde13965b4fc342133e6f8cf821597350ae8037ccfc`.
The first focused run exposed an optional-null replacement defect and two
incomplete admission fixtures; the production gate and fixtures were corrected,
then the unchanged behavioral assertions passed. No assertion was weakened.

## Previous verified browser publication

Current-worktree `serve` now provides six exact POST routes for private action,
retained edit, manuscript and retcon review saves, plus approved action/edit
application. They reuse native closed decoders, exact source/private-file pins,
complete review hashes, explicit approval, writer locks and terminal receipts.
Historical sessions and the existing immutable library router remain GET-only.

POSTs require a single canonical loopback Host, an exact matching Origin, JSON
content type and the explicit write header. Bodies retain their existing native
byte limits. One request gate coordinates bounded fresh canonical captures with
operation dispatch; native cross-process writer checks remain authoritative.
There is no raw Plan apply route or direct HTTP record/body bypass.

Canonical requests use stable, bounded current-source capture and refuse failed
or partial reads. Review caught an important exception: historical/private
operations already select and capture their own sources. Those existing reads
remain independent of malformed live canonical source, without a stale outer
fingerprint. Reader preview retains its installed presentation service and its
response-time policy check.

The browser can save, load, review, explicitly approve and apply without a
server restart. It refreshes source after a confirmed write while retaining
unsaved inputs, exact prose and their original pins. Loading the saved document
remains explicit. Global write results survive navigation and concurrent typing.

Acknowledgements are checked against their native operation, exact identity,
prior hash and relevant review/receipt or manuscript bindings. Lost or malformed
responses remain unconfirmed, block further writes until explicit refresh and
never trigger an automatic retry. A confirmed write stays successful if the
subsequent refresh fails. The last submitted request remains downloadable.
The review also fixed stale cancelled-action controls and a cached retcon
comparison revision after source refresh.

See [BROWSER_PUBLICATION.md](BROWSER_PUBLICATION.md) for the workflow, exact
routes, bounds, response scope and failure behavior. Existing downloads and
native CLI paths remain available.

## Previous publication baseline

| Check | Result |
| --- | --- |
| Complete native inventory | **1,599 passed; zero failed, ignored or filtered**, across **144 native executables**, plus a successful zero-test doctest (**145 result summaries**). No compiler warnings or test errors. |
| New registered tests | **15 added**: five complete publication workflow tests and ten transport/security/freshness regressions. All **146 previous test/support files** remain unchanged. |
| Writer workflow coverage | Private action/readiness/proposal saves, exact outcome and retained-edit publication, explicit approval/refusal, stale pins, no-op canonical outcomes, verified idempotent application retries, exact manuscript saves and retcon review saves. |
| Transport coverage | Exact six-route whitelist, mandatory single Host/Origin/write header, JSON/form/method/CORS boundaries, all six body limits, concurrent same-prior-hash saves, fresh reads and stale-save rejection, malformed/pending source refusal and independent historical reads. |
| Browser verification | **185 checks**, with **nine actual browser POSTs** and **34 native CLI reads**. Action and retained-edit reviews, exact submitted requests, published bytes and terminal receipts agree with native output. |
| Browser failure boundaries | Delayed successful write during further editing/tab departure, committed manuscript write with deliberately lost response, and a separately committed write with malformed HTTP 200 acknowledgement. Both uncertain cases preserve exact requests/pins, block writes and require explicit recovery; no automatic retry. |
| Browser request audit | **65 recorded API entries**: 53 GET entries including startup, nine browser POSTs and three forwarded POST probes. Zero page errors or HTTP failures; the sole failed request is the deliberately dropped response. |
| Source integrity | Read/review/private-save boundaries preserve canonical bytes and nanosecond mtimes. Canonical application changes only the reviewed actor and explicit new object; unrelated world/location/scene source remains exact. Private IDs stay outside canonical counts. CRLF, trailing bytes and tick `9007199254740993` remain exact. |
| Visual checks | Desktop **1280px** and mobile **390px** approval, result, uncertainty and recovery controls inspected; no horizontal overflow or clipped controls. |
| Preserved source | All **873 previous example files**, **146 previous test/support files**, `Cargo.toml` and `Cargo.lock` remain byte-identical to the starting archive. |
| Formatting and documents | Previous formatting, author JavaScript parsing, **777 author and 16 reader HTML IDs**, **333 labels** and **487 local Markdown file links** passed. This review found that the earlier second inline-script check did not parse the external reader file; the current pass checks the actual reader script. |
| Archive | Complete **1,157-file** source tree. ZIP CRC and every archived member verified against current source. |

The 348 frozen core semantic cases remain inside one registered test and are not
counted again. The functional target remains
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`. Passing current tests does not establish
unlimited domain parity or resolve the earlier observations below.

## Unresolved verification observations


The earlier action/outcome and 207-request canonical authoring replays remain
baseline evidence. **One earlier verification observation remains unresolved:**
two preliminary single-record replays saw an unrelated previously edited file
return to exact older bytes on disk. Fingerprints detected the changed source
and refused the next operation. Two later complete instrumented replays were
clean. The cause is unestablished; no retry or source workaround was added.
Those clean runs do not resolve the observation or prove power-loss durability.
Recovery tests simulate interruptions rather than exhaustive hardware failures.


The previous Writing iteration also saw two uninstrumented browser runs restore a deliberately
deleted live private work to its original bytes and mtime during a historical
read sequence. Historical payloads were correct and the deliberately corrupted
canonical file stayed corrupted. Fresh native and instrumented browser runs, plus a later complete run on a
different local port, kept the work absent; observed historical code paths contain no recovery or source
restoration call. The original failures remain unexplained. Clean reruns do not
resolve them, identify a writer or establish read-only behavior in every run.
No retry, restoration workaround or weakened assertion was added to the runtime.

The first complete Writing browser replay in the earlier typed-core/hypothesis iteration reproduced the same
work-file restoration: a deliberately deleted live work reappeared with its
original bytes and mtime, while the live canonical corruption remained. The
strict source-image assertion failed. This is an additional unresolved
observation, not a confirmed reader-side write or an established external cause.
The final isolated replay passed the same unchanged assertions on the final
binary. That clean run does not resolve the recurrence or identify its cause.

The preceding publication iteration re-examined the original failing Writing
artifact and harness.
The restored file still has its recorded exact hash and original mtime with a
later ctime, but the retained failing-run evidence does not identify the writer.
The instrumented clean replay belongs to a different run. No cause is established,
and the new stable-capture guard is not claimed to resolve file restoration.

The 4 November review also checked the frozen single-record CLI evidence and the
restored work's inode/ctime. Browser or rebuild behavior cannot explain that
whole observation family. The inspected code paths contain no historical
worktree-restoration operation, but absence of such a call and clean traces
from other executions do not identify the actual writer. A single overlapping
executor edit/deletion probe remained unchanged throughout and did not
reproduce the failure. See [CORRECTNESS_REVIEW.md](CORRECTNESS_REVIEW.md).

The archive includes implementation, specifications, plan, parity assessment and
executable examples. Temporary worlds, build outputs, reference copies,
screenshots and verification harnesses/logs are excluded.

## Remaining work and estimate

| Remaining area | Comparable iterations |
| --- | ---: |
| Integrated correctness and scope review | 1 |
| **Total confirmed work** | **1** |
| Earlier unexplained file-restoration observations | Diagnosis cannot yet be bounded |

Passing current native/browser tests does not complete integrated scope review
or resolve the restoration observations. Optional-field removal and unlisted
policy/extension data retain their documented direct-source boundary. No
migrations, compatibility layers or release infrastructure are included.
See [PLAN.md](PLAN.md), [PARITY.md](PARITY.md) and the
[scope review](CORRECTNESS_REVIEW.md) for the exact required behavior.
