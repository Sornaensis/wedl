# Authoring examples

These small request files build on a blank `wedl-rs init` world. They are
command inputs, not canonical Markdown records. Files 12–14 also require the
explicit route capability edit described below; files 15–18 require geometry.
Generational histories in files 23–35 require `generational-core-v1`.
Learned claims in files 36–44 additionally require `generational-knowledge-v1`.
Files 45–57 add geometry, anchors, portals and overlays. Files 58–63 add and
correct spatial histories, then explicitly clear one history array. Files 64–71
add typed lore profiles, concepts and links with `lore-core-v1`. Files 72–79 add
explicit state bindings and separately authored common/individual descriptions.
Files 80–87 add magic role profiles, three dated practices and a tradition edit.
Files 88–97 add explicit dated Markdown, predicates and classified assertions.
Files 98–125 add social/language and environmental/material/technology contexts.
Files 126–142 add independent existence episodes and explicit governance roles,
collective parties and native policy assertions.
Files 143–158 add scoped public concepts, linked assertion learning, independent
record acquisition, dated sections and endpoint-gated spatial history.
Files 159–175 add authored required briefs, nested/cyclic pins and an explicit
writing scene. Files 176–203 add independent family acquisitions, dated accounts and
explicit family capture. Files 204–207 add a public retrieval prior, learned
report and independently acquired dated notice. The complete sequence leaves
144 valid source records across 207 requests,
25 profiles and nine predicates, with 24 editable non-world kinds.
Apply requests by numeric prefix: 100 follows 99; filename lexicographic order
is not the execution order.
The complete
capability declarations for each sequence appear below.

Process files in numbered order. For each file, take the current fingerprint
from `status`, preview, inspect `validation` and `afterSource`, then apply the
same request with the returned `afterSha256`:

| Files | Operation | Target/default |
| --- | --- | --- |
| 01–05 | `record create` | Location, character, object, event and knowledge |
| 06-character-update.yaml | `record update "Ana Reed"` | Partial character update |
| 07-scene.yaml | `record create` | Planned scene, Harbor Watch |
| 08-conversation.yaml | `record create` | Draft conversation, The Landing Is Clear |
| 09-scene-close.yaml | `record update "Harbor Watch"` | Close scene and add conversation link |
| 10-conversation-close.yaml | `record update "The Landing Is Clear"` | Close conversation |
| 11-landing.yaml | `record create` | Child location, Lower Landing |
| 12-route.yaml | `record create` | Two-way walking route, Harbor Steps |
| 13-route-close.yaml | `record update "Harbor Steps"` | Close route |
| 14-route-reopen.yaml | `record update "Harbor Steps"` | Reopen route in one direction |
| 15-map.yaml | `record create` | Local-planar Harbor Map |
| 16-harbor-placement.yaml | `record update Harbor` | Place Harbor at [0,0] |
| 17-landing-placement.yaml | `record update "Lower Landing"` | Place Lower Landing at [3,4] |
| 18-map-bounds.yaml | `record update "Harbor Map"` | Expand bounds while retaining placements |
| 19-world-chronology.yaml | `record update WORLD_ID` | Declare two calendars, an era and exact story anchors |
| 20-character-chronology.yaml | `record update "Ana Reed"` | Add eight annotations covering all seven value forms |
| 21-rowan.yaml | `record create` | Character, Rowan Reed |
| 22-jules.yaml | `record create` | Character, Jules Vale |
| 23-parentage.yaml | `record create` | Rowan's dated parentage of Ana |
| 24-union.yaml | `record create` | Rowan and Jules's dated union |
| 25-parentage-end.yaml | `record update kinship_00000000000000000000000001` | End parentage at tick 10 while retaining confirmation |
| 26-union-end.yaml | `record update union_00000000000000000000000001` | End the union at tick 11 while retaining formation |
| 27-organization.yaml | `record create` | Harbor Guild institution |
| 28-affiliation.yaml | `record create` | Ana's membership and dated role |
| 29-legacy.yaml | `record create` | Harbor Keeper office |
| 30-tenure.yaml | `record create` | Vacancy followed by Ana's explicit holding |
| 31-claim.yaml | `record create` | Jules's separate keeper claim |
| 32-vital.yaml | `record create` | Ana's known birth history |
| 33-organization-rename.yaml | `record update organization_00000000000000000000000001` | Rename the organization at tick 3 |
| 34-tenure-vacancy.yaml | `record update tenure_00000000000000000000000001` | Vacate over ticks 10–12, exact order 0 endpoints |
| 35-affiliation-end.yaml | `record update affiliation_00000000000000000000000001` | End membership at tick 15 |
| 36-learned-parentage.yaml | `record create` | Ana's mistaken parentage belief, learned at tick 4 |
| 37-learned-union.yaml | `record create` | An explicitly learned union |
| 38-learned-organization.yaml | `record create` | A learned guild name and root organization |
| 39-learned-affiliation.yaml | `record create` | Suspected membership with an unspecified role |
| 40-learned-tenure.yaml | `record create` | Belief that an office is vacant |
| 41-learned-claim.yaml | `record create` | A separately learned keeper claim |
| 42-learned-vital.yaml | `record create` | A fallible vital-state assertion |
| 43-reject-parentage.yaml | `record update know_00000000000000000000000002` | Reject the earlier belief at tick 9/order 0 |
| 44-replacement-parentage.yaml | `record create` | Learn a different parentage at tick 9/order 1 |
| 45-boardwalk-line.yaml | `record create` | Harbor Boardwalk line on Harbor Map |
| 46-courtyard-polygon.yaml | `record create` | Filled Harbor Courtyard polygon on Harbor Map |
| 47-level-map.yaml | `record create` | Three-dimensional local-planar Harbor Levels map |
| 48-watchtower-point.yaml | `record create` | Harbor Watchtower at [3,4,2] on Harbor Levels |
| 49-geographic-map.yaml | `record create` | EPSG:4326 Harbor Coast Survey map |
| 50-geographic-point.yaml | `record create` | Harbor Survey Mark at longitude 12.5, latitude 55.7 |
| 51-survey-anchor.yaml | `record create` | Literal correspondence between Harbor Map and Harbor Coast Survey |
| 52-watchtower-portal.yaml | `record create` | Directed foot portal from Harbor to Harbor Watchtower |
| 53-position-portal.yaml | `record create` | Climb portal from Harbor to an explicit map position |
| 54-static-overlay.yaml | `record create` | Static Maintenance Area membership |
| 55-dated-overlay.yaml | `record create` | Harbor Flood Warning membership over ticks 10–12, exact order 0 endpoints |
| 56-retire-portal.yaml | `record update portal:harbor/watchtower` | Retire the location portal created by file 52 |
| 57-overlay-window.yaml | `record update overlay:harbor/flood-warning` | Extend the warning through tick 14/order 0 |
| 58-landing-placement-history.yaml | `record update location:harbor/landing` | Place the landing at [8,9] over ticks 20–29 |
| 59-landing-history-correction.yaml | `record update location:harbor/landing` | Retain the original and correct its position to [9,10] |
| 60-landing-parentage-history.yaml | `record update location:harbor/landing` | Add independent root parentage over the same exact interval |
| 61-route-history.yaml | `record update route:harbor/steps` | Add independent metrics and a shorter closed-availability interval |
| 62-route-history-correction.yaml | `record update route:harbor/steps` | Retain the original cost and correct it from 10 to 7 coins |
| 63-clear-landing-history.yaml | `record update location:harbor/landing` | Clear only the landing's history array; retain static placement/parent |
| 64-world-lore-profiles.yaml | `record update WORLD_ID` | Declare Harbor symbol and Harbor tradition profiles |
| 65-lantern-symbol.yaml | `record create` | Timeless Lantern Mark concept |
| 66-lantern-tradition.yaml | `record create` | Lantern Launch concept over ticks 10–20, exact order 0 endpoints |
| 67-character-concept-links.yaml | `record update "Ana Reed"` | Add a dated association with Lantern Launch and a timeless symbol link |
| 68-tradition-values.yaml | `record update concept_00000000000000000000000002` | Replace the complete tradition values, changing the offering and adding notes |
| 69-world-profile-extension.yaml | `record update WORLD_ID` | Replace the profiles, extending the tradition with optional `season_label` |
| 70-retire-symbol.yaml | `record update concept_00000000000000000000000001` | Retire Lantern Mark while retaining its source and authored references |
| 71-clear-concept-links.yaml | `record update "Ana Reed"` | Explicitly clear Ana's concept links |
| 72-world-state-profile-bindings.yaml | `record update WORLD_ID` | Retain existing profiles, declare description profiles and bind a declared character state key |
| 73-tide-folk-common.yaml | `record create` | Root Tide Folk common description with an explicit empty parent list |
| 74-river-folk-common.yaml | `record create` | River Folk common description explicitly linked to Tide Folk |
| 75-ana-individual-variation.yaml | `record create` | Ana's separate variation linked to River Folk over ticks 10–20, exact order 0 endpoints |
| 76-character-bound-state.yaml | `record update "Ana Reed"` | Replace initial state with ordinary mood and a typed species-notes object |
| 77-bound-state-set-event.yaml | `record create` | Explicitly replace the bound notes at tick 12/order 0 |
| 78-common-description-update.yaml | `record update concept_00000000000000000000000003` | Replace Tide Folk values without rewriting the variation or bound state |
| 79-bound-state-clear-event.yaml | `record create` | Explicitly clear the bound state key at tick 18/order 0 |
| 80-world-magic-profiles.yaml | `record update WORLD_ID` | Retain all earlier profiles and add tradition, practice, realm and artifact profiles |
| 81-lantern-magic-tradition.yaml | `record create` | Timeless Lantern Calling Tradition with separate restrictions and exceptions |
| 82-tidal-magic-realm.yaml | `record create` | Timeless Tidal Echo Realm with an absent restriction and explicit null exception |
| 83-lantern-magic-artifact.yaml | `record create` | Calling Lantern linked to the realm, with an empty exceptions array |
| 84-early-lantern-calling.yaml | `record create` | Early practice over ticks 0–9, exact order-zero endpoints |
| 85-middle-lantern-calling.yaml | `record create` | Middle practice over ticks 10–19, exact order-zero endpoints |
| 86-late-lantern-calling.yaml | `record create` | Late practice over ticks 20–29, exact order-zero endpoints |
| 87-magic-tradition-update.yaml | `record update concept_00000000000000000000000006` | Replace the shared tradition wording without rewriting any practice |
| 88-world-narrative-predicates.yaml | `record update WORLD_ID` | Declare three current predicates with object, string and null value types |
| 89-early-practice-descriptions.yaml | `record update concept_00000000000000000000000009` | Add two independent dated Markdown accounts without changing the body |
| 90-correct-early-description.yaml | `record update concept_00000000000000000000000009` | Retain both accounts and append an exact-applicability correction of the first |
| 91-early-practice-fact.yaml | `record create` | Typed author-fact assertion with exact values and explicit references |
| 92-attributed-practice-account.yaml | `record create` | A separate claim attributed to Ana, with historical citation syntax |
| 93-unresolved-practice-origin.yaml | `record create` | Explicit unresolved assertion whose declared value is null |
| 94-correct-practice-fact.yaml | `record create` | Replace the first fact at the same subject, predicate and interval |
| 95-account-description-and-date.yaml | `record update assertion_00000000000000000000000002` | Add a dated explanation and independent chronology annotation |
| 96-extend-narrative-predicate.yaml | `record update WORLD_ID` | Add an optional qualification field without rewriting assertions |
| 97-alternative-practice-correction.yaml | `record create` | A second correction of the original fact, retained alongside the first correction |
| 98-world-family-context-profiles.yaml | `record update WORLD_ID` | Preserve existing declarations and add the fifteen family profiles and three predicates |
| 99-harbor-script.yaml | `record create` | Harbor script |
| 100-river-language.yaml | `record create` | River language |
| 101-hill-language.yaml | `record create` | Hill language |
| 102-ceremonial-register.yaml | `record create` | Ceremonial register |
| 103-river-sense.yaml | `record create` | River bank sense |
| 104-hill-sense.yaml | `record create` | Hill bank sense |
| 105-older-river-spelling.yaml | `record create` | Older River spelling |
| 106-current-river-spelling.yaml | `record create` | Current River spelling |
| 107-later-river-spelling.yaml | `record create` | Later River spelling |
| 108-overlapping-river-spelling.yaml | `record create` | Overlapping River spelling |
| 109-hill-homonym.yaml | `record create` | Hill spelling bank |
| 110-harbor-culture.yaml | `record create` | Harbor culture |
| 111-river-religion.yaml | `record create` | River religion |
| 112-speaker-religion-membership.yaml | `record create` | Speaker religion membership |
| 113-communal-doctrine.yaml | `record create` | Communal doctrine |
| 114-individual-doctrine-account.yaml | `record create` | Individual doctrine account |
| 115-correct-communal-doctrine.yaml | `record create` | Corrected communal doctrine |
| 116-harbor-authored-era.yaml | `record create` | Harbor reed era |
| 117-reed-resource.yaml | `record create` | Reed resource |
| 118-reed-habitat.yaml | `record create` | Reed habitat |
| 119-reed-weaving.yaml | `record create` | Reed weaving |
| 120-harbor-workshop.yaml | `record create` | Harbor workshop |
| 121-reed-livelihood.yaml | `record create` | Reed livelihood |
| 122-reed-trade.yaml | `record create` | Reed trade |
| 123-trade-availability.yaml | `record create` | Trade availability account |
| 124-contrary-trade-account.yaml | `record create` | Contrary trade account |
| 125-unresolved-trade-dependency.yaml | `record create` | Unresolved trade dependency |
| 126-world-governance-profile.yaml | `record update WORLD_ID` | Preserve prior declarations; add one governance profile and three predicates |
| 127-embodied-episode.yaml | `record create` | Embodied episode with independent historical evidence |
| 128-disembodied-episode.yaml | `record create` | Overlapping disembodied episode |
| 129-constructed-episode.yaml | `record create` | Later constructed episode with an unbounded end |
| 130-unbounded-other-episode.yaml | `record create` | Affirmative wholly unbounded account |
| 131-unknown-episode.yaml | `record create` | Explicit unknown account with empty evidence |
| 132-alternative-embodied-episode.yaml | `record create` | Independent overlapping embodied alternative |
| 133-episode-description-and-date.yaml | `record update existence_00000000000000000000000001` | Common dated description and calendar annotation |
| 134-legal-harbor-policy.yaml | `record create` | Legal policy with three Party kinds and exact obligations |
| 135-de-facto-overlay-policy.yaml | `record create` | Static overlay policy with an explicit null field |
| 136-claimed-warning-policy.yaml | `record create` | Claimed policy with timed overlay jurisdiction |
| 137-contested-harbor-policy.yaml | `record create` | Contested account with a cited supersedes pointer |
| 138-policy-account.yaml | `record create` | Author policy-text assertion |
| 139-guild-policy-endorsement.yaml | `record create` | Attributed assertion whose root entity value links the policy |
| 140-contrary-policy-account.yaml | `record create` | Independent attributed account of the contested policy |
| 141-correct-policy-account.yaml | `record create` | Compatible assertion correction retaining its predecessor |
| 142-literal-policy-note.yaml | `record create` | Ref-shaped literal data that does not join as a typed entity |
| 143-public-reference-context.yaml | `record create` | Scoped public technology concept, independently learned later |
| 144-public-reading-context.yaml | `record create` | Public resource concept with typed references, links and dated sections |
| 145-canonical-ferry-rule.yaml | `record create` | Canonical ferry rule with author-only scope |
| 146-learned-ferry-report.yaml | `record create` | Literal mistaken report with rejection, learning, forgetting and remembering |
| 147-private-ferry-report.yaml | `record create` | Author-only learned report, excluded before character admission |
| 148-learn-reading-context.yaml | `record create` | Acquire concept 37 at tick 10 |
| 149-learn-reference-context.yaml | `record create` | Acquire concept 38 at tick 13, forget it at tick 20 |
| 150-learn-harbor-record.yaml | `record create` | Acquire the Harbor record at tick 10 |
| 151-harbor-learnable-placement.yaml | `record update loc_00000000000000000000000001` | Retain and correct dated Harbor placement without teaching its map |
| 152-learn-harbor-map.yaml | `record create` | Acquire the map at tick 15, forget it at tick 18 |
| 153-correct-public-reading-section.yaml | `record update concept_00000000000000000000000037` | Retain and correct one admitted section without removing its alternative |
| 154-correct-canonical-ferry-rule.yaml | `record create` | Correct canonical wording without rewriting the learned report |
| 155-learn-existence-pointer.yaml | `record create` | Ordinary episode pointer that supplies no typed episode payload |
| 156-forget-reading-context.yaml | `record update know_00000000000000000000000012` | Forget concept 37 at tick 22 |
| 157-unlearned-rejection.yaml | `record create` | Reject an assertion without a prior affirmative learning transition |
| 158-dated-self-description.yaml | `record update char_00000000000000000000000001` | Add Ana's independently scoped description over ticks 10–19 |
| 159-bell-dated-description.yaml | `record update obj_00000000000000000000000001` | Date the bell's exact visible description |
| 160-ferry-discussion-scene.yaml | `record create` | Closed writing scene with Ana, the bell and Rowan's later arrival |
| 161-ferry-detail-brief.yaml | `record create` | Required concept section, learned transition and bell section |
| 162-learn-ferry-detail-brief.yaml | `record create` | Independently acquire the detail brief |
| 163-harbor-writing-brief.yaml | `record create` | Region brief requiring the detail brief and Ana's section |
| 164-learn-harbor-writing-brief.yaml | `record create` | Independently acquire the region brief |
| 165-withheld-required-brief.yaml | `record create` | Brief with a private required pin after a public section |
| 166-learn-withheld-required-brief.yaml | `record create` | Acquire the brief without learning its required private target |
| 167-learned-report-brief.yaml | `record create` | Require the separately learned mistaken report |
| 168-learn-report-brief.yaml | `record create` | Independently acquire the report brief |
| 169-canonical-rule-brief.yaml | `record create` | Require the corrected canonical assertion |
| 170-learn-canonical-rule-brief.yaml | `record create` | Acquire the brief without learning the canonical assertion |
| 171-repeat-nested-required-pin.yaml | `record update brief_00000000000000000000000002` | Repeat an exact section already required by a child brief |
| 172-revise-authored-brief-prose.yaml | `record update brief_00000000000000000000000001` | Replace only explicit brief prose |
| 173-reciprocal-setting-brief.yaml | `record create` | Brief requiring the existing region brief |
| 174-learn-reciprocal-setting-brief.yaml | `record create` | Independently acquire the reciprocal brief |
| 175-close-required-brief-cycle.yaml | `record update brief_00000000000000000000000002` | Close a finite exact-reference cycle |
| 176-public-tide-description.yaml | `record update concept_00000000000000000000000003` | Publish scoped common sections with a retained correction and private alternative |
| 177-public-river-description.yaml | `record update concept_00000000000000000000000004` | Allow the River Folk account through its own presentation labels |
| 178-public-individual-description.yaml | `record update concept_00000000000000000000000005` | Publish a separate dated individual account |
| 179-learn-individual-description.yaml | `record create` | Acquire the individual account at 11/0 |
| 180-learn-river-description.yaml | `record create` | Independently acquire its direct common account at 11/0 |
| 181-learn-common-description-later.yaml | `record create` | Acquire the ancestral account at 13/0 and forget it at 17/0 |
| 182-public-river-language.yaml | `record update concept_00000000000000000000000013` | Publish a dated language note without teaching the language record |
| 183-public-river-sense.yaml | `record update concept_00000000000000000000000016` | Publish the exact River sense identity |
| 184-public-later-river-spelling.yaml | `record update concept_00000000000000000000000020` | Publish the later spelling with its original exact interval |
| 185-public-harbor-script.yaml | `record update concept_00000000000000000000000012` | Publish the script without granting it through language links |
| 186-public-river-religion.yaml | `record update concept_00000000000000000000000024` | Publish the religion and its separate dated observance |
| 187-learn-river-language.yaml | `record create` | Acquire the River language record |
| 188-learn-river-sense.yaml | `record create` | Independently acquire the selected sense |
| 189-learn-later-river-spelling.yaml | `record create` | Independently acquire the later spelling |
| 190-learn-harbor-script-later.yaml | `record create` | Acquire the script at 13/0 and forget it at 17/0 |
| 191-learn-river-religion.yaml | `record create` | Independently acquire the religion account |
| 192-learned-social-account.yaml | `record create` | Learn a mistaken social report, retain rejection and later forget it |
| 193-public-reed-trade.yaml | `record update concept_00000000000000000000000032` | Publish the trade account and its dated note |
| 194-public-reed-resource.yaml | `record update concept_00000000000000000000000027` | Publish the resource quantity as literal authored text |
| 195-public-reed-livelihood.yaml | `record update concept_00000000000000000000000031` | Publish the livelihood without acquiring it through trade links |
| 196-learn-reed-trade.yaml | `record create` | Acquire the trade root |
| 197-learn-reed-resource.yaml | `record create` | Acquire the resource and later forget it |
| 198-learn-reed-livelihood-later.yaml | `record create` | Acquire the direct livelihood endpoint at 13/0 |
| 199-learned-trade-account.yaml | `record create` | Learn a separate mistaken trade report |
| 200-middle-practice-dated-explanation.yaml | `record update concept_00000000000000000000000010` | Attach a native dated correction to the middle magic practice |
| 201-middle-practice-account.yaml | `record create` | Create a canonical middle-practice account |
| 202-learned-magic-account.yaml | `record create` | Learn a mistaken magic report independently of the canonical account |
| 203-correct-middle-practice-account.yaml | `record create` | Correct canonical wording without rewriting the learned report |
| 204-public-retrieval-prior.yaml | `record update WORLD_ID` | Add explicit timeless public training sections, excluded private/timed sections and separate author body text |
| 205-learned-ferry-account.yaml | `record create` | Learn a literal ferry report at 20/0, reject it at 22/0 and forget it at 26/0 |
| 206-ferry-notice.yaml | `record create` | Create an independently dated public notice and a separate private section |
| 207-learn-ferry-notice.yaml | `record create` | Acquire the notice at 21/0 and forget it at 24/0 |

```sh
wedl-rs --repo my-world status
wedl-rs --repo my-world record create --file examples/authoring/01-location.yaml --expected-fingerprint FINGERPRINT
wedl-rs --repo my-world record create --file examples/authoring/01-location.yaml --expected-fingerprint FINGERPRINT --apply --expected-after-sha256 HASH
```

Use the new fingerprint after each save. For example, file 06 is a partial update:

```sh
wedl-rs --repo my-world record update "Ana Reed" --file examples/authoring/06-character-update.yaml --expected-fingerprint FINGERPRINT
```

Apply an update with the same two apply flags. The title, aliases, pronouns and
body remain intact because file 06 omits them.

Files 07 and 08 require the earlier records and demonstrate nested observations,
speech/actions and personal recollection. Planned/draft contents validate but
do not enter character evidence. Files 09 and 10 explicitly make them closed,
with existing end times; they preserve the bodies and other omitted fields.
The scene link is added after the conversation exists, so every single-record
save has valid references.

These examples use closed status because the initializer's world cursor remains
at tick 0/order 0. Setting the later scene active would require a matching world
cursor, which currently needs a direct source edit. No request moves it implicitly.

After the first ten requests, the world has eight records including its world record:

```sh
wedl-rs --repo my-world timeline --tick 1
wedl-rs --repo my-world knowledge "Ana Reed" --tick 1
wedl-rs --repo my-world scene "Harbor Watch" --tick 2 --order 0
wedl-rs --repo my-world character context char_00000000000000000000000001 --tick 2 --order 0
wedl-rs --repo my-world character search char_00000000000000000000000001 lantern --tick 2 --order 0
wedl-rs --repo my-world character conversation char_00000000000000000000000001 conv_00000000000000000000000001 --tick 3 --order 0
wedl-rs --repo my-world serve
```

At tick 1/order 1 the observation is visible but the speech at order 2 is still
future. At tick 2/order 0 both beats are available; the personal recollection
appears only at tick 3/order 0. The executable tests run this exact request sequence
and check those boundaries, save bytes and rejected invalid edits.

## Extend the spatial graph

File 11 creates Lower Landing with Harbor as its explicit parent. It does not
infer a travel connection. Before creating the route in file 12, edit the blank
world's existing `capabilities` field in `story/world.md` to:

```yaml
capabilities: [spatial-core-v1, route-v1]
```

Retain any other declared capabilities and their established order if extending
a different world. Refresh the fingerprint after this direct source edit. Without
the declaration, route preview returns `canApply:false` and apply refuses it.
The request does not change world capabilities automatically.

Apply file 12 with the same preview-and-hash flow. These queries then show the
child relationship and a one-hop route in either direction:

```sh
wedl-rs --repo my-world spatial location Harbor
wedl-rs --repo my-world spatial path Harbor "Lower Landing" --mode foot
wedl-rs --repo my-world spatial path "Lower Landing" Harbor --mode foot
```

File 13 closes the route; neither direction is traversable. File 14 reopens it
as one-way from Harbor to Lower Landing. The reverse query then returns
`found:false`, as does a query with `--mode boat`. Authored distance and duration
remain on the route. Request a metric to minimize its authored value:

```sh
wedl-rs --repo my-world spatial path Harbor "Lower Landing" --metric route_distance --unit m
wedl-rs --repo my-world spatial path Harbor "Lower Landing" --metric duration --unit seconds
```

These return exact totals `"30"` m and `"45"` seconds. With no metric specified,
paths still minimize hops. A travel-cost query has no qualified path because
these requests never author a cost.

After the first fourteen requests, the world has ten records. Spatial queries inspect
the canonical authored graph across all story time; they take no story horizon.
The executable regression verifies this complete extension, including capability
failure, exact saved bytes, metric preservation and each availability/direction
change. See [the spatial contract](../../docs/SPATIAL.md) for details.

## Add a map and point placements

Before file 15, extend the world capabilities to:

```yaml
capabilities: [spatial-core-v1, geometry-v1, route-v1]
```

Refresh the fingerprint, then preview/apply files 15–18 in order. They create a
map, place Harbor and Lower Landing, and expand the map bounds. The world then
has eleven records, including the map and world records.

```sh
wedl-rs --repo my-world spatial maps
wedl-rs --repo my-world spatial bbox "Harbor Map" --min-x -1 --min-y -1 --max-x 3 --max-y 4
wedl-rs --repo my-world spatial nearby "Harbor Map" --x 0 --y 0 --radius 5
```

Both locations match these inclusive queries. The geometric distance from Harbor
to Lower Landing is 5 m, while the authored walking-route distance remains 30 m.
Changing the radius to 4.9 excludes Lower Landing. Map coordinates do not change
route measurements or create connectivity. The executable regression runs all
eighteen requests and verifies these results and exact preview/save bytes.

## Add calendar dates and story anchors

File 19 updates the existing world record; use its ID from `entity list --kind
world`. It also works on a blank initialized world, with no capability edit.
Preview and apply it with the same fingerprint/hash flow. The two calendars
share an explicitly authored axis epoch, and the Harbor Era skips display year
zero. Anchors associate axis day 0 with tick 0/order 0 and axis day 30 with
tick 1/order 1. They do not move the world cursor.

```sh
wedl-rs --repo my-world chronology calendars
wedl-rs --repo my-world chronology convert calendar_00000000000000000000000001 calendar_00000000000000000000000002 --year 0 --month 2 --day 1
wedl-rs --repo my-world chronology to-story calendar_00000000000000000000000001 --year 0 --month 2 --day 1
wedl-rs --repo my-world chronology from-story calendar_00000000000000000000000002 --tick 1 --order 1
```

Harbor date 0/2/1 converts to Tide date 3/1/1, both on axis day 30. Changing
the date to 0/2/2 has no anchor; changing the story order to 0 also has no
anchor. Neither lookup interpolates. A dedicated executable test previews and
applies file 19 and verifies the resulting conversion and exact source bytes.

Run `cargo run --locked --` in place of `wedl-rs` if using the source checkout.
Restart the browser server after edits to load the new source snapshot.

## Add dated and uncertain source claims

After file 19, preview/apply file 20 to Ana Reed. It needs the character and Bell
Rung event from earlier requests. It records an exact civil arrival, the same
day in an era, a range, an approximation, conflicting claims, a relative
reference, a duration and an open range. It changes no story tick or state.

```sh
wedl-rs --repo my-world chronology annotations "Ana Reed"
wedl-rs --repo my-world chronology annotations --era era_00000000000000000000000001
wedl-rs --repo my-world chronology compare "Ana Reed" chronology_00000000000000000000000001 "Ana Reed" chronology_00000000000000000000000002
```

The last query returns `equal`: both annotations represent axis day 30. Compare
the first annotation to the fifth, sixth or seventh to see `unknown` with an
explicit reason. Comparing it to the eighth returns `after`, because that open
range ends at axis day 29. Finite approximation bounds can establish overlap
but do not prove before/after. Use `chronology: []` in a character update to
remove the annotations while preserving other character fields and prose.

## Search dates, convert era inputs and inspect intervals

The same saved requests support the complete date-query workflow:

```sh
wedl-rs --repo my-world chronology format --era era_00000000000000000000000001 --year 1 --month 2
wedl-rs --repo my-world chronology convert-date --era era_00000000000000000000000001 --year 1 --month 2 --day 1 --target-calendar calendar_00000000000000000000000002
wedl-rs --repo my-world chronology search on_date --calendar calendar_00000000000000000000000001 --year 0 --month 2 --day 1 --record "Ana Reed"
wedl-rs --repo my-world chronology search overlaps --calendar calendar_00000000000000000000000001 --year 0 --month 2 --record "Ana Reed"
wedl-rs --repo my-world chronology story-times --calendar calendar_00000000000000000000000001 --year 0
```

Era year 1/month 2 formats as Harbor civil year 0/month 2, retaining month
precision. Its first day converts to Tide date 3/1/1 on axis day 30. The on-date
search finds the civil, era and range claims. The month-overlap search also
includes the finite approximation, with an explicit advisory. Conflicts,
relative references and durations remain unresolved.

The final query returns both explicit anchors inside Harbor year 0, with
`mapping:"ambiguous"`: tick 0/order 0 and tick 1/order 1. Both authored positions
are returned; no date-to-tick rate is inferred. Use an exact day or explicit
upper endpoint to narrow the interval. [CHRONOLOGY.md](../../docs/CHRONOLOGY.md)
describes all predicates, era filters and advanced typed operands.

## Add parentage and union histories

Files 21 and 22 add Rowan Reed and Jules Vale. Before file 23, extend the world
capabilities while preserving their registry order:

```yaml
capabilities: [generational-core-v1, spatial-core-v1, geometry-v1, route-v1]
```

If running only the family examples on a blank world, use
`[generational-core-v1]` and first create files 01, 02 and 04 for Harbor, Ana and
the Bell Rung event. Then create files 21–24 through the usual preview/hash
workflow. Refresh the fingerprint after the direct capability edit; requests
do not enable it automatically. The first twenty-six numbered requests together leave
fifteen source records, including the world record.

File 23 asserts Rowan's biological parentage of Ana at tick -40/order 0 and
confirms it at tick 2/order 0, caused by the strictly earlier Bell Rung event.
File 24 declares Rowan and Jules's union at tick -40/order 0 and forms it at
tick 0/order 0. Queries use exact inclusive story horizons:

```sh
wedl-rs --repo my-world generational family "Ana Reed" --tick 2 --order 0
wedl-rs --repo my-world generational family "Rowan Reed" --tick 2 --order 0
wedl-rs --repo my-world generational ancestors "Ana Reed" --tick 2
wedl-rs --repo my-world generational descendants "Rowan Reed" --tick 2
wedl-rs --repo my-world generational path "Ana Reed" "Jules Vale" --tick 2
wedl-rs --repo my-world generational history kinship_00000000000000000000000001 --tick 2
```

Ana's parent is Rowan. Rowan's family lists Ana as a child and the union with
Jules. The path from Ana to Jules is absent: a union supplies no parentage edge
or inferred second parent. Before the confirmation, the parentage lifecycle is
`asserted`; at tick 2/order 0 it is `confirmed`. These are author facts and do not
appear in character context, conversation or search without learned evidence.

Apply files 25 and 26 as updates to the IDs listed above. Their arrays retain
the prior confirmation/formation and add endings. Parentage stops contributing
at tick 10/order 0; the union stops at tick 11/order 0. Earlier family queries
still return their prior facts, and history at/after the endings shows all three
entries with lifecycle `ended`. An omitted array would preserve it, while
`transitions: []` would replace all later history with an empty array.

The executable tests verify these six new requests, exact preview/save bytes,
capability failures, corrections, invalid candidates and character-response
invariance. [GENERATIONAL.md](../../docs/GENERATIONAL.md) gives the complete source,
query and limit contract. Restart the browser server after saving to use the
Generations tab against the new snapshot.

## Add organizations, offices, claims and vital history

Files 27–32 use the same `generational-core-v1` declaration and preview/hash
workflow. They require Harbor, Ana, Jules and Bell Rung from earlier files. No
extra capability is needed. They create Harbor Guild, Ana's affiliation, the
Harbor Keeper office, an explicitly authored tenure, Jules's separate claim and
Ana's known birth history. All thirty-five numbered requests together leave
twenty-one source records, including the world record.

```sh
wedl-rs --repo my-world generational organization "Harbor Guild" --tick 2 --order 0
wedl-rs --repo my-world generational organization-legacies "Harbor Guild" --tick 2
wedl-rs --repo my-world generational legacy "Harbor Keeper" --tick 2
wedl-rs --repo my-world generational vital "Ana Reed" --tick -30 --order 0
```

Ana is a member from tick -40 with no specified role; at tick 2/order 0 the role
becomes watch keeper. Ana holds the office from tick 0. Jules's claim remains a
separate assertion and never appoints Jules. The vital query is unknown before
the birth at tick -30/order 0 and living at that point; it infers no age or
calendar date.

Apply files 33–35 as updates to the IDs in the table. The organization keeps its
source header selector `Harbor Guild`, while its historical `value.title` becomes
`Harbor Watch` at tick 3/order 0. The tenure is holding before tick 10/order 0,
vacant through tick 12/order 0 inclusively, and expired at tick 12/order 1. Neither
vacancy nor expiry chooses a replacement holder. Membership ends at tick 15/order
0; use `--include-former` to inspect that ended affiliation.

```sh
wedl-rs --repo my-world generational organization "Harbor Guild" --tick 15 --order 0 --include-former
wedl-rs --repo my-world generational history tenure_00000000000000000000000001 --tick 12 --order 1
```

The new tests run all nine requests through both the shared writer and CLI,
checking exact preview/save bytes, historical cutoffs, source preservation,
nullable payloads and refused invalid intermediate edits. Restart the snapshot
server after saves to use the browser's Generations tab.

## Add separately learned generational beliefs

Before file 36, explicitly extend the world capabilities and refresh its
fingerprint:

```yaml
capabilities: [generational-core-v1, spatial-core-v1, geometry-v1, route-v1, generational-knowledge-v1]
```

Files 36–42 create all seven typed assertion kinds, each first learned by Ana at
tick 4/order 0. They use explicitly learned names such as Annie, Jules the Elder,
The Lantern Guild and Keeper of the Light. These may differ from canonical names.
The parentage belief names Jules, while canonical example 23 names Rowan; both
can be queried without changing either source record. The tenure belief asserts
a vacant office independently of the canonical holder.

```sh
wedl-rs --repo my-world generational learned char_00000000000000000000000001 assertions --tick 4 --order 0
wedl-rs --repo my-world generational learned char_00000000000000000000000001 family Annie --tick 4 --order 0
wedl-rs --repo my-world generational learned char_00000000000000000000000001 organization "The Lantern Guild" --tick 4
wedl-rs --repo my-world generational learned char_00000000000000000000000001 history "Jules the Elder" --tick 8 --order 1
```

The first parentage applies through tick 8/order 0 inclusively. At order 1 it
leaves applicable family/context reads, while held history and learned-name
search still retain it. File 43 replaces the old record's transitions with its
original learning plus rejection at tick 9/order 0. File 44 creates a separate
accepted parentage belief at tick 9/order 1. Queries at earlier horizons still
return the older belief. Other learned assertions and canonical histories are
unchanged. A forgotten transition removes a held assertion in the same way.

All forty-four requests together leave twenty-nine source records including the
world. The new requests are exercised through the shared writer and CLI, including
exact preview/save bytes and refused invalid candidates. The browser has an
explicit Character viewpoint under Generations; enter Ana's exact viewer ID and
an exact endpoint ID or learned name. Restart the server after edits. See
[the learned contract](../../docs/GENERATIONAL_KNOWLEDGE.md) for evidence,
uncertainty, source restrictions and complete-result bounds.

## Extend geometry, anchors, portals and overlays

Before file 45, retain the earlier capabilities and explicitly add `overlay-v1`
in registry order. The complete declaration for all 57 examples is:

```yaml
capabilities: [generational-core-v1, spatial-core-v1, geometry-v1, route-v1, overlay-v1, generational-knowledge-v1]
```

Refresh the fingerprint after the direct source edit. Preview and apply files
45–55 as creates, then files 56–57 as updates to the exact IDs in the table.
Use the current fingerprint and each preview's returned hash for every save.
All 57 requests in numbered order leave 40 source records, including the world.
Retiring a record preserves its source rather than deleting it.

Files 45 and 46 place a line and a filled polygon on the existing Harbor Map.
Bounding-box queries compare their geometry bounds: `intersects` is the default,
while `within` requires the whole bounds to fit. Nearby queries measure the
distance to the actual line segments or filled polygon, so a point inside the
courtyard has distance zero.

```sh
wedl-rs --repo my-world spatial bbox "Harbor Map" --min-x -3 --min-y -3 --max-x 3 --max-y 3 --relation within
wedl-rs --repo my-world spatial nearby "Harbor Map" --x 5 --y 1 --radius 1
wedl-rs --repo my-world spatial nearby "Harbor Map" --x 1 --y 1 --radius 0
```

Files 47 and 48 add a map requiring a third coordinate and a watchtower at
`[3,4,2]`. Its bbox and nearby queries must include Z. Files 49 and 50 add a
geographic map with longitude/latitude axes and degree units. Its distances use
those coordinate degrees, without converting them to metres or geodesic lengths.

```sh
wedl-rs --repo my-world spatial bbox "Harbor Levels" --min-x 2 --min-y 3 --min-z 1 --max-x 4 --max-y 5 --max-z 3
wedl-rs --repo my-world spatial nearby "Harbor Levels" --x 3 --y 4 --z 2 --radius 0
wedl-rs --repo my-world spatial nearby "Harbor Coast Survey" --x 12.5 --y 55.7 --radius 0
wedl-rs --repo my-world spatial anchors --from-map "Harbor Map" --to-map "Harbor Coast Survey"
```

The final query reads file 51's explicit correspondence between `[0,0]` and
`[12.5,55.7]`, including its authored conversion note. An anchor records those
literal positions; it supplies no executable coordinate transformation and
creates no travel connection.

File 52 creates a directed foot portal from Harbor to the watchtower. Before
applying file 56, these queries show a one-hop path and include the watchtower
in Harbor's reachable locations:

```sh
wedl-rs --repo my-world spatial path Harbor "Harbor Watchtower" --mode foot
wedl-rs --repo my-world spatial reachable Harbor --mode foot
wedl-rs --repo my-world spatial location Harbor
```

The portal has no reverse edge or route metrics. Weighted path queries still use
authored routes. File 53's climb portal is visible under `outgoingPortals`, but
its explicit position target does not create a location edge, even though its
coordinates match the watchtower. File 56 retires only the portal from file 52;
the watchtower is then absent from Harbor's reachable set and the foot path
returns `found:false`. These graph reads inspect canonical source across story
time and take no horizon.

File 54 makes Harbor and the boardwalk members of the static Maintenance Area.
File 55 adds Harbor and Lower Landing to the Harbor Flood Warning from tick
10/order 0 through tick 12/order 0 inclusively on `main`. File 57 preserves the
start and extends the inclusive end to tick 14/order 0. After all requests:

```sh
wedl-rs --repo my-world spatial overlays Harbor --timeline main --tick 14 --order 0 --audience author --view author
wedl-rs --repo my-world spatial overlays Harbor --timeline main --tick 14 --order 1 --audience author --view author
wedl-rs --repo my-world spatial overlays Harbor --timeline main --tick 14 --order 0 --overlay overlay:harbor/flood-warning
```

The first query includes both overlays. The second includes only Maintenance
Area because it is beyond the warning's exact endpoint. The final query selects
the warning by exact ID. Supply explicit order 0 to inspect the inclusive end;
omitting order queries the end of the tick. Audience and view match the authored
overlay presentation labels and default to `author`. They do not select a
character viewpoint or provide character evidence; these remain author queries.
Restart the server after saves to use the browser's spatial controls against the
updated snapshot. See [the spatial contract](../../docs/SPATIAL.md) for source,
query and complete-result limits.

## Add exact spatial history

Before file 58, add `spatial-history-v1` explicitly and refresh the fingerprint.
The complete capability declaration for all 63 requests is:

```yaml
capabilities: [generational-core-v1, spatial-core-v1, geometry-v1, route-v1, overlay-v1, generational-knowledge-v1, spatial-history-v1]
```

Files 58–63 are updates to existing records. All 63 requests together still leave
40 source records, including the world. Each supplied `spatial_history` replaces
the complete array, so retain the earlier members when adding a correction.

File 58 places the landing at `[8,9]` from tick 20/order 0 through tick 29/order 0
inclusively. File 59 retains that member and corrects its position to `[9,10]`
with the same aspect and exact validity declaration. File 60 retains both and
adds an independent parentage member with `parent_id: null`, making the landing
an explicit root during that interval. Its static parent and `[3,4]` placement
remain unchanged.

After file 60 and before clearing the history with file 63:

```sh
wedl-rs --repo my-world spatial history "Lower Landing"
wedl-rs --repo my-world spatial history "Lower Landing" --tick 25 --order 0
wedl-rs --repo my-world spatial nearby map:harbor --x 9 --y 10 --radius 0 --tick 29 --order 0
wedl-rs --repo my-world spatial nearby map:harbor --x 3 --y 4 --radius 0 --tick 29 --order 1
```

The all-time read retains all three members. The dated read returns the corrected
placement and root parentage, citing the original/correction chain. The two
nearby queries demonstrate the inclusive endpoint and exact static fallback.
Omitting `--tick` keeps the geometry query static. Supplied order/timeline flags
require a tick; omitting order with a tick means the end of that tick.

File 61 gives Harbor Steps independent distance, duration and cost values over
ticks 20–29, with exact order 0 endpoints, and closed availability over ticks
24–26. File 62 retains those members and corrects the cost from 10 to 7 coins.
After file 62:

```sh
wedl-rs --repo my-world spatial route route:harbor/steps --tick 25 --order 0
wedl-rs --repo my-world spatial route route:harbor/steps --tick 29 --order 1
wedl-rs --repo my-world spatial path Harbor "Lower Landing" --metric duration --unit seconds
```

The dated inspector at 25/0 reports closed availability, 25 metres, 60 seconds
and 7 coins with exact field citations. After 29/0 it falls back to the static
30 metres and 45 seconds; cost becomes absent again. The ordinary weighted path
keeps its static 45-second result. Histories never silently change static paths,
reachability or containment inspection.

File 63 explicitly sets only the landing's `spatial_history` to `[]`. Its static
parent, placement, metadata and prose remain; the route history is unaffected.
Null is not a history deletion operation. These author histories do not grant
character evidence. Restart the snapshot server after saves to use the browser's
history inspector and optional exact geometry horizon. See
[the spatial contract](../../docs/SPATIAL.md) for source and query rules.

## Add typed lore profiles, concepts and explicit links

Before file 64, add `lore-core-v1` in its registry position, immediately before
`spatial-history-v1`, and refresh the fingerprint. The complete capability
declaration for all 71 requests is:

```yaml
capabilities: [generational-core-v1, spatial-core-v1, geometry-v1, route-v1, overlay-v1, generational-knowledge-v1, lore-core-v1, spatial-history-v1]
```

File 64 updates the existing world using its exact ID from
`entity list --kind world`. It replaces the complete profile array with two definitions: `harbor-symbol` and
`harbor-tradition`. Then create files 65 and 66, update Ana with file 67, update
the tradition with file 68, and update the world again with file 69. Use the
current fingerprint, inspect each preview's full candidate and validation, and
apply with that preview's `afterSha256`. All 71 requests, including the final
retirement and clearing steps, leave **42 source records**, including the world.

Lantern Mark is timeless. Lantern Launch applies on `main` from tick 10/order 0
through tick 20/order 0 inclusively. Its profile exercises all eight types,
including the explicit null `origin_story`, numeric offering amount, enum unit,
ordinary record references and a typed array referencing Lantern Mark. The
`values.chronology` string remains literal description governed by its profile;
it does not create a chronology annotation or story-time mapping.

File 68 supplies every required value again, changes the offering amount from
3 to 5, and adds the optional notes. Values replace the complete mapping. File
69 retains both profiles, updates the tradition's display label and adds the
optional string `season_label`. Existing concept values remain unchanged and
do not acquire a default. Every profile update validates all current dependent
concepts. An incompatible type change or removal of a referenced profile cannot
be saved. Profile definitions change directly within v1, without versions,
hash pinning, migrations or automatic dependent rewrites.

After file 69 and before retiring the symbol with file 70:

```sh
wedl-rs --repo my-world lore profiles
wedl-rs --repo my-world lore profile harbor-tradition
wedl-rs --repo my-world lore catalog
wedl-rs --repo my-world lore catalog --domain harbor.custom --tick 20 --order 0
wedl-rs --repo my-world lore catalog --domain harbor.custom --tick 20 --order 1
wedl-rs --repo my-world lore concept "Lantern Launch" --tick 14 --order 0
```

The profile catalogue contains two current definitions, and the all-time concept
catalogue contains two concepts. The two domain-filtered queries demonstrate
the inclusive end: Lantern Launch appears at 20/0 and is absent at 20/1. The
detail at 14/0 returns the amount as exact decimal text `"5"` with number type
metadata, its enum's scalar type, the explicit null, and cited references to the
landing, Ana and Lantern Mark. Profile reads are all-time registry operations;
they accept no horizon or audience/view flags. Other lore reads default to
author audience/view labels, and no horizon considers all authored timelines.

File 67 associates Ana with Lantern Launch from tick 12/order 0 through tick
16/order 0 inclusively, and with Lantern Mark timelessly. Before files 70–71:

```sh
wedl-rs --repo my-world lore links "Ana Reed" --tick 12 --order -1
wedl-rs --repo my-world lore links "Ana Reed" --tick 12 --order 0
wedl-rs --repo my-world lore links "Ana Reed" --tick 16 --order 0
wedl-rs --repo my-world lore links "Ana Reed" --tick 16 --order 1
```

These queries return one, two, two and one links respectively. Both the link's
own validity and the concept's validity must match the horizon. Supply explicit
order 0 to inspect an inclusive endpoint; omitted order means the end of that
tick. `--order` and `--timeline` require `--tick`. Audience/view filters and exact
domain filters run before complete-result limits, which fail rather than
truncate.

Apply file 70 as an update to Lantern Mark's exact ID in the table. It retires
Lantern Mark. The record and all authored references remain in
source, available through `entity show`, but semantic lore reads withhold the
retired concept. Lantern Launch still validates; its projected `values.symbols`
array is now empty and its typed reference metadata omits the symbol. Ana's
symbol link is also omitted, leaving only the applicable Lantern Launch link
at 14/0. This is a current status edit, not a dated retirement event.

File 71 explicitly replaces Ana's `concept_links` with `[]`. All of Ana's link
queries then return an empty list, including earlier story horizons; the writer
does not preserve a separate link-edit history. The concepts, profiles, Ana's
other fields and prose are retained. Top-level null is not a clearing operation.

These are author presentation reads. Profiles, concepts and links do not grant
character beliefs, traits, abilities, membership or authority, and do not enter
generic character context or search. Restart the fixed-snapshot server to use
the Lore tab after saving. [LORE.md](../../docs/LORE.md) documents the exact
type, validity, projection, citation and query contracts.

## Bind state values and compose common/individual descriptions

Files 72–79 use the same explicit `lore-core-v1` capability and the same complete
capability order shown above. No additional capability edit is required. All
**79 requests leave 47 source records**, including the world. Use the existing
preview/hash/apply flow for the operations in the table.

File 72 supplies complete replacements for `profiles`, `state_keys` and
`state_profile_bindings`. It retains the earlier lore profiles and declares
`species-common`, `species-individual` and `character-description-state`.
The binding is explicit:

```yaml
state_keys:
  character:
    mood: {type: string}
    species_notes: {type: string}
state_profile_bindings:
  character:
    species_notes: character-description-state
```

The profile governs the whole `species_notes` object, replacing the ordinary
leaf type for that declared key. Ordinary mood remains a string. Existing key
exclusivity still applies; binding an object does not enable set-item operations.
Definitions and bindings must exist before authoring their values. Later edits
must leave all dependent initial values and event effects valid, including old,
future and draft effects. Optional field additions create no defaults.

Files 73 and 74 create common description concepts with IDs ending in `03` and
`04`. Tide Folk has `parents: []`; River Folk explicitly references Tide Folk in
its typed `parents` array. File 75 creates Ana Individual Variation, ending in
`05`, with an exact `common` reference to River Folk. The variation applies from
10/0 through 20/0 inclusively on `main`. Its separate `individual` reference to
Ana describes the subject; it does not assign canonical traits or teach her lore.

File 76 supplies a complete initial-state mapping: ordinary mood plus a bound
object containing the variation reference, prose, an empty marker array, an
exact large number, declared null and literal `chronology` text. File 77 creates
a canonical event at 12/0 that replaces the whole object, adds a marker and
changes the number. These reads also work after the final clear event is saved:

```sh
wedl-rs --repo my-world lore state "Ana Reed" --tick 9 --order 0
wedl-rs --repo my-world lore state "Ana Reed" --tick 11 --order 0
wedl-rs --repo my-world lore state "Ana Reed" --tick 12 --order 0
wedl-rs --repo my-world state "Ana Reed" --tick 12 --order 0
```

At 9/0, the future variation reference is withheld while the other bound values
remain. At 11/0, it is visible and `measure` is the exact JSON string
`"9007199254740993"`. At 12/0, `measure` is `"9007199254740995"` and its state
citation points to `effects[0].value` in the set event. Used type shapes and
original typed-reference source paths accompany the values. The ordinary
`state` command returns only mood, with no bound-value citation; ordinary author
context uses the same omission. Raw author source remains inspectable. The
`lore state` command requires a tick; order defaults to the end of that tick,
and audience/view labels default to `author`.

Species queries select the common and individual profile IDs, reference fields
and reported roles explicitly. No profile or field spelling has implicit
biological meaning. For this sequence, compose Ana's variation with both common
accounts at 14/0:

```sh
wedl-rs --repo my-world species compose concept_00000000000000000000000005 \
  --common-profile species-common --individual-profile species-individual \
  --individual-link common --common-link parents \
  --common-fields '{"taxonomy":"taxonomy","life_stage":"life_stage","habitat":"habitats","traits":"traits","culture":"culture","language":"language","religion":"religion"}' \
  --individual-fields '{"traits":"traits","culture":"culture","language":"language"}' \
  --tick 14 --order 0 --audience author --view author
```

The result keeps Tide Folk, River Folk and Ana's individual statement separate.
It selects no winning trait and does not merge the common accounts into the
individual. Tide Folk's empty habitats and explicit null life stage/religion are
known authored values; its absent language is unknown. Ana's explicit null
language is likewise known. Unselected roles remain unknown with an unselected
reason. Field citations identify each statement's own source.

File 78 replaces only Tide Folk's common values. The composed common statement
then includes the revised broad-palms/short-nails wording, while Ana's variation,
its scar statement and the state's stored notes remain untouched. This source
edit applies to current reads at earlier story horizons too; it is not a dated
state event. The following queries find the explicitly linked variation and
demonstrate its inclusive validity boundary:

```sh
wedl-rs --repo my-world species affected concept_00000000000000000000000003 \
  --common-profile species-common --individual-profile species-individual \
  --individual-link common --common-link parents \
  --individual-fields '{"traits":"traits"}' --tick 20 --order 0 --limit 1
wedl-rs --repo my-world species affected concept_00000000000000000000000003 \
  --common-profile species-common --individual-profile species-individual \
  --individual-link common --common-link parents \
  --individual-fields '{"traits":"traits"}' --tick 20 --order 1 --limit 1
```

The first returns Ana Individual Variation; the second returns zero items.
Species horizons and author audience/view labels are optional. Without a horizon,
matching concepts span all authored timelines. Links must be declared typed
entity fields; prose, field names and ordinary concept links do not create this
composition graph. Cycles and exhausted work limits produce errors. Unsupported or absent
selected linkage is reported as unknown rather than guessed.

File 79 creates a clear effect at 18/0. Compare the exact boundary:

```sh
wedl-rs --repo my-world lore state "Ana Reed" --tick 18 --order -1
wedl-rs --repo my-world lore state "Ana Reed" --tick 18 --order 0
```

The first retains the amended notes. The second has no bound state value, type
shape or profile entry, but retains the clear event's citation at `effects[0]`.
Clear means actual absence; it neither inserts null nor deletes the concepts,
initial value or earlier event. Restart the snapshot server to read the saved
world using the state and species operations in the Lore tab. See
[LORE.md](../../docs/LORE.md) and [SPECIES.md](../../docs/SPECIES.md) for the exact
contracts and remaining character-admission boundaries.

## Compose explicitly authored magic accounts

Files 80–87 use the existing `lore-core-v1` capability. No new kind or capability
is introduced. File 80 updates the world with the entire nine-profile registry,
retaining all five earlier definitions. It omits state declarations, bindings
and chronology, so those fields remain intact. For a magic-only exercise, enable
`lore-core-v1` in a blank initialized world, apply file 80 to its world record,
create files 81–86, then update the tradition with file 87. These magic concepts
have no dependency on the earlier example characters.

Files 81–83 create a tradition, realm and artifact. Each practice explicitly
links to all three, and the artifact also links to the realm. The composition
returns each record once while retaining both visible realm edges. The three
practice intervals are main 0/0–9/0, 10/0–19/0 and 20/0–29/0. These are authored
story intervals; the names Early/Middle/Late do not declare a calendar era.

Select exact profile IDs and root fields for each role:

```sh
magic_profiles='{"tradition":"magic-tradition","practice":"magic-practice","realm":"magic-realm","artifact":"magic-artifact"}'
magic_links='{"tradition":["links"],"practice":["links"],"realm":["links"],"artifact":["links"]}'
magic_restrictions='{"tradition":"restrictions","practice":"restrictions","realm":"restrictions","artifact":"restrictions"}'
magic_exceptions='{"tradition":"exceptions","practice":"exceptions","realm":"exceptions","artifact":"exceptions"}'

wedl-rs --repo my-world magic compose concept_00000000000000000000000009 \
  --profiles "$magic_profiles" --links "$magic_links" \
  --restrictions "$magic_restrictions" --exceptions "$magic_exceptions" \
  --tick 5 --order 0 --audience author --view author
```

The result keeps practice, tradition, realm and artifact statements separate in
that traversal order. It has four records, four cited edges and complete linkage.
The realm's missing restriction is unknown, its null exception is known, and the
artifact's empty exceptions array is also known. These field states do not make
the explicit graph incomplete. Each practice restriction retains the exact
integer string `"9007199254740993"`, the numeric float `1.0`, and an ordinary
`{kind,id}` object naming a nonexistent concept. That object is declared as
literal strings, so it neither resolves nor becomes an edge. No count, ratio,
restriction or exception is interpreted as a cost, precedence rule or outcome.

The common tradition update in file 87 changes its composed wording at every
applicable practice horizon. It does not rewrite any practice file or change
its independent vow statement. Compare the later practice records with the same
selection:

```sh
wedl-rs --repo my-world magic compose concept_00000000000000000000000010 \
  --profiles "$magic_profiles" --links "$magic_links" \
  --restrictions "$magic_restrictions" --exceptions "$magic_exceptions" \
  --tick 10 --order 0
wedl-rs --repo my-world magic compose concept_00000000000000000000000011 \
  --profiles "$magic_profiles" --links "$magic_links" \
  --restrictions "$magic_restrictions" --exceptions "$magic_exceptions" \
  --tick 29 --order 0
```

The first practice is still available at 9/0 and unavailable at 9/1. Likewise,
the middle starts at 10/0 and the late practice ends inclusively at 29/0. An
omitted order defaults to the end of the selected tick, so use exact order zero
when checking these endpoints. With no horizon, composition can use any admitted
current authored interval, without inferring that different intervals coexist.

The first **87 requests leave 53 source records** and retain the earlier state, spatial
and character workflows. Restart the snapshot server after saving, select
**Magic and metaphysical traditions** in the Lore tab, and enter the same four role/profile/link
choices and exact horizon. [MAGIC.md](../../docs/MAGIC.md) documents unknowns,
source citations and complete-or-error bounds. The next examples add separate
dated sections and classified assertions. Social/environmental readers compose
these streams below. Requests 143–158 add generic character lore/section and
linked-assertion admission. Requests 176–203 demonstrate native dated accounts
in species/magic composition, independently admitted character family reads and
explicit family selection in writing frames. See
[Independently admitted family context](#independently-admitted-family-context).
Action capture and consequence workflows remain later work.

## Add dated sections and classified assertions

Before file 88, explicitly enable `descriptions-v1` and `assertions-v1` while
preserving capability registry order. For the complete sequence:

```yaml
capabilities: [generational-core-v1, spatial-core-v1, geometry-v1, route-v1, overlay-v1, generational-knowledge-v1, lore-core-v1, descriptions-v1, assertions-v1, spatial-history-v1]
```

Refresh the source fingerprint after this direct edit. Descriptions alone need
only `descriptions-v1`; the assertion examples also use lore concepts and the
three world predicates declared by file 88. Every numbered request still goes
through the same preview/hash/apply workflow.

Files 89 and 90 update Early Lantern Calling's explicit `descriptions` array.
Its opaque body and profile values remain untouched. The first two sections are
independent accounts over main 0/0–9/0. File 90 appends a third section that
explicitly replaces the first with exactly the same applicability and scopes.

```sh
wedl-rs --repo my-world descriptions list concept_00000000000000000000000009 --tick 5 --order 0
wedl-rs --repo my-world descriptions list concept_00000000000000000000000009
wedl-rs --repo my-world descriptions section concept_00000000000000000000000009 description_00000000000000000000000001
```

The dated list returns sections 02 and 03. The unrelated alternative remains,
and the correction cites its original `descriptions[2]` position. The history
list returns all three, and the exact history lookup retains section 01's
original Markdown. Adding `--tick 5 --order 0` to that old-section lookup returns
unavailable because it is replaced in the dated view. All horizons retain exact
inclusive order: the early concept and its sections are available at 9/0 and
unavailable as an owner at 9/1. Supplying `descriptions: []` in an ordinary update
would clear this complete array while preserving the body.

Files 91–93 create three separate propositions about the same practice:

| Assertion | Classification | Authored value |
| --- | --- | --- |
| `assertion_00000000000000000000000001` | `author-fact` | A typed rule object, including an exact large integer, null, typed links and a literal Ref-shaped object |
| `assertion_00000000000000000000000002` | `attributed-claim` | A literal account attributed explicitly to Ana |
| `assertion_00000000000000000000000003` | `unresolved` | Explicit null under the `practice-origin` predicate |

File 92's historical citation uses illustrative object IDs and a source path
that is absent from the example world. These are syntax examples, not verified
old Git content or live references. The author assertion read reports
`evidence: []` and cites the actual current assertion source. Its attribution is
independent of the claim's subject and does not imply that Ana has learned it.

File 94 adds a replacement fact while retaining the old assertion file. File 97
adds another correction of the same predecessor. Both corrections have exactly
the original subject, predicate and validity. Their differing values remain
alternatives; neither source order nor classification chooses a winner.

```sh
wedl-rs --repo my-world assertions predicates
wedl-rs --repo my-world assertions list concept_00000000000000000000000009 --tick 5 --order 0
wedl-rs --repo my-world assertions list concept_00000000000000000000000009
wedl-rs --repo my-world assertions list assertion_00000000000000000000000001
```

At the request-97 checkpoint, the predicate catalogue has three entries. The dated
practice query has four assertions: attributed claim 02, unresolved 03 and
corrections 04/05. The history query has all five. The last command inspects the
original fact in history; adding the dated horizon yields an empty list because
that fact is suppressed there. Unknown or invisible assertion targets likewise
return an empty list without exposing hidden names. The practice has four rows
at 9/0 and zero at 9/1.

The first fact's count remains the exact JSON string `"9007199254740993"`.
Correction 04 preserves `"9007199254740995"`. The literal `{kind,id}` object does
not become a reference, while actual links retain source paths such as
`value.links[1].id`. File 96's compatible optional `qualification` field supplies
no default; file 97 explicitly authors it in the new alternative. A predicate
change that invalidates an existing canonical, draft or retired assertion cannot
publish.

File 95 attaches a separately dated explanation and a calendar annotation to the
attributed assertion. The description starts exactly at main 5/0; the chronology
annotation is an authored civil date and does not change that story coordinate.

```sh
wedl-rs --repo my-world descriptions list assertion_00000000000000000000000002 --tick 5 --order -1
wedl-rs --repo my-world descriptions section assertion_00000000000000000000000002 description_00000000000000000000000001 --tick 5 --order 0
wedl-rs --repo my-world chronology annotations assertion_00000000000000000000000002
```

The first list is empty; the section lookup at 5/0 returns its exact Markdown and
citation. The chronology read returns one independent annotation. The local
section ID 01 is safe to reuse because its owner is part of its identity.

The **first 97 requests leave 58 source records**, including five assertions. The
existing magic, state, spatial and learned-character workflows remain intact.
Restart the snapshot server and open **Narrative** to use description lists,
exact section inspection, assertions and predicate definitions. Description and
assertion reads use a horizon for applicable accounts or omit it for admitted
history. Predicate definitions are current-only. These author-only examples
do not supply character learning. Requests 143–158 below add explicit acquisition
and scoped sections for the separate character readers.
[DESCRIPTIONS.md](../../docs/DESCRIPTIONS.md) and
[ASSERTIONS.md](../../docs/ASSERTIONS.md) give the precise source/query contracts.


## Social/language and environmental contexts

File 98 retains the earlier nine profiles and three predicates, then adds the
fifteen family profiles and three predicates from the two JSON fragments in
`examples/profiles/`. It replaces complete world declarations through the same
preview/hash/apply flow. The fragments do not install themselves or enable any
capability. The complete capability list above already covers these readers.
Existing source, bindings, state definitions and body prose are retained.

Files 99–109 create independent script, language, register, sense and spelling
records. River language is concept 13 with sense 16; Hill language is concept 14
with sense 17. Both may spell a word `bank`, while their sense glosses and source
identities remain different. River spellings have these inclusive main intervals:

| Concept suffix | Form | Validity |
| --- | --- | --- |
| 18 | `banc` | −20/0 through −1/0 |
| 19 | `bank` | 0/0 through 9/0 |
| 20 | `banka` | 10/0 through 20/0 |
| 21 | `banque` | 2/0 through 5/0, independently overlapping |

At 9/1 the current spelling has ended and the later one has not begun. The reader
reports the authored gap. Form matching is exact: case, Unicode and an explicit
empty string are preserved. A form filter selects spelling rows; it does not
remove the chosen language/sense or that language's separate register.

Files 110–112 create culture 23, religion 24 and membership 25, with explicit
diaspora and individual-exception description sections. Files 113–115 author a
doctrine, an attributed account and a compatible doctrine correction. The old
assertion stays in source; the correction and attributed account remain separate.

```sh
wedl-rs --repo my-world social context concept_00000000000000000000000013 --sense concept_00000000000000000000000016 --context concept_00000000000000000000000023 --context concept_00000000000000000000000024 --context concept_00000000000000000000000025 --tick 3 --order 0
wedl-rs --repo my-world social context concept_00000000000000000000000013 --sense concept_00000000000000000000000016 --form bank --tick 3 --order 0
wedl-rs --repo my-world social context concept_00000000000000000000000014 --sense concept_00000000000000000000000017 --form bank --tick 3 --order 0
```

The first query returns nine records, including both `bank` and `banque`, two
explicit sections and two doctrine accounts. The other queries return different
River/Hill sense and spelling records. There is no inferred translation, fluency,
shared identity or membership from the matching letters.

Files 116–122 create era 26, resource 27, habitat 28, technology 29, infrastructure
30, livelihood 31 and trade practice 32. Resource quantity is literal wording,
including exact text `9007199254740993`; it does not become an inventory or
arithmetic value. The trade root names direct resources, infrastructure and
livelihood dependencies, plus a timed habitat link valid from 2/0 through 5/0.

Files 123–125 add an author availability account, a contrary attributed account
and an unresolved dependency account. These assertions do not open or close a
route, calculate production, or revise any learned belief.

```sh
wedl-rs --repo my-world environment context --context concept_00000000000000000000000032 --tick 3 --order 0
wedl-rs --repo my-world environment context --context concept_00000000000000000000000032 --context concept_00000000000000000000000028 --tick 3 --order 0
wedl-rs --repo my-world environment context --context concept_00000000000000000000000028 --tick 9 --order 0
wedl-rs --repo my-world environment context --context concept_00000000000000000000000028 --tick 9 --order 1
```

The first query returns five records, four direct links, three accounts, one
competing availability cohort and one explicit unresolved dependency. Its direct
workshop endpoint does not expand into technology 29. Selecting habitat 28 as an
additional root includes its own direct era link and dated section. The last
two reads retain the same habitat concept but show its section at 9/0 and an
empty section list at 9/1. Missing accounts remain unknown, without a falsehood
or absence inference.

The **125-request checkpoint leaves 85 source records**, with **24 profiles and six
predicates**. Restart the snapshot
server and choose **Language and culture** or **Environment and materials**
in the Narrative tab. Both require an exact horizon; their complete result bounds
are fixed, and their controls omit unrelated selectors. Existing description and
assertion reads retain their optional history horizons. Specialized character
family composition and exact writing capture are implemented in the final
section below. Manuscript/reader and action outcomes remain planned. See [SOCIAL.md](../../docs/SOCIAL.md)
and [ENVIRONMENT.md](../../docs/ENVIRONMENT.md) for the exact query contracts.

## Authored existence and governance

Before request 126, add `existence-v1` and `collective-parties-v1` to the world's
explicit capability list. The full sequence then uses:

```yaml
capabilities:
  - generational-core-v1
  - spatial-core-v1
  - geometry-v1
  - route-v1
  - overlay-v1
  - generational-knowledge-v1
  - lore-core-v1
  - descriptions-v1
  - assertions-v1
  - existence-v1
  - collective-parties-v1
  - spatial-history-v1
```

Request 126 preserves all 24 existing profiles and six predicates, adds the
`harbor-policy` profile and three policy predicates, and leaves state bindings
unchanged. The matching `examples/profiles/harbor-governance.json` fragment is
source documentation, not an automatic installer or replacement of prior source.

Requests 127–132 create six episodes for Ana: two independent embodied accounts,
one disembodied account, a later constructed account, an unbounded `other`
account and an unbounded `unknown` account. Affirmative starts require evidence
even if unbounded; bounded ends also require evidence. Unknown uses empty start
evidence and no end evidence. The illustrative historical pointers are explicitly
unverified and do not need matching current records or a Git repository. They
never become live evidence about Ana or modify the earlier vital history.
Request 133 adds common calendar/date and description source to the first episode.

```sh
wedl-rs --repo my-world existence list char_00000000000000000000000001 --tick 5 --order 0
wedl-rs --repo my-world existence list char_00000000000000000000000001 --tick 9 --order 0
wedl-rs --repo my-world existence list char_00000000000000000000000001 --tick 9 --order 1
wedl-rs --repo my-world existence list char_00000000000000000000000001 --tick 20 --order 0
wedl-rs --repo my-world existence list char_00000000000000000000000001
wedl-rs --repo my-world descriptions list existence_00000000000000000000000001 --tick 5 --order 0
wedl-rs --repo my-world chronology annotations existence_00000000000000000000000001
```

The five episode reads return **5, 4, 3, 3 and 6** records respectively. The
description and chronology reads each return one member. Overlapping episode
kinds remain independent authored accounts; no winning body, age or resurrection
is inferred. Omitting the horizon retains history across all authored timelines.

Requests 134–137 create legal, de-facto, claimed and contested policy accounts,
using character, organization and lore-concept parties. The profile fields are
ordinary typed data until a query explicitly selects their roles:

```sh
ROLES='{"territory":"jurisdiction","party":"issuer","basis":"basis","subjects":"subjects","obligations":"duties","supersedes":"supersedes"}'
wedl-rs --repo my-world governance query loc_00000000000000000000000001 --profile harbor-policy --roles "$ROLES" --tick 12 --order 0
wedl-rs --repo my-world governance query loc_00000000000000000000000001 --profile harbor-policy --roles "$ROLES" --tick 14 --order 0
wedl-rs --repo my-world governance query loc_00000000000000000000000001 --profile harbor-policy --roles "$ROLES" --tick 14 --order 1
wedl-rs --repo my-world governance query loc_00000000000000000000000001 --profile harbor-policy --roles "$ROLES"
NULL_ROLES='{"territory":"jurisdiction","party":"issuer","basis":"basis","subjects":"subjects","obligations":"unresolved","supersedes":"supersedes"}'
wedl-rs --repo my-world governance query loc_00000000000000000000000001 --profile harbor-policy --roles "$NULL_ROLES" --tick 12 --order 0
```

The first four reads return **4, 4, 3 and 3** policies. The claimed policy follows
the earlier flood-warning overlay's exact inclusive interval (10/0 through
14/0); it is omitted without a horizon. The static maintenance overlay matches
only its direct members. Location jurisdiction uses exact equality and does not
inherit through containment. The final query retains four policies and binds
obligations to `unresolved`: only the de-facto policy has a known null; the other
three have an unknown missing field. Null binding values themselves select no
field. All six binding keys are required, and non-null fields must be unique.

Requests 138–142 add native assertion evidence. At 12/0, the legal policy carries
assertions 13 and 15; all-time history additionally retains predecessor 12.
Assertion 13 links through a root typed entity value, while literal Ref-shaped
assertion 16 does not join the policy. The contested policy retains its own
attributed assertion 14. Its supersedes pointer cites the legal policy without
removing that policy, choosing a winner or changing any character belief.
The exact integer duty amount remains `"9007199254740993"` on the JSON wire,
with the original type and field citation. Literal null and absent fields remain
distinct through `roleKnown` and `roleTypes`.

The **142-request checkpoint leaves 100 source records**, with **25 profiles and nine
predicates**. At that checkpoint the writer supports **23 non-world kinds**. Restart the immutable
snapshot server and select **Existence episodes** or **Authority and obligations**
in the Narrative tab. Optional horizons retain history; role controls use blank
for unbound null. The character extension below uses separate admission rules. See
[EXISTENCE.md](../../docs/EXISTENCE.md) and [GOVERNANCE.md](../../docs/GOVERNANCE.md)
for source validation, whole-policy admission and complete-result limits.

## Shared character narrative admission

Requests 143–158 retain the same explicit capabilities, 25 profiles and nine
predicates. They add 12 records through the existing reviewed writer; the four
updates replace one complete field each while preserving unrelated source.
The **158-request checkpoint leaves 112 valid source records**, with **23 editable
non-world kinds** at that checkpoint. These requests require no new profile,
source kind, migration or version.

Concept 37 is a public resource context with a private dependency and an
independently learnable public technology, concept 38. Both use the exact domain
`narrative.reading` and apply from main 10/0 through 29/0. Ana acquires concept
37 at 10/0, acquires concept 38 at 13/0, forgets concept 38 at 20/0 and forgets
concept 37 at 22/0. A concept's link or typed reference cannot teach its endpoint.
The private dependency is never returned, and returned references have no hidden
source-array indices or author citations.

```sh
wedl-rs --repo my-world character lore char_00000000000000000000000001 --domain narrative.reading --tick 10 --order 0
wedl-rs --repo my-world character lore char_00000000000000000000000001 --domain narrative.reading --tick 13 --order 0
wedl-rs --repo my-world character concept char_00000000000000000000000001 concept_00000000000000000000000037 --tick 13 --order 0
wedl-rs --repo my-world character links char_00000000000000000000000001 concept_00000000000000000000000037 --tick 12 --order 0
wedl-rs --repo my-world character links char_00000000000000000000000001 concept_00000000000000000000000037 --tick 13 --order 0
```

The two catalogues contain **one then two** concepts. At 13/0, concept 37's
dependencies contain only concept 38; the private dependency stays absent. The
two outgoing-link reads return **zero then one**. At 20/0 only concept 37 remains
known, and at 22/0 the catalogue is empty. Exact unavailable concept/owner reads
use the same generic error as missing targets.

Knowledge 10 stores the literal report “Residents told me the ferry runs all
night.” with a link to assertion 17. The canonical rule and its correction in
request 154 are separate author content. The learned result never imports their
wording, applicability, status, evidence or subject. The initial rejection at
10/0 supplies no learning; suspicion at 11/0 does. A rejection at 12/0 retains
the already learned account, forgetting at 14/0 hides it, remembering at 15/0
restores it, and forgetting at 17/0 hides it again. The author-only report and
the never-learned rejection in request 157 remain excluded throughout.

```sh
wedl-rs --repo my-world character assertions char_00000000000000000000000001 assertion_00000000000000000000000017 --tick 12 --order 0
wedl-rs --repo my-world character assertions char_00000000000000000000000001 know_00000000000000000000000010 --tick 14 --order 0
wedl-rs --repo my-world character descriptions char_00000000000000000000000001 concept_00000000000000000000000037 --tick 12 --order 0
wedl-rs --repo my-world character section char_00000000000000000000000001 concept_00000000000000000000000037 description_00000000000000000000000002 --tick 12 --order 0
```

The assertion reads return **one rejected account then zero**. The section list
returns **two** entries: the corrected morning-bell notice and the independent
alternative. The exact section lookup returns the correction. The private
section never appears, and the winter section appears only from 20/0. Ana's own
dated section is independently available from 10/0 through 19/0. No untimed
concept, knowledge or character body is included.

Harbor becomes known at 10/0, while its map becomes known at 15/0 and is forgotten
at 18/0. The two placement members cover 10/0 through 19/0; the admitted
correction replaces [2,3] with [3,4]. Endpoint admission occurs before suppression.

```sh
wedl-rs --repo my-world character spatial-history char_00000000000000000000000001 loc_00000000000000000000000001 --tick 14 --order 0
wedl-rs --repo my-world character spatial-history char_00000000000000000000000001 loc_00000000000000000000000001 --tick 15 --order 0
wedl-rs --repo my-world character spatial-history char_00000000000000000000000001 loc_00000000000000000000000001 --tick 18 --order 0
wedl-rs --repo my-world character governance char_00000000000000000000000001 loc_00000000000000000000000001 --tick 12 --order 0
wedl-rs --repo my-world character existence char_00000000000000000000000001 char_00000000000000000000000001 --tick 12 --order 0
wedl-rs --repo my-world character context char_00000000000000000000000001 --tick 12 --order 0 --max-characters 100000
wedl-rs --repo my-world character search char_00000000000000000000000001 'morning bell' --tick 12 --order 0
```

Spatial counts are **zero, one, zero**; the one result uses exact wire coordinates
`["3","4"]`. Governance contains the held report with jurisdiction unknown, basis
null and an empty party list; it does not classify the report by current policy.
Existence returns no typed episode payload, including after the ordinary episode
pointer is learned. At the 158-request checkpoint, context contains **three** admitted sections at 12/0, **one**
at 20/0, and no `descriptions` member at 22/0. The search returns the morning-bell
description; learned words use their separate `assertion-belief` search kind.
Private and future words contribute no search rows or packet-budget usage.

Restart the snapshot server and select any of the nine **Character** operations
in the Narrative tab. An exact viewer and horizon are required. Author controls
are disabled and omitted for these reads. See
[NARRATIVE_ADMISSION.md](../../docs/NARRATIVE_ADMISSION.md),
[LEARNED_ASSERTIONS.md](../../docs/LEARNED_ASSERTIONS.md) and
[CHARACTER_VIEWS.md](../../docs/CHARACTER_VIEWS.md) for the precise contracts.

## Authored briefs and exact writing frames

Before request 161, append `briefs-v1` to the world's explicit capability list
and refresh the source fingerprint. The 175-request checkpoint uses:

```yaml
capabilities:
  - generational-core-v1
  - spatial-core-v1
  - geometry-v1
  - route-v1
  - overlay-v1
  - generational-knowledge-v1
  - lore-core-v1
  - descriptions-v1
  - assertions-v1
  - existence-v1
  - collective-parties-v1
  - spatial-history-v1
  - briefs-v1
```

The new source kind itself requires only `briefs-v1`; these examples also depend
on the earlier authored concepts, sections, assertions and learning. Requests
159–175 use the same reviewed writer. They add 13 records and four updates,
leaving **125 valid records, 25 profiles and nine predicates**. The writer now
supports **24 non-world kinds**. No request changes a cursor or enables a
capability implicitly.

The new brief prose and sections apply from main 10/0 through 19/0, inclusively.
Ana separately acquires each of the six briefs at 11/0. The region brief names
Harbor as its scope reference; that reference must be independently admitted
but is not emitted as an implicit required item. Its detail brief requires
concept 37's exact corrected section, knowledge 1's `first-observation`
transition, and the bell's dated section. The parent additionally requires
Ana's section and the reciprocal brief. A repeated section is emitted once and
the reciprocal cycle terminates, retaining **seven** first-seen exact targets.

```sh
wedl-rs --repo my-world briefs compose brief_00000000000000000000000002 --tick 12 --order 0
wedl-rs --repo my-world character brief char_00000000000000000000000001 brief_00000000000000000000000002 --tick 12 --order 0
wedl-rs --repo my-world character brief char_00000000000000000000000001 brief_00000000000000000000000003 --tick 12 --order 0 --limit 1
wedl-rs --repo my-world character brief char_00000000000000000000000001 brief_00000000000000000000000004 --tick 12 --order 0
wedl-rs --repo my-world character brief char_00000000000000000000000001 brief_00000000000000000000000004 --tick 14 --order 0
wedl-rs --repo my-world briefs compose brief_00000000000000000000000005 --tick 12 --order 0
wedl-rs --repo my-world character brief char_00000000000000000000000001 brief_00000000000000000000000005 --tick 12 --order 0
```

The author parent returns seven items. Its standalone character read is unknown:
no scene is implicitly selected and the bell has not independently been learned.
The private required pin makes brief 3 wholly unknown even with limit 1; it cannot
consume budget or leave a public partial result. Brief 4 returns two items at
12/0: its prose and the learned rejected report “Residents told me the ferry runs
all night.” Forgetting that report makes the whole brief unknown at 14/0.
The corrected canonical assertion in brief 5 produces two author items but no
character result. Knowing a brief never teaches its required references.

The new scene explicitly places Ana and the bell at Harbor. Rowan joins at 13/0.
Selecting this scene in a character writing frame admits the bell independently
of the pins, so the region brief's seven-item closure becomes available. Untimed
scene/knowledge bodies and acquisition notes do not enter the character prompt.

Capture the current source fingerprint into a separate local operational file
after completing the requests. Run these commands from a working directory
outside `my-world/story`; frame JSON is not a story record:

```sh
wedl-rs --repo my-world status > writing-status.json
python3 - <<'PY'
import json
from pathlib import Path
status = json.loads(Path('writing-status.json').read_text())
frame = {
    'fingerprint': status['fingerprint'],
    'work': None,
    'sceneId': 'scene_00000000000000000000000002',
    'povId': 'char_00000000000000000000000001',
    'at': {'timeline': 'main', 'tick': '12', 'order': '0'},
    'lens': 'character',
}
for lens in ('character', 'author', 'dramatic-irony'):
    frame['lens'] = lens
    Path(f'{lens}-frame.json').write_text(json.dumps(frame, indent=2) + '\n')
PY
wedl-rs --repo my-world writing resolve --file character-frame.json
wedl-rs --repo my-world writing choices --file character-frame.json --limit 100
wedl-rs --repo my-world writing context --file character-frame.json --brief brief_00000000000000000000000002 --max-characters 100000
wedl-rs --repo my-world writing context --file dramatic-irony-frame.json --brief brief_00000000000000000000000002 --max-characters 100000
wedl-rs --repo my-world writing context --file author-frame.json --brief brief_00000000000000000000000002 --max-characters 100000
```

Resolve returns the exact frame. At 12/0 the character POV choices exclude Rowan,
and moments after 12/0 are absent. The character context contains seven required
items, including the bell's “blue cord” section. Dramatic irony returns the exact
same `characterPrompt` plus a separate `authorMargin`; the author lens returns
only the margin. Budgets count the complete compact result, including selected
briefs and frame fields, and fail instead of truncating. Any source change makes
the old fingerprint stale until the caller explicitly captures a new one.

Restart the snapshot server after source edits. The Narrative tab has author
and character brief reads; the Writing tab has tab-local frame controls,
resolve/choices/context, strict local-file loading and frame download. Changing
the lens, exact time, scene, POV, pin or selected briefs clears prior results.
The browser never publishes source or moves shared cursors. See
[BRIEFS.md](../../docs/BRIEFS.md) and
[WRITING_FRAMES.md](../../docs/WRITING_FRAMES.md) for source/member admission,
frame validation, explicit family context and remaining action workflows.


## Independently admitted family context

Requests 176–203 need no additional capability, source field or profile. Their
12 concept updates preserve omitted metadata, typed values and body bytes. The
14 learning records and two canonical assertions are separate creations. The
**203-request checkpoint leaves 141 valid records**, with **25 profiles,
nine predicates and 24 editable non-world kinds**.

Public presentation labels and learning are independent. Ana learns the
individual account and its direct River Folk common account at 11/0; the ancestral
Tide Folk account is learned only at 13/0 and forgotten at 17/0. Reading a typed
link never acquires its endpoint. The later ancestral acquisition admits its
corrected dated coastal section; its private alternative remains excluded.

```sh
read_species() {
  wedl-rs --repo my-world character species char_00000000000000000000000001 concept_00000000000000000000000005 \
    --common-profile species-common --individual-profile species-individual \
    --individual-link common --common-link parents \
    --common-fields '{"traits":"traits"}' --individual-fields '{"traits":"traits"}' \
    --tick "$1" --order 0
}
read_species 12
read_species 13
read_species 17
wedl-rs --repo my-world character species-affected char_00000000000000000000000001 concept_00000000000000000000000003 \
  --common-profile species-common --individual-profile species-individual \
  --individual-link common --common-link parents --tick 13 --order 0 --limit 1
```

The common lists have **one, two, one** entries. The individual scar statement
never changes or becomes a winning common trait. The affected read returns the
one independently known, explicitly linked variation. Character statements
exclude current titles, untimed bodies, source positions and author citations.
The author species reader now additionally attaches native `canon` and
`sections` streams for the actual composed owners when those capabilities exist.

The language, sense, later spelling and religion are acquired at 11/0. The script
is independently learned at 13/0 and forgotten at 17/0. The ceremonial register
and culture remain private. The optional belief target selects a separately
learned report, without inferring its subject from the canonical assertion.

```sh
read_social() {
  wedl-rs --repo my-world character social char_00000000000000000000000001 concept_00000000000000000000000013 \
    --sense concept_00000000000000000000000016 --form banka \
    --context concept_00000000000000000000000024 \
    --belief-target assertion_00000000000000000000000008 --tick "$1" --order 0
}
read_social 12
read_social 13
```

The reads contain **four then five** records. The fifth is the script. Exact
spelling and sense identity remain separate; no translation or fluency is
inferred. The literal report “Someone told me river gatherings forbid boats.”
remains a rejected held account at 12/0, without importing canonical doctrine or
classifying it through the current assertion's subject. Forgetting at 16/0
removes that report from its selected stream.

Trade and resource records are acquired at 11/0, the livelihood at 13/0, and the
resource is forgotten at 17/0. Only admitted direct links enter the environmental
report; the livelihood's own technology dependency is not recursively selected.

```sh
read_environment() {
  wedl-rs --repo my-world character environment char_00000000000000000000000001 \
    --context concept_00000000000000000000000032 \
    --belief-target assertion_00000000000000000000000009 --tick "$1" --order 0
}
read_environment 12
read_environment 13
read_environment 17
```

Record counts are **two, three, two**, with **one, two, one** direct links. The
resource quantity remains literal text `9007199254740993`. The explicitly
selected learned report says “I was told no reed carts may reach the dock.”
It is separate from canonical availability, production, costs and route state.

Character magic selects explicit learned assertion/knowledge targets. It does
not read canonical magic role profiles or infer which rule a character knows.
Request 203 corrects the canonical middle-practice account, while knowledge 37
retains its own statement: “I learned that middle calling needs no witness.”

```sh
wedl-rs --repo my-world character magic char_00000000000000000000000001 \
  --belief-target assertion_00000000000000000000000019 --tick 12 --order 0
wedl-rs --repo my-world character magic char_00000000000000000000000001 \
  --belief-target know_00000000000000000000000037 --tick 16 --order 0
wedl-rs --repo my-world magic compose concept_00000000000000000000000010 \
  --profiles '{"practice":"magic-practice"}' \
  --restrictions '{"practice":"restrictions"}' --exceptions '{"practice":"exceptions"}' \
  --tick 12 --order 0
```

The character reads return **one rejected account then zero** after forgetting.
The author read includes the separate corrected canonical assertion 20 and the
corrected dated practice section. Neither supplies replacement character words.
Unknown learned targets stay empty; no canonical fallback is used.

### Capture all four families

Families are an optional closed context request, separate from the six-field
frame. The [focused selection](../writing/family-context-focused.json) selects a
direct species common account, one magic practice, language/sense and one
resource root. It includes explicit learned targets but no inferred knowledge.
The [broader selection](../writing/family-context.json) also follows the common
ancestry, author magic links, religion context and trade's direct dependencies.
Both are complete selections with intentionally different scope.

After request 203, recapture the exact source fingerprint; files captured before
these edits are stale. These operational files belong outside the story source:

```sh
wedl-rs --repo my-world status > writing-status.json
python3 - <<'PYFRAME'
import json
from pathlib import Path
status = json.loads(Path('writing-status.json').read_text())
frame = {
    'fingerprint': status['fingerprint'],
    'work': None,
    'sceneId': 'scene_00000000000000000000000002',
    'povId': 'char_00000000000000000000000001',
    'at': {'timeline': 'main', 'tick': '12', 'order': '0'},
    'lens': 'character',
}
for lens in ('character', 'author', 'dramatic-irony'):
    frame['lens'] = lens
    Path(f'{lens}-frame.json').write_text(json.dumps(frame, indent=2) + '\n')
PYFRAME
families="$(cat examples/writing/family-context-focused.json)"
for lens in character author dramatic-irony; do
  wedl-rs --repo my-world writing context --file "$lens-frame.json" \
    --families "$families" --max-characters 100000
done
wedl-rs --repo my-world writing context --file character-frame.json \
  --families "$(cat examples/writing/family-context.json)" --max-characters 100000
```

All focused lenses fit the existing 100000-character service bound in the
completed example world. Dramatic irony includes the same complete
`characterPrompt` as the corresponding character read plus a separate author
margin. A full four-family dramatic read with the broader selection exceeds
that bound and fails without returning partial content; the broader character
and author reads each fit separately. Select a narrower explicit context when
the complete requested packet is too large. No automatic ranking or truncation
changes the requested evidence.

An unavailable required family root or brief pin refuses the entire context,
including when another component exceeds its bound. Optional native unknown
linkage stays explicit, and unknown selected beliefs stay empty. These reading
packets do not establish action readiness, evaluate requirements or publish
outcomes.

The Lore and Narrative tabs expose the separate character family operations.
**Use selection in Writing** deliberately captures that form's family; it does
not automatically copy unrelated controls. The Writing summary allows individual
removal or clearing. Magic author and learned selections remain separate so a
dramatic frame can include both. Editing a family form invalidates the old result
and pending response. Source, shared cursors and the six-field frame remain
unchanged by family selection.


## Local semantic and hybrid retrieval

Requests **204–207** use the existing description and knowledge grammars. Apply
204 to `WORLD_ID`; the other three are creates. The complete sequence now has
**207 requests and 144 valid source records**, with the same 25 profiles, nine
predicates and 24 editable non-world kinds.

Request 204 supplies six public, ordinary, timeless world sections as a fixed
local LSA prior. Its private and interval sections are excluded from training.
The separately authored body is available only to author source search. The
world's training sections do not become character hits or teach the world record.
No source or API version, profile, capability, dependency or external model is
added by retrieval.

Author search retains its all-time title/body corpus and excludes hypotheses:

```sh
wedl-rs --repo my-world search ferry --mode semantic --limit 100
wedl-rs --repo my-world search ferry --mode hybrid --limit 100
```

Both reads include the new ferry notice, with separate lexical/semantic ranks
and scores. The semantic score is cosine similarity; the hybrid score combines
rank positions. Neither is a truth judgment or an authored confidence value.

Ana's learned account says “Residents told me the ferry needs no captain.”
The public prior connects *vessel* with *ferry*, without replacing those learned
words or joining them to the canonical assertion. At 20/0, literal search for
`vessel` has no hits, while semantic search recalls that literal account:

```sh
wedl-rs --repo my-world character search char_00000000000000000000000001 vessel --mode lexical --tick 20 --order 0 --limit 100
wedl-rs --repo my-world character search char_00000000000000000000000001 vessel --mode semantic --tick 20 --order 0 --limit 100
```

At 21/0 Ana independently acquires the notice, admitting its dated public
section. At 22/0 she rejects the separately learned account, which remains a held
literal report. Neither transition makes the private notice searchable:

```sh
wedl-rs --repo my-world character search char_00000000000000000000000001 vessel --mode hybrid --tick 21 --order 0 --limit 100
wedl-rs --repo my-world character search char_00000000000000000000000001 vessel --mode hybrid --tick 22 --order 0 --limit 100
```

At 24/0 the notice acquisition is forgotten, so its section leaves character
search even though its own validity continues through 29/0. At 26/0 the learned
account is separately forgotten:

```sh
wedl-rs --repo my-world character search char_00000000000000000000000001 vessel --mode semantic --tick 24 --order 0 --limit 100
wedl-rs --repo my-world character search char_00000000000000000000000001 vessel --mode semantic --tick 26 --order 0 --limit 100
```

Other admitted evidence may still appear. For a nonzero query projection,
semantic candidates include zero and negative similarities, sorted by score and
stable identity. `total` counts the complete candidate union before the result
limit; it is not a count of confirmed facts or exact word matches. The fixed
public prior never includes target documents, private/timed sections or owner
metadata in its fitting statistics.

After restarting the immutable snapshot server, select the character perspective
and retrieval mode in Search, then use the same exact viewer, query and horizon.
Changing those inputs invalidates older results and pending responses. Without
an eligible public prior, character semantic and hybrid modes fail explicitly;
lexical search remains available. Complete model/input bounds also fail without
silently switching modes. These reads write no source, model or persistent index.
See [RETRIEVAL.md](../../docs/RETRIEVAL.md) for the precise contract.
