+++
schema = "adrai/decision/v1"
adr = "A01M4CXZHJF4ATRPVTRE4R9DHAN"
record = "R01M4CXZHQY0FTM6444X00QHX75"
title = "Tideglass native v0.7 example contract"
summary = "Freeze the original 64-record Tideglass roster and seven current-API demonstration scenarios; approve later additive integration while preserving existing examples and contracts."
domains = ["narrative-example", "packaged-examples"]
+++

# Tideglass native v0.7 example contract

## Decision and implementation boundary

Adopt Tideglass as a new native authored example of a coastal town whose public-works cooperative repairs a storm-damaged beacon, restores ferry service and investigates an inaccurate maintenance log. The cast, setting, prose and identifiers are newly authored for this example. The plot has an administrative explanation: a copied inspection entry was mistaken for a completed repair; Jessa finds the discrepancy and the cooperative records the correction. No imported franchise material or retired-world source supplies narrative content.

This decision approves the content contract and later additive integration. At this decision's baseline, `tideglass` is NOT an implemented initializer choice. Subsequent scoped implementation adds its package and selector, then tests and executable guides. Keep the four existing public choices `ash-archive`, `ash-archive-v07`, `frontiersmen`, `frontiersmen-v07`; Ash remains the default, empty initialization remains v0.3, and the existing three-package converter and its pinned inputs remain unchanged. Do not describe an approved future selector as a working command until its integration lands.

The owned future content root is `src/wedl/data/tideglass_v07/`, with practical material under `docs/stories/tideglass/`. Architecture stays in managed ADRAI decisions. This decision does not introduce an API, alter source envelopes or waive existing gates. Existing custody, compatibility assets and unrelated work remain protected. Dedicated licensing, rights-confirmation, contribution/authorship gates and notice assets are outside this work; existing root LICENSE remains untouched.

## Source and identity

The canonical story contains exactly 64 UTF-8/LF Markdown records, all `schema: wedl/v0.7`. Total canonical story source is at most 512 KiB and narrative bodies total at most 15,000 whitespace-delimited words. Narrative prose is original, concise and practical: each record explains its authored role without embedding tracking/review logs. Additional YAML fields, transitions, effects, observations, turns and recollections are embedded values, not additional top-level records. No generated cache, Git metadata, archived source or extracted fixture is a package asset.

World capabilities in canonical order are `generational-core-v1`, `spatial-core-v1`, `geometry-v1`, `route-v1`, `overlay-v1`. Ordinary knowledge supplies privacy examples; the generational knowledge extension is not enabled. Existing current validators, record factories and closed payload grammars are authoritative. Body serialization preserves decoded body content; paths never supply identity.

Ordinary record IDs come from the existing `id_from_seed(kind, "tideglass-demo-v1/<kind>/<slug>")` factory. Standalone map, route and overlay IDs are the literal portable IDs below. Embedded IDs use the same factory with the existing auxiliary kind and seed `tideglass-demo-v1/<aux-kind>/<owning-record-slug>/<local-slug>`; local slugs are stable descriptive names, and all embedded IDs must be unique. No random IDs, Windows-reserved path components, aliases that change identity, or copied IDs.

### Exact record roster

Paths below are relative to `src/wedl/data/tideglass_v07/`.

| Kind | Title | ID | Path |
| --- | --- | --- | --- |
| world | Tideglass | `world_7XGTHK671HA355KN17X2HRET5G` | `story/world.md` |
| character | Mina Vale | `char_4JVNDN6QW0BAFFV379AJJSMEPZ` | `story/characters/mina-vale.md` |
| character | Orin Reed | `char_62PCSFAX4YQE48JK476XKGG6B8` | `story/characters/orin-reed.md` |
| character | Jessa Quill | `char_6D246WN3J75HBRJJXGQ5H6QAD4` | `story/characters/jessa-quill.md` |
| character | Tavi Moss | `char_0PYVCG3ETXZ640BWJ5JMT6XXFZ` | `story/characters/tavi-moss.md` |
| character | Cora Finch | `char_46VTBJZ9TNMTF7G14N5GKYQ9C1` | `story/characters/cora-finch.md` |
| character | Sela Vale | `char_39E9EWWC017E40519033S4RW4X` | `story/characters/sela-vale.md` |
| location | Tideglass Town | `loc_2DW2BZ5PN3XBMD2FWQA4Q61V1E` | `story/locations/tideglass-town.md` |
| location | Beacon | `loc_6ZT4TZRBGZTHMYVRDXV3HFTTPQ` | `story/locations/beacon.md` |
| location | Quay | `loc_0708459BXR2PVVHA4VASXHB69C` | `story/locations/quay.md` |
| location | Ferry House | `loc_3VDY39471DED5V26TEVYXMRQK3` | `story/locations/ferry-house.md` |
| location | Workshop | `loc_117ZGNWJM0Y3N7CWER2H3XK722` | `story/locations/workshop.md` |
| location | Archive | `loc_2SYYRXM9AHJGKYKGG200TZV82Y` | `story/locations/archive.md` |
| location | Marsh Road | `loc_7F7G4Y2CZAD32HYW0B28N61Q72` | `story/locations/marsh-road.md` |
| object | Signal Lens | `obj_4C9HNZYKJG5QQ6DWTWDJ4NWJ8D` | `story/objects/signal-lens.md` |
| object | Maintenance Logbook | `obj_0NQWK9WPK4PSR5722YH0RJ89CH` | `story/objects/maintenance-logbook.md` |
| object | Ferry Key | `obj_13BZCJF8R08S5RSH4VRJXMA0YK` | `story/objects/ferry-key.md` |
| object | Survey Staff | `obj_76VC9SVNSR7ASZK025X7PTRBJH` | `story/objects/survey-staff.md` |
| relationship | Mina trusts Tavi's repairs | `rel_7Y1Y0Z51JTGRZ02BTV2K31ERNA` | `story/relationships/mina-tavi-trust.md` |
| relationship | Orin trusts Jessa's records | `rel_4WEP0KV5C79HNMY7J2QTN67BY1` | `story/relationships/orin-jessa-trust.md` |
| relationship | Jessa trusts Sela's recollection | `rel_0Y5PE3MEG7ZK91Z24B1F6G1T31` | `story/relationships/jessa-sela-trust.md` |
| relationship | Cora owes Mina a review | `rel_600QPBZBZVZJ96C0YX59790KNM` | `story/relationships/cora-mina-obligation.md` |
| relationship | Tavi owes Orin a route inspection | `rel_4QMDM336YCMY6TT554VXDFQHPE` | `story/relationships/tavi-orin-obligation.md` |
| event | Storm Damage | `event_63W6ZYRM67EZ5VAB3PR5R19TFC` | `story/events/main/storm-damage.md` |
| event | Workshop Arrival | `event_7J9JDA9939Y1NJ08HZWE7RRCQE` | `story/events/main/workshop-arrival.md` |
| event | Missing Log | `event_3M1TJPZ4845X2J6V5QASPFZK4G` | `story/events/main/missing-log.md` |
| event | Log Discovery | `event_22CNFT9Q1FNTR7039D5A324D4X` | `story/events/main/log-discovery.md` |
| event | Lens Repaired | `event_2WRNKX74A1ZC8WJZ6GYXVFSXFR` | `story/events/main/lens-repaired.md` |
| event | Route Inspected | `event_6SVV6PN3XW92SE3X4SCRQ5GWQA` | `story/events/main/route-inspected.md` |
| event | Beacon Lit | `event_0AP9C85TAREDZ7F84QNM9SNQZ5` | `story/events/main/beacon-lit.md` |
| event | Ferry Resumed | `event_6FTMM1W0HX7G8HNWA313JBD2XE` | `story/events/main/ferry-resumed.md` |
| scene | Workshop Assessment | `scene_0QXK1AT5DC93TB0YZSB1KAEQVC` | `story/scenes/workshop-assessment.md` |
| scene | Quay Assessment | `scene_0M3G43J3775Q54BHQJH4KRWQCX` | `story/scenes/quay-assessment.md` |
| scene | Archive Consultation | `scene_098KPQFBY472Q3ZJDPN6NTA8B2` | `story/scenes/archive-consultation.md` |
| scene | Beacon Relight | `scene_7H983H74J7N5W9QRF3496GBXDH` | `story/scenes/beacon-relight.md` |
| scene | Ferry Return | `scene_0ZF3ST1ZH9RVWF23T7ZR002ZMT` | `story/scenes/ferry-return.md` |
| knowledge | Storm Report | `know_33RB1GJYPCGCPDA9KYT8GN4PN9` | `story/knowledge/storm-report.md` |
| knowledge | Repair Plan | `know_7EJZ8JXCFWVX6DEX37AY8N62KY` | `story/knowledge/repair-plan.md` |
| knowledge | Ferry Suspension | `know_5WR6NRNE78SCKKZ0FPAEYR61T5` | `story/knowledge/ferry-suspension.md` |
| knowledge | Log Absence | `know_0XNM8BDXPVH3XAEMC1STDN9VMB` | `story/knowledge/log-absence.md` |
| knowledge | Log Discrepancy | `know_0WYS8PMTPS0KNEX3SCQZ30E6JF` | `story/knowledge/log-discrepancy.md` |
| knowledge | Return Authorized | `know_16HV4MMSQFV7RVFESSE8GQEKJ0` | `story/knowledge/return-authorized.md` |
| conversation | Quay Log Review | `conv_3G4YJGZW7NHJ2NB5PESMV0A6S5` | `story/conversations/quay-log-review.md` |
| conversation | Workshop Repair Decision | `conv_4YFR36RGTKN7VE0R5XQFBKH6QT` | `story/conversations/workshop-repair-decision.md` |
| story-point | Repair the Beacon | `sp_1ZTN64D3SV94044DGY1JCK7AXN` | `story/story-points/repair-beacon.md` |
| story-point | Restore the Ferry | `sp_3Z4N9HS0W9N4NJDP95KFXZVFZD` | `story/story-points/restore-ferry.md` |
| story-point | Account for the Log | `sp_2CQKBF1PHXAEWR8ST4CM7PG5HT` | `story/story-points/account-for-log.md` |
| environment | Storm Weather | `env_07N1ZF3XBMMCJGVBB4N21QG93Y` | `story/environments/storm-weather.md` |
| environment | Quay Flood | `env_28ET043JHFN0CMZEYH1S9Y0B3K` | `story/environments/quay-flood.md` |
| hypothesis | Log Transcription Error | `hyp_7CDNS4KZ3WNHDHW716KE9W6PN1` | `story/hypotheses/log-transcription-error.md` |
| organization | Beacon Cooperative | `organization_4YNRVZSRYTGBB27YKD7NW1T7CZ` | `story/organizations/beacon-cooperative.md` |
| organization | Harbor Guild | `organization_5JNW6KPDP187ZKYZ2JRE9TP1ZG` | `story/organizations/harbor-guild.md` |
| affiliation | Mina's Cooperative Membership | `affiliation_3163J7T1VFYEY88H175PSMKJR5` | `story/affiliations/mina-beacon-cooperative.md` |
| affiliation | Orin's Guild Membership | `affiliation_6X7F5RXMG784DJX9VCVJR72HZD` | `story/affiliations/orin-harbor-guild.md` |
| parentage | Sela is Mina's Parent | `kinship_6FMPV333HWER1D3NXKSSNP6AZ9` | `story/kinships/sela-mina.md` |
| union | Sela and Orin's Union | `union_04W52R482EPFAG76MX1Y0DK2M1` | `story/unions/sela-orin.md` |
| legacy | Beacon Keeper Office | `legacy_4N3B8XV3V1QN8E13H8V5CHWDPQ` | `story/legacies/beacon-keeper.md` |
| tenure | Cora's Keeper Tenure | `tenure_7XHCEMFF7JDR55YP3NCW76C2F0` | `story/tenures/cora-keeper.md` |
| claim | Tavi's Keeper Claim | `claim_289DFQ0S7Z2YDEFBJXY2H5YPXH` | `story/claims/tavi-keeper.md` |
| vital-history | Sela's Vital History | `vital_7YPSPVFBM15VGK0NS739PNBQ18` | `story/vitals/sela-history.md` |
| map | Tideglass Town Map | `map:tideglass-town` | `story/maps/tideglass-town.md` |
| route | Quay to Beacon | `route:quay-beacon` | `story/routes/quay-beacon.md` |
| route | Beacon to Quay | `route:beacon-quay` | `story/routes/beacon-quay.md` |
| overlay | Flood Warning | `overlay:flood-warning` | `story/overlays/flood-warning.md` |

Counts: world 1, character 6, location 7, object 4, relationship 5, event 8, scene 5, knowledge 6, conversation 2, story-point 3, environment 2, hypothesis 1, organization 2, affiliation 2, parentage 1, union 1, legacy 1, tenure 1, claim 1, vital-history 1, map 1, route 2, overlay 1. Total: 64.

## Authored time, state and presence

Use one timeline, `main`. Every point is the literal tuple `{timeline: main, tick: <integer>, order: <integer>}`. Reads use inclusive horizons. Ticks/order are ordering coordinates; do not invent elapsed duration, civil dates or ages. Generational initialization occurs at -40/0, Sela's explicit birth at -30/0, and the parentage confirmation, union formation, affiliation roles and keeper-hold transitions at 0/0. Tavi's keeper dispute is at 20/0. All eight narrative events have these coordinates:

| Event | ID | Tick/order |
| --- | --- | --- |
| Storm Damage | `event_63W6ZYRM67EZ5VAB3PR5R19TFC` | 10/0 |
| Workshop Arrival | `event_7J9JDA9939Y1NJ08HZWE7RRCQE` | 20/0 |
| Missing Log | `event_3M1TJPZ4845X2J6V5QASPFZK4G` | 20/1 |
| Log Discovery | `event_22CNFT9Q1FNTR7039D5A324D4X` | 20/2 |
| Lens Repaired | `event_2WRNKX74A1ZC8WJZ6GYXVFSXFR` | 30/0 |
| Route Inspected | `event_6SVV6PN3XW92SE3X4SCRQ5GWQA` | 30/1 |
| Beacon Lit | `event_0AP9C85TAREDZ7F84QNM9SNQZ5` | 40/0 |
| Ferry Resumed | `event_6FTMM1W0HX7G8HNWA313JBD2XE` | 40/1 |

The Signal Lens starts intact at Beacon. Storm Damage sets cracked condition; Workshop Arrival moves it to Workshop; Lens Repaired sets repaired condition; Beacon Lit returns it to Beacon. Every state transition is an authored event effect with explicit target, field, value and cause. Placement remains exclusive: these lens transitions set location only, without simultaneous holder/container placement. The logbook starts at Archive and remains there; Log Discovery reveals the inaccurate entry rather than silently moving the object. Ferry Key starts held by Orin; Survey Staff starts held by Tavi. Ferry-resumed status is authored in an event/story-point transition; dialogue action beats cannot create state. Mina's directed trust toward Tavi initializes to 0.25 and becomes 0.75 at Lens Repaired. Other relationship records remain independent directed authored facts.

Scene participants have explicit location and presence bounds, observations have typed IDs and timed audiences, and scene outcomes reference only canonical events within the scene. All five scenes are closed historical records with current=end; concurrent scene demonstrations explicitly replay the named scenes at 20/0. No character is in two overlapping scene-presence windows. The intervals and casts are:

| Scene | Location | Cast | Inclusive interval | Outcome |
| --- | --- | --- | --- | --- |
| `scene_0QXK1AT5DC93TB0YZSB1KAEQVC` | `loc_117ZGNWJM0Y3N7CWER2H3XK722` | `char_4JVNDN6QW0BAFFV379AJJSMEPZ`, `char_0PYVCG3ETXZ640BWJ5JMT6XXFZ` | 20/0..20/0 | `event_7J9JDA9939Y1NJ08HZWE7RRCQE` |
| `scene_0M3G43J3775Q54BHQJH4KRWQCX` | `loc_0708459BXR2PVVHA4VASXHB69C` | `char_62PCSFAX4YQE48JK476XKGG6B8`, `char_6D246WN3J75HBRJJXGQ5H6QAD4` | 20/0..20/1 | `event_3M1TJPZ4845X2J6V5QASPFZK4G` |
| `scene_098KPQFBY472Q3ZJDPN6NTA8B2` | `loc_2SYYRXM9AHJGKYKGG200TZV82Y` | `char_6D246WN3J75HBRJJXGQ5H6QAD4`, `char_39E9EWWC017E40519033S4RW4X` | 20/2..20/2 | `event_22CNFT9Q1FNTR7039D5A324D4X` |
| `scene_7H983H74J7N5W9QRF3496GBXDH` | `loc_6ZT4TZRBGZTHMYVRDXV3HFTTPQ` | `char_4JVNDN6QW0BAFFV379AJJSMEPZ`, `char_0PYVCG3ETXZ640BWJ5JMT6XXFZ`, `char_46VTBJZ9TNMTF7G14N5GKYQ9C1` | 40/0..40/0 | `event_0AP9C85TAREDZ7F84QNM9SNQZ5` |
| `scene_0ZF3ST1ZH9RVWF23T7ZR002ZMT` | `loc_3VDY39471DED5V26TEVYXMRQK3` | `char_62PCSFAX4YQE48JK476XKGG6B8`, `char_6D246WN3J75HBRJJXGQ5H6QAD4`, `char_46VTBJZ9TNMTF7G14N5GKYQ9C1` | 40/1..40/1 | `event_6FTMM1W0HX7G8HNWA313JBD2XE` |

Ordinary knowledge starts unlearned and receives explicit timed knower transitions at the following horizons, with event causes. Private proposition bodies must use audience-filtered source sections as required by the current source/context contract, so literal record prose cannot leak inaccessible facts through character search/context. A knower transition is not a claim that every event participant learns the proposition.

| Knowledge | Knower | First learned | Cause |
| --- | --- | --- | --- |
| `know_33RB1GJYPCGCPDA9KYT8GN4PN9` | `char_4JVNDN6QW0BAFFV379AJJSMEPZ` | 10/0 | `event_63W6ZYRM67EZ5VAB3PR5R19TFC` |
| `know_7EJZ8JXCFWVX6DEX37AY8N62KY` | `char_0PYVCG3ETXZ640BWJ5JMT6XXFZ` | 20/0 | `event_7J9JDA9939Y1NJ08HZWE7RRCQE` |
| `know_5WR6NRNE78SCKKZ0FPAEYR61T5` | `char_62PCSFAX4YQE48JK476XKGG6B8` | 10/0 | `event_63W6ZYRM67EZ5VAB3PR5R19TFC` |
| `know_0XNM8BDXPVH3XAEMC1STDN9VMB` | `char_6D246WN3J75HBRJJXGQ5H6QAD4` | 20/1 | `event_3M1TJPZ4845X2J6V5QASPFZK4G` |
| `know_0WYS8PMTPS0KNEX3SCQZ30E6JF` | `char_6D246WN3J75HBRJJXGQ5H6QAD4` | 20/2 | `event_22CNFT9Q1FNTR7039D5A324D4X` |
| `know_16HV4MMSQFV7RVFESSE8GQEKJ0` | `char_46VTBJZ9TNMTF7G14N5GKYQ9C1` | 40/1 | `event_6FTMM1W0HX7G8HNWA313JBD2XE` |

## Spatial and generational envelope

The map is local planar, `crs: local-planar:tideglass`, axis order `[east, north]`, unit `pace`, min `[-100,-100]`, max `[100,100]`. Locations are authored point geometry on that map: Town `[0,0]`, Beacon `[40,20]`, Quay `[0,-20]`, Ferry House `[10,-20]`, Workshop `[20,0]`, Archive `[-20,10]`, Marsh Road `[-40,-20]`. Their narrative location hierarchy may use Town as explicit parent; no containment is inferred from coordinates. The two routes are separate one-way foot edges Quay→Beacon and Beacon→Quay. No route distance/duration is inferred from geometry. Flood Warning is a time-bounded author-perspective overlay with exactly Quay and Marsh Road membership, valid 10/0..30/1 inclusive. Storm Weather and Quay Flood remain ordinary environment records, distinct from overlay membership.

Beacon Cooperative and Harbor Guild use `organization_kind: institution`. Mina's role is repair coordinator and Orin's role ferry operator. Sela→Mina is the single biological parentage, confirmed at 0/0. Sela/Orin is the single union, formed at 0/0 with sorted participant IDs; it does not infer Orin's parentage. Beacon Keeper is an `office` legacy belonging to the Cooperative. Cora's tenure initializes unheld then explicitly holds at 0/0 with legal basis. Tavi's single claim initializes with an empty `competes_with` list and disputes at 20/0; no recognized claim, transfer or additional tenure is authored. Sela's known vital-history initializes then records birth at -30/0; it authors no death or inferred age. Generational records use the current closed transition payloads, public audience/ordinary perspective and explicit applicability. No extra member, inferred lineage, inferred successor or positive-grant knowledge transition is introduced.

## Scenario and expected-outcome matrix

All examples use current operations, source-backed `fts`/`state` paths and the current compiled APIs. They require no external model. Search limits remain within 1..50; use context budget 8000 (at least the current 1800 minimum) and bounded item/turn limits. Compiled first use may build the database; source/compiled reads must agree on the authored facts below, including cold rebuild after deletion of only a disposable copy's generated cache. Explicit require-compiled behavior retains its typed missing-cache refusal. No public v0.7 thread selectors or unsupported chronology operation is promised. Character viewpoint is narrative filtering, not an authentication boundary.

### 1. causes-and-state

Horizon(s): main:10/0, main:20/0, main:30/0, main:40/0.

- At 10/0 `obj_4C9HNZYKJG5QQ6DWTWDJ4NWJ8D` is cracked at `loc_6ZT4TZRBGZTHMYVRDXV3HFTTPQ`; its condition cause is `event_63W6ZYRM67EZ5VAB3PR5R19TFC`.
- At 20/0 its only placement is `loc_117ZGNWJM0Y3N7CWER2H3XK722` caused by `event_7J9JDA9939Y1NJ08HZWE7RRCQE`; at 30/0 condition is repaired caused by `event_2WRNKX74A1ZC8WJZ6GYXVFSXFR`; at 40/0 placement is `loc_6ZT4TZRBGZTHMYVRDXV3HFTTPQ` caused by `event_0AP9C85TAREDZ7F84QNM9SNQZ5`.
- The authored trust metric on `rel_7Y1Y0Z51JTGRZ02BTV2K31ERNA` changes from 0.25 to 0.75 at 30/0 with `event_2WRNKX74A1ZC8WJZ6GYXVFSXFR` as cause; no dialogue action mutates state.

### 2. same-tick-private-knowledge

Horizon(s): main:20/1, main:20/2.

- `char_6D246WN3J75HBRJJXGQ5H6QAD4` does not know `know_0WYS8PMTPS0KNEX3SCQZ30E6JF` at 20/1 and does know it at 20/2 through `event_22CNFT9Q1FNTR7039D5A324D4X`.
- `char_62PCSFAX4YQE48JK476XKGG6B8` never knows that private proposition at these horizons. Character search/context for Orin excludes its ID and unique proposition marker 'Tideglass log discrepancy'; Jessa at 20/1 excludes both; Jessa at 20/2 includes the ID and proposition.
- All character contexts before 40/1 exclude `know_16HV4MMSQFV7RVFESSE8GQEKJ0` and its unique proposition marker 'Tideglass return authorization'. Author view can inspect source; character viewpoint is narrative filtering, not authentication.

### 3. concurrent-scene-contexts

Horizon(s): main:20/0.

- Explicit `scene_0QXK1AT5DC93TB0YZSB1KAEQVC` context at 20/0 contains Mina and Tavi at Workshop, excluding Orin/Jessa and private Quay observations.
- Explicit `scene_0M3G43J3775Q54BHQJH4KRWQCX` context at 20/0 contains Orin and Jessa at Quay, excluding Mina/Tavi and private Workshop observations.
- Both are closed historical scenes replayed at the same horizon with disjoint participants and authored presence intervals. Never depend on an implicit scene selection among concurrent scenes.

### 4. conversations-and-recollections

Horizon(s): main:20/0, main:20/1, main:30/0.

- `conv_3G4YJGZW7NHJ2NB5PESMV0A6S5` has speech by Orin at 20/0 then Jessa at 20/1, both present, with an explicit Jessa recollection of her own second turn at 20/1.
- `conv_4YFR36RGTKN7VE0R5XQFBKH6QT` has speech by Mina at 20/0 and Tavi at 20/1, then an authored Tavi action beat at 20/2; Mina recalls the first speech at 30/0.
- At 20/0 transcript/context excludes later beats and recollections; at 20/1 the second speech is present; at 30/0 Mina's recollection is present. Recollections reference existing visible speech IDs, have authored at horizons, and do not create durable state effects.

### 5. authored-spatial-reads

Horizon(s): main:10/0, main:30/1, main:30/2.

- map:tideglass-town uses local-planar:tideglass, axes east/north, unit pace, bounds [-100,-100]..[100,100]; all seven authored points lie within them.
- route:quay-beacon is one-way foot movement `loc_0708459BXR2PVVHA4VASXHB69C` to `loc_6ZT4TZRBGZTHMYVRDXV3HFTTPQ`; route:beacon-quay authors the inverse explicitly. Return path comes from that record, not inferred bidirectionality.
- overlay:flood-warning contains exactly `loc_0708459BXR2PVVHA4VASXHB69C` and `loc_7F7G4Y2CZAD32HYW0B28N61Q72`; inclusive validity is 10/0 through 30/1, so present at both boundaries and absent at 30/2. Audience and perspective are author; no cross-map geometry, portal or elapsed-time claim.

### 6. explicit-family-and-keeper-history

Horizon(s): main:0/0, main:20/0.

- At 0/0 confirmed biological parentage `kinship_6FMPV333HWER1D3NXKSSNP6AZ9` gives `char_4JVNDN6QW0BAFFV379AJJSMEPZ` exactly authored parent `char_39E9EWWC017E40519033S4RW4X`; `union_04W52R482EPFAG76MX1Y0DK2M1` with Orin does not add Orin as Mina's parent.
- Affiliations explicitly connect Mina to `organization_4YNRVZSRYTGBB27YKD7NW1T7CZ` as repair coordinator and Orin to `organization_5JNW6KPDP187ZKYZ2JRE9TP1ZG` as ferry operator. Both organizations are institution records.
- `legacy_4N3B8XV3V1QN8E13H8V5CHWDPQ` is an office of Beacon Cooperative; `tenure_7XHCEMFF7JDR55YP3NCW76C2F0` explicitly holds it for `char_46VTBJZ9TNMTF7G14N5GKYQ9C1` from 0/0. `claim_289DFQ0S7Z2YDEFBJXY2H5YPXH` is Tavi's unresolved claim at 20/0, with no tenure transfer, recognized succession or additional holder.
- `vital_7YPSPVFBM15VGK0NS739PNBQ18` explicitly records Sela's birth at -30/0, known disclosure. Ticks do not imply years, civil dates, ages or lifespans.

### 7. confirmed-disposable-edit

Horizon(s): main:40/1.

- In a disposable initialized copy only, preview a wedl-changeset/v1 request with expectedHead equal to that copy's HEAD, idempotencyKey 'tideglass-survey-title-v1', a summary and one entity.update operation with entityId `obj_76VC9SVNSR7ASZK025X7PTRBJH` and frontmatterPatch {title: 'Calibrated Survey Staff'}. Preview alone changes neither source nor HEAD.
- A stale expectedHead or mismatched confirmationToken refuses with no source/HEAD change. Apply exactly the previewed payload using its expectedHead and preview confirmationToken through the current apply interface; only the permitted record field changes.
- Replay the identical already-applied request and confirmation through the receipt contract: idempotentReplay is true, with no second content mutation or duplicate commit. Canonical package and checkout stay unchanged. This demonstrates current editing APIs; do not add a mutation API or grant runtime identity permissions.

## Additive implementation and acceptance

Phase B authors only the native package and stable source identities described here. Phase C adds `tideglass` through the existing initializer/parser and package-data conventions; its current compatibility decisions are updated to distinguish the then-implemented choice. Phase D adds focused regression coverage for the seven rows, exact count/capability/schema/size bounds, deterministic IDs, authored-only source, strict validation, source/compiled equivalence and cache recovery. Phase E delivers practical executable guides with literal IDs/arguments matching the package. Phase F verifies isolated installed delivery, current doctor and the unchanged supported normal suite with its true 800-second global limit and honest additive test enrollment. Existing assertions, P-suite isolation, old selectors/defaults, source migration behavior, the three converter outputs and protected custody remain intact. No deadline waiver, new benchmark qualification or copied-story recovery is part of this example.

The focused scenarios and disposable editing demonstration are developer/reviewer pass/fail evidence; routine success does not create a permanent raw-log requirement. Use bounded task-owned scratch, retain only targeted unresolved diagnostics with owner and finite disposition, and preserve preexisting work and protected evidence. Each scoped source change and complete automatic ADRAI commit chain receives independent review before its phase closes. This design decision itself does not claim package/tests/guides/installed delivery have already passed.

<!-- @adrai:eyJhIjp7ImkiOiJjYWxpYnJhdGlvbl9kZXZlbG9wZXIiLCJrIjoibGxtIiwibSI6ImdwdC02LjEtc29sIn0sImIiOiI3NzM4MDIyZmNiMWM5MDg0ZThjYWFlOWVmZGE2YWVjYmZkZDAwZmI3IiwiaSI6InNoYTI1NjpKOHBJcXlsblRlSGNyTzB1N0JhN2ZCb3pxQTZxQTBYSXhibFNIRHl3YnVzIiwiayI6ImRlY2lzaW9uLmNyZWF0ZSIsIm8iOiJSMDFNNENYWkhRWTBGVE02NDQ0WDAwUUhYNzUiLCJvcCI6Ik8wMU00Q1haSFFZMEZUTTY0NDRYMDBRSFg3NSIsInIiOiJtYXN0ZXIiLCJzIjoic2hhMjU2OlFOOXcyRXRGci1yZUtTbjdDOHlHWnVsZVk3eEplSmUzTE0tU25GQWVYSlUiLCJ0IjoxNzkxNDM1NDU4MzAyLCJ2IjoxLCJ4IjoiYWRyYWkvMS4wLjAifQ -->
