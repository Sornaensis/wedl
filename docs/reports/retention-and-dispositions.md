# Historical report retention and exact-path dispositions

This is the concrete custody reconciliation for the fourteen tracked report
inputs inspected at source revision `4d81ccc3c3cfc52381396c24458866cf1e6fb2ad`. It records destinations and
provenance; policy authority remains ADRAI A01M49735W4D3CZ2PJ129HTJCGG and
A01M3Y3S5QMJWMBMQT42HDQPDJS. Architectural excerpts are in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z.
Read decisions with `adrai --repo WEDL_SOURCE_CHECKOUT show ADR_ID --json` in
the WEDL development/source checkout.

Owner of the retained reports/raw JSON: repository maintainers. Scope: the exact
fourteen paths below and their listed destinations. Retention: durable historical
provenance until a separately authorized replacement/retirement reconciliation
preserves every unique outcome, failure and custody obligation. No age-only
disposal is applied. Six JSON files retain the original SHA256 exactly; prose
retains measured observations with architectural material moved to ADRAI.

| Original path | Retained destination | Reason |
| --- | --- | --- |
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

## Original input fingerprints

These fingerprints identify the original pre-transfer inputs; the six JSON fingerprints also identify the retained output bytes.

| Original input | SHA256 |
| --- | --- |
| `docs/ITERATION_NOTES.md` | `504BD3AB15C91CC606DCDF1F8E3A75C80C43169240DA73A8A32BD7EF744448C3` |
| `docs/SEARCH_ITERATION_NOTES.md` | `A9F038A0A2F1CC57299409C54FDB2403D49BD6063062CAF4CCE385DD61863AC5` |
| `docs/PERFORMANCE.md` | `DA601E7F9E5BE2D9C4C5D2053A1A9E62E9393A3EC2B4F2A5903281633DD4BA57` |
| `docs/VALIDATION.md` | `7A7A869F9619B16E6E7F8B7AD0FF54F9371040F9E646783323529A7B1C11B370` |
| `docs/VALIDATION-0.5.1.md` | `B7CA2CAFEBB6C8F6D3334F00A9FA4C19C9952A7A03D63FF9BE0CA1FB006437D9` |
| `docs/WORKSPACE_CLEANUP.md` | `583E03AEA5584FF2D9C3CC82E30A09D1BA776B2DFFBFEF09FC5B0E3C5C42ECBB` |
| `docs/FRONTIERSMEN_INTERACTION_LOG.md` | `8F8CEF62673237EAACEBFDA2DC9A1039B7D5EB3CA9D393BB93D055AFB96E37FB` |
| `docs/FRONTIERSMEN_PERFORMANCE.md` | `2BEB9EBEEC5E7930230332C44EB3FC44DDBD72D478E9778E89F853B34832E67F` |
| `docs/BENCHMARKS.json` | `8658A0C82461611D62013A017B314E1AAA69A244866244691A2F967FA1550E3A` |
| `docs/SEARCH_PROFILE_BENCHMARKS.json` | `19C8D1A529735BF6BA6A32A6B506D7E239441FFF5D25AD0B925ADA03BE5CE10E` |
| `docs/MEDIUM_SEARCH_PROFILE_BENCHMARKS.json` | `75A40E828E677C446BAE3A094EAECE1267A9312E904B7200DE2368381342B843` |
| `docs/SEARCH_PROFILE_SMOKE.json` | `C406E3BACFBEE03397A9438EA413F61E9788B57785C8FC33B847B5068845284F` |
| `docs/FRONTIERSMEN_BENCHMARKS.json` | `2CBF4C6D8C80AC536EAC41E185E70727AC8F5FF698C7FA1C0FCCDA4061F4A4F2` |
| `docs/FRONTIERSMEN_API_BENCHMARKS.json` | `E23A26E8AE4054E3B399113A82C8D57A7127F34A822C650C634601EDCEC7C51F` |


## Preserved outcomes and custody limits

The 0.5.0 and 0.5.1 35/37-test results are historical, not current certification.
The old Frontiersmen 294/17/224/main195 measurements stay unchanged; maintained
packages have 309/19/248/main210. Historical 308/307 rebuild claims in related
case studies do not establish a new rebuild. No benchmark or release runner was
executed for this transfer, and no missing date/revision is invented.

`output/repository-cleanup-20261002/` remains wholly protected under its existing
owner/custody. Entry observation on 2026-10-06: directory present with p4b5f,
recovery-candidate-34dc9556-3279-4d13-8086-976161574fa5 and
test-run-4b5f0b38-26e1-46e4-b417-c95aec3a4537. Named root-receipts.zip and
legacy-retirement/diagnostics.zip are absent in this checkout. Original archive
verification claims remain historical statements; this transfer did not delete,
restore, reconstruct or reverify those receipts or infer that other output expired.

Existing chronology-index-benchmark.json, spatial-query-benchmark.json and
generational-benchmark.json under docs/reports remain unchanged domain evidence.
The historical generational warm result remains 2626.816ms against 250ms, failed.
Their existing retention is not shortened. Fresh generator output uses a new
explicit owned destination; archived receipt filenames are not reproduction targets.

This reconciliation is created before retiring original paths. The relocated
reports and exact-byte JSON are verified before old filenames are removed.
The current documentation task's separate review scratch is owned by its developer
and expires after independent acceptance/task closure; it is not retained release
evidence and grants no authority over unrelated outputs or worktrees.
