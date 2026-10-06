# Documentation cleanup dispositions, 2026-10-06

This is the static custody reconciliation for the 69 tracked documentation inputs
at Git revision `996f5d18b4d982fd67c777ff833c667d80e83e29`: 55 Markdown files,
nine JSON receipts and five YAML vectors. It records the migration destinations
and reasons, rather than current decision authority or status. Read architecture
and its evolving history through ADRAI in the WEDL development/source checkout;
use the [documentation entry point](../README.md#architecture-with-adrai).
All listed original paths were retired after their retained material was
reconciled. The duplicate `docs/ADR.md` bridge was retired without removing
the seven preexisting decisions, their aliases, approval history or relationships.

Owner: repository maintainers. Retained guides and story material remain
maintained documentation; manuscripts, dated reports, raw measurements and
architectural history remain durable provenance until a separately authorized
replacement or retirement preserves unique content and custody obligations.
Vectors remain executable contract evidence outside docs. This reconciliation
records the completed migration and grants no future deletion authority.
The [fourteen-report retention ledger](retention-and-dispositions.md) preserves
the detailed report fingerprints, owner, retention and inherited custody limits.

## Exact original-path mapping

| Original path | Retained material | Reason |
| --- | --- | --- |
| `docs/ADR.md` | CLI discovery/history of the seven preexisting decisions | Retired the duplicate Markdown decision bridge; preserved stable aliases and managed history. |
| `docs/ARCHITECTURE.md` | ADRAI A01M48RX5WAQT5ECH66KTCFVC0T; ADRAI A01M48S4ZSCWHKWP8ZXFZ3HQJB2 | Current shared-world contract separated from the historically approved Haskell/Elm proposal. |
| `docs/CONTINUITY_SCHEMA_CONTRACT.md` | ADRAI A01M48NJH26YJTG9XWA0SKCPR2Z | Withdrawn continuity rationale retained with its original approval and supersession history. |
| `docs/THREAD_SCHEMA_CONTRACT.md` | ADRAI A01M48RX5WAQT5ECH66KTCFVC0T | Architectural shared-world/thread and quarantined recovery contract. |
| `docs/SEMANTICS.md` | ADRAI A01M48RYDXG6N84N3XSTH44WS81 | Architectural replay/time/perspective semantics. |
| `docs/SOURCE_FORMAT.md` | ADRAI A01M48S0B8N1M7HRABA7YSM5ZR7 | Architectural source format and compatibility contract. |
| `docs/SQLITE_MODEL.md` | ADRAI A01M48S1T65PSN638XCB59539Q8 | Architectural projection/storage contract; current SQLite v15 and downstream index versions reconciled. |
| `docs/CONTEXT_BRIEFS.md` | ADRAI A01M48RRCERE8D2MG7AZ9CD5JQ5; `docs/guides/context-briefs.md` | Architecture transferred to ADRAI; practical commands retained in the guide. |
| `docs/CONVERSATIONS.md` | ADRAI A01M48RVATM4A6XDWHY9S6Z4ESB; `docs/guides/conversations.md` | Architecture transferred to ADRAI; practical commands retained in the guide. |
| `docs/SEARCH_PROFILES.md` | ADRAI A01M48RW32X6AEHK2376V4YH3KP; `docs/guides/search-profiles.md` | Architecture transferred to ADRAI; practical commands retained in the guide. |
| `docs/EVENT_CONSEQUENCES_CONTRACT.md` | ADRAI A01M48VHYA0RGNJWBFZJZMGVZX6 | Architecture and preview/batch invariants transferred. |
| `docs/EVENT_CONSEQUENCE_PREVIEW.md` | ADRAI A01M48VHYA0RGNJWBFZJZMGVZX6; `docs/guides/event-consequence-preview.md` | Architecture separated from practical preview usage. |
| `docs/EVENT_CONSEQUENCES_READER.md` | ADRAI A01M48VQ7R714VYPET6C6DSHN2Z; `docs/guides/event-consequences-reader.md` | Reader architecture separated from practical read usage. |
| `docs/CHRONOLOGY_SCHEMA_CONTRACT.md` | ADRAI A01M48XWQD0809VQ3RD9NBYPSKB; `docs/guides/chronology.md` | Architecture transferred; practical recipes retained. Existing v0.6 admission and v0.7 compilation asymmetry recorded. |
| `docs/CHRONOLOGY_VALIDATION.md` | ADRAI A01M48XXBREJY497SNXZVCW85EC; `docs/guides/chronology.md` | Architecture transferred; practical recipes retained. Existing v0.6 admission and v0.7 compilation asymmetry recorded. |
| `docs/CHRONOLOGY_MIGRATION_CONTRACT.md` | ADRAI A01M48XY4R2CJ698Z2VQM9SG4ZZ; `docs/guides/chronology.md` | Architecture transferred; practical recipes retained. Existing v0.6 admission and v0.7 compilation asymmetry recorded. |
| `docs/CHRONOLOGY_INDEX_QUERY_CONTRACT.md` | ADRAI A01M48XYVYK43DWXQTC541WSFZK; `docs/guides/chronology.md` | Architecture transferred; practical recipes retained. Existing v0.6 admission and v0.7 compilation asymmetry recorded. |
| `docs/CHRONOLOGY_API_CONTRACT.md` | ADRAI A01M48XZJ6V1X1QSM4QEZJZS81M; `docs/guides/chronology.md` | Architecture transferred; practical recipes retained. Existing v0.6 admission and v0.7 compilation asymmetry recorded. |
| `docs/CHRONOLOGY_ROLLOUT.md` | ADRAI A01M48Y06MMHSR2E0A4R094QQ15; `docs/guides/chronology.md` | Architecture transferred; practical recipes retained. Existing v0.6 admission and v0.7 compilation asymmetry recorded. |
| `docs/examples/chronology-api-v1.yaml` | `architecture/adrai/examples/chronology-api-v1.yaml` | Executable normative vector moved with identical bytes and semantic cases. |
| `docs/examples/chronology-migration-v06.yaml` | `architecture/adrai/examples/chronology-migration-v06.yaml` | Executable normative vector moved with identical bytes and semantic cases. |
| `docs/examples/chronology-schema-v06.yaml` | `architecture/adrai/examples/chronology-schema-v06.yaml` | Executable normative vector moved with identical bytes and semantic cases. |
| `docs/examples/chronology-validation-v06.yaml` | `architecture/adrai/examples/chronology-validation-v06.yaml` | Executable normative vector moved with identical bytes and semantic cases. |
| `docs/chronology-index-benchmark.json` | `docs/reports/chronology-index-benchmark.json` | Dated raw measurement retained byte-for-byte; fresh generator output cannot overwrite it. |
| `docs/SPATIAL_SOURCE_CONTRACT.md` | ADRAI A01M48ZRCS8CP9EC5H5YH2MRVQ4 | Architecture transferred; current runtime admission separated from historical rollout framing. |
| `docs/SPATIAL_INDEX_CONTRACT.md` | ADRAI A01M48ZRZ2FB3S5BWBCBK9KS13N | Architecture transferred; current runtime admission separated from historical rollout framing. |
| `docs/SPATIAL_QUERY_CONTRACT.md` | ADRAI A01M48ZSKSZ3EKW7EZX5HDTSRET | Architecture transferred; current runtime admission separated from historical rollout framing. |
| `docs/SPATIAL_VALIDATION.md` | ADRAI A01M48ZT8WHG5XAVFBVXSZDGGMK | Architecture transferred; current runtime admission separated from historical rollout framing. |
| `docs/SPATIAL_INDEX_BENCHMARK.md` | `docs/reports/spatial-index-benchmark.md`; ADRAI A01M48ZRZ2FB3S5BWBCBK9KS13N; ADRAI A01M48ZSKSZ3EKW7EZX5HDTSRET | Original 100k synthetic measurements retained; algorithms/budgets transferred. |
| `docs/spatial-query-benchmark.json` | `docs/reports/spatial-query-benchmark.json` | Dated raw measurement retained byte-for-byte. |
| `docs/examples/spatial-source-component-v07.yaml` | `architecture/adrai/examples/spatial-source-component-v07.yaml` | Executable normative vector moved byte-for-byte. |
| `docs/GENERATIONAL_SOURCE_CONTRACT.md` | ADRAI A01M491X2B6KF0PZFZ4HJMBFSSR; `docs/guides/generational.md` | Architecture transferred; current SQLite v15/index v9 and authorized knowledge extension separated from original five-token oracle. |
| `docs/GENERATIONAL_COMPILATION.md` | ADRAI A01M491Y1RVN98VZDF318XJ1ARW; `docs/guides/generational.md` | Architecture transferred; current SQLite v15/index v9 and authorized knowledge extension separated from original five-token oracle. |
| `docs/GENERATIONAL_QUERY.md` | ADRAI A01M491YZG40BC65JPVT4JMYR7D; `docs/guides/generational.md` | Architecture transferred; current SQLite v15/index v9 and authorized knowledge extension separated from original five-token oracle. |
| `docs/GENERATIONAL_VALIDATION.md` | ADRAI A01M491ZCQ1E4HTMCF2NHN87323; `docs/guides/generational.md` | Architecture transferred; current SQLite v15/index v9 and authorized knowledge extension separated from original five-token oracle. |
| `docs/GENERATIONAL_AUTHORING.md` | ADRAI A01M491ZXP5DQ61KMG2YC66G8MM; `docs/guides/generational.md` | Architecture transferred; current SQLite v15/index v9 and authorized knowledge extension separated from original five-token oracle. |
| `docs/GENERATIONAL_BENCHMARK.md` | `docs/reports/generational-benchmark.md`; ADRAI A01M491Y1RVN98VZDF318XJ1ARW | Core-only historical measurements/failed target retained; architecture transferred. |
| `docs/generational-benchmark.json` | `docs/reports/generational-benchmark.json` | Dated raw failed-target receipt retained byte-for-byte. |
| `docs/HTTP_API.md` | ADRAI A01M494NTSZ76DE2X6NM4TBCB0C; `docs/guides/http-api.md` | Architectural guarantees transferred; current operational commands and observed limitations retained. |
| `docs/LLM_AND_COMMAND_PROTOCOL.md` | ADRAI A01M494PM59C6BG1W7K1CXXYR0H; `docs/guides/command-line.md` | Architectural guarantees transferred; current operational commands and observed limitations retained. |
| `docs/MIGRATION_AND_RECOVERY.md` | ADRAI A01M494PZVEJB05Y3RDNHAAKMWD; `docs/guides/migration-and-recovery.md` | Architectural guarantees transferred; current operational commands and observed limitations retained. |
| `docs/TESTING.md` | ADRAI A01M494QCT41GF34E92FKKHQ8AS; `docs/guides/testing.md` | Architectural guarantees transferred; current operational commands and observed limitations retained. |
| `docs/ITERATION_NOTES.md` | `docs/reports/iteration-0.4.md` | Unique 94-file authoring exercise, perspective/continuity observations and measured bottlenecks; design/future-work rationale transferred to ADRAI. |
| `docs/SEARCH_ITERATION_NOTES.md` | `docs/reports/search-iteration-0.5.0.md` | Unique query-quality probes, private marker exclusion, latency interpretation; provider/lane/model rationale and proposals transferred to ADRAI. |
| `docs/PERFORMANCE.md` | `docs/reports/performance-baselines.md` | Original profile, 4,302/10,662 stress-phase, API and synthetic spatial measurements/digests retained; algorithms/budgets/stage design transferred to ADRAI. |
| `docs/VALIDATION.md` | `docs/reports/validation-0.5.0.md` | Original 0.5.0 35-test/source/wheel/smoke/stress/migration results and Windows fixture limitations retained; formal validation/algorithm rationale transferred to ADRAI. |
| `docs/VALIDATION-0.5.1.md` | `docs/reports/validation-0.5.1.md` | Original 2026-08-20 37-test, 262-record completion, dialogue, 4,954-character context and archive/wheel byte/provenance observations retained. |
| `docs/WORKSPACE_CLEANUP.md` | `docs/reports/workspace-cleanup-2026-10-02.md` | Original 2026-10-02 execution counts/archive claims/limitations retained as history; tool/policy design transferred to ADRAI; named receipts absent at entry acknowledged. |
| `docs/FRONTIERSMEN_INTERACTION_LOG.md` | `docs/reports/frontiersmen-interaction-historical.md` | Unique act-boundary POV, secret-marker, audibility, packet-length and 294/17/224 pursuit observations retained as history. |
| `docs/FRONTIERSMEN_PERFORMANCE.md` | `docs/reports/frontiersmen-performance-historical.md` | Original 294/17/224/main195 compilation/query/API timings, response sizes and 85.6ms reuse anomaly retained; design/recommendations transferred to ADRAI. |
| `docs/BENCHMARKS.json` | `docs/reports/BENCHMARKS.json` | Raw historical measured receipt; preserve original bytes, environment/input provenance, timings, outcomes and failures. |
| `docs/SEARCH_PROFILE_BENCHMARKS.json` | `docs/reports/SEARCH_PROFILE_BENCHMARKS.json` | Raw historical measured receipt; preserve original bytes, environment/input provenance, timings, outcomes and failures. |
| `docs/MEDIUM_SEARCH_PROFILE_BENCHMARKS.json` | `docs/reports/MEDIUM_SEARCH_PROFILE_BENCHMARKS.json` | Raw historical measured receipt; preserve original bytes, environment/input provenance, timings, outcomes and failures. |
| `docs/SEARCH_PROFILE_SMOKE.json` | `docs/reports/SEARCH_PROFILE_SMOKE.json` | Raw historical measured receipt; preserve original bytes, environment/input provenance, timings, outcomes and failures. |
| `docs/FRONTIERSMEN_BENCHMARKS.json` | `docs/reports/FRONTIERSMEN_BENCHMARKS.json` | Raw historical measured receipt; preserve original bytes, environment/input provenance, timings, outcomes and failures. |
| `docs/FRONTIERSMEN_API_BENCHMARKS.json` | `docs/reports/FRONTIERSMEN_API_BENCHMARKS.json` | Raw historical measured receipt; preserve original bytes, environment/input provenance, timings, outcomes and failures. |
| `docs/COMPLETED_STORY_WALKTHROUGH.md` | `docs/stories/ash-archive/walkthrough.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/NARRATIVE_WALKTHROUGH.md` | `docs/stories/ash-archive/historical-unresolved-walkthrough.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/STORY_AUTHORING_NOTES.md` | `docs/stories/ash-archive/authoring-notes.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/THE_ASH_ARCHIVE.md` | `docs/stories/ash-archive/novella.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/FRONTIERSMEN_AUTHORING_NOTES.md` | `docs/stories/frontiersmen/authoring-notes.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/FRONTIERSMEN_NARRATIVE_WALKTHROUGH.md` | `docs/stories/frontiersmen/walkthrough.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/FRONTIERSMEN_WORLD_GUIDE.md` | `docs/stories/frontiersmen/world-guide.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/THE_FRONTIERSMEN.md` | `docs/stories/frontiersmen/chronicle-through-pursuit.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/examples/CHANGESET_EXAMPLE.md` | `docs/stories/ash-archive/seed-changeset.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/examples/EXPECTED_QUERIES.md` | `docs/stories/frontiersmen/queries.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/examples/MANIFEST.md` | `docs/stories/ash-archive/seed-manifest.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/examples/README.md` | `docs/stories/ash-archive/seed-overview.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |
| `docs/examples/STORY_GUIDE.md` | `docs/stories/ash-archive/seed-story-guide.md` | Unique narrative/manuscript/transcript/ID/workflow retained; architectural material transferred to nonnormative ADRAI case history where present. |

## Factual reconciliations and custody limits

Current maintained examples contain 262 Ash Archive records and 309 Frontiersmen
records in each legacy and v0.7 package. The Ash seed's 119 non-world IDs plus
world (120 records, tick 121), the unresolved 238-record/tick 178 snapshot and
the completed closed tick-208 story remain distinct. Historical Frontiersmen
294/17/224/tick-195 measurements and 307/308 reconstruction claims remain
historical; they were not replaced by current 309/19/248/tick-210 facts.
Both manuscripts, all nine raw JSON receipts and five relocated YAML vectors
retain their original bytes. The seed changeset's historical JSON and all
119 manifest ID/key mappings remain intact; current usage guidance does not
promise applying that seed payload to the completed scene.

Architectural freshness corrections record SQLite v15 and generational index v9
(with v8 historical), the six-token v0.7 registry and the separately authorized
2026-10-05 knowledge extension. They preserve the original approval-stage
five-token vectors and dates. Existing public thread catalogue/filter admission
is v0.5/v0.6 even though approved canonical thread semantics are inherited by
v0.7. Chronology compilation accepts v0.6/v0.7 while advertised capability,
catalogue definitions and replace authoring remain v0.6; some compiled reads
also work on v0.7. These are documented implementation boundaries, not repairs
or changes to approved semantics.

The generational historical warm query of 2626.816 ms still misses its 250 ms
target. Other historical measurements, 35/37-test receipts, limitations and
failures remain dated evidence rather than a present release qualification.
Fresh benchmark output requires a caller-owned fresh destination; the archived
chronology receipt is not a generator output target.

The entire preexisting `output/repository-cleanup-20261002` tree was protected.
The historically named `root-receipts.zip` and
`legacy-retirement/diagnostics.zip` were absent at entry to this cleanup.
The earlier report's archival claims remain historical, with that inherited
custody limitation explicit; no archive was restored or claimed reverified.
No preexisting output, registered worktree, story source or retained receipt
was retired by this migration.

