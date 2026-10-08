# Tideglass

Tideglass is a small coastal town trying to restore its beacon and ferry after
a storm. Its 64-record native v0.7 example combines a repair story with a
missing-log question, explicit family and institutional facts, and a small map.
Start with the [walkthrough](walkthrough.md); the [query recipes](queries.md)
show spatial and generational reads and a confirmed edit on your own copy.

## People and places

| Person | Part in the story |
|---|---|
| Mina Vale | Beacon Cooperative repair coordinator; repairs the signal lens with Tavi |
| Tavi Moss | Workshop collaborator; has a disputed claim to the keeper role |
| Orin Reed | Harbor Guild ferry operator; assesses the Quay with Jessa |
| Jessa Quill | Investigates the missing maintenance log, then consults the Archive |
| Cora Finch | The sole explicitly authored Beacon Keeper |
| Sela Vale | Mina's biological parent; consults with Jessa at the Archive |

The town contains the Beacon, Workshop, Quay, Archive, Ferry House and Marsh
Road. The map gives these places authored points. The Quay-to-Beacon foot route
and its reverse are two separate directed routes. A flood-warning overlay covers
the Quay and Marsh Road from `main 10:0` through `main 30:1`, inclusive.

## Follow the repair

Use `main tick:order` below when selecting a read. The values order story facts;
they are not elapsed hours or civil dates.

| Moment | What to inspect |
|---|---|
| `10:0` | Storm Damage leaves the signal lens cracked at the Beacon |
| `20:0` | Workshop Arrival moves the cracked lens to the Workshop; repair and Quay assessments run concurrently |
| `20:1` | Both Quay speeches are visible; Jessa has not yet acquired the log-discrepancy knowledge |
| `20:2` | Jessa acquires the private discrepancy at the Archive; the workshop conversation includes Tavi's action |
| `30:0` | Lens Repaired changes the lens condition and Mina's trust in Tavi from 0.25 to 0.75 |
| `40:0` | Beacon Lit returns the repaired lens to the Beacon |
| `40:1` | Ferry Resumed closes the short story; Cora has the return authorization |

All five example scenes are closed. Select a named scene and an authored
presence time for a character context. Mina and Tavi are present in Workshop
Assessment at `20:0`; Orin and Jessa are present in Quay Assessment through
`20:1`. Orin's Quay context at `20:2` is refused. Conversation beats and later
recollections have their own authored times: a short scene window does not
extend or truncate those separate records.

Jessa's discrepancy is available to her at `20:2`, not at `20:1`, and it is not
Orin's knowledge. The uncertain transcription-error hypothesis does not settle
the missing-log question. Sela's birth is explicitly recorded at `-30:0` with
known disclosure; the example supplies no inferred civil age. Sela's union with
Orin does not make him Mina's parent. Tavi's disputed keeper claim does not add
a holder or a succession edge: Cora remains the sole holder.

## Useful identities

Commands also accept exact titles, but these stable IDs make the recipes
unambiguous.

| Record | ID |
|---|---|
| Mina | `char_4JVNDN6QW0BAFFV379AJJSMEPZ` |
| Jessa | `char_6D246WN3J75HBRJJXGQ5H6QAD4` |
| Orin | `char_62PCSFAX4YQE48JK476XKGG6B8` |
| Sela | `char_39E9EWWC017E40519033S4RW4X` |
| Signal lens | `obj_4C9HNZYKJG5QQ6DWTWDJ4NWJ8D` |
| Survey staff | `obj_76VC9SVNSR7ASZK025X7PTRBJH` |
| Workshop Assessment | `scene_0QXK1AT5DC93TB0YZSB1KAEQVC` |
| Quay Assessment | `scene_0M3G43J3775Q54BHQJH4KRWQCX` |
| Archive Consultation | `scene_098KPQFBY472Q3ZJDPN6NTA8B2` |
| Quay Log Review | `conv_3G4YJGZW7NHJ2NB5PESMV0A6S5` |
| Workshop Repair Decision | `conv_4YFR36RGTKN7VE0R5XQFBKH6QT` |
| Log discrepancy | `know_0WYS8PMTPS0KNEX3SCQZ30E6JF` |
| Quay | `loc_0708459BXR2PVVHA4VASXHB69C` |
| Beacon Cooperative | `organization_4YNRVZSRYTGBB27YKD7NW1T7CZ` |
| Harbor Guild | `organization_5JNW6KPDP187ZKYZ2JRE9TP1ZG` |
| Beacon Keeper | `legacy_4N3B8XV3V1QN8E13H8V5CHWDPQ` |

See the [manual](../../README.md) for the CLI, context and conversation guides.
