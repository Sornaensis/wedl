# Coordinated scene and movement controls

These author operations construct one complete ordinary consequence Plan from
one pinned source snapshot. Previewing is read-only. Review and publication use
the existing [Actions](ACTIONS.md) and [guarded outcome writer](CONSEQUENCES.md).
No new source schema or publication path is introduced.

## Request and interfaces

`scene-control preview --file FILE [--max-characters N]` accepts closed JSON;
`--file -` reads stdin. The equivalent read endpoint is
`GET /api/scene-control/preview?request=JSON&maxCharacters=N`. The native functions
are `scene_control::decode`, `plan` and `preview`. The Writing browser provides
forms for all four operations, request import/export, complete previews and Plan
export/transfer to Consequences.

```json
{
  "expectedFingerprint": "<exact lowercase source fingerprint>",
  "at": {"timeline": "main", "tick": "9007199254740993", "order": "8"},
  "action": {
    "type": "character.move",
    "characterIds": ["char_00000000000000000000000001"],
    "locationId": "loc_00000000000000000000000002",
    "sceneId": "scene_00000000000000000000000002",
    "event": {"id": "event_00000000000000000000000001", "title": "Arrival"}
  }
}
```

The fingerprint placeholder must be replaced with the captured source value.
All IDs and time coordinates are explicit. `tick` and `order` are canonical
signed decimal strings within i64/i32; the timeline must be declared. Unknown,
duplicate and missing fields are refused. Optional fields use omission; explicit
null is not an alternate spelling. New scene and event IDs must be unused.
The complete request is limited to 16 KiB and each title to 1,024 UTF-8 bytes.

| `action.type` | Required fields | Optional fields |
| --- | --- | --- |
| `scene.create` | `sceneId`, `title`, `locationId`, nonempty distinct `characterIds`, `event` | None |
| `scene.advance` | `sceneId` | `locationId`, `event` |
| `scene.close` | `sceneId` | None |
| `character.move` | Nonempty distinct `characterIds`, `locationId`, `event` | `sceneId` |

`event` is always `{id,title}` for a **new** canonical movement event. Each moving
character receives one independent `location = {entity: destination}` effect.
Effect IDs are deterministically derived from the explicit event and character
IDs. The planner does not select or rewrite an existing movement event, alter
object state, append scene outcome links, create knowledge, or infer a route.

## Scene lifecycle and presence

**Create** ends the selected characters' eligible presences in other active
scenes, transfers their listed carried objects, and sets the world and every
existing active scene cursor to `at`. It creates an active scene whose start and
current time are `at`, with the selected participants and transferred objects,
plus the explicit movement event.
Creation cannot precede an authored world cursor or change its timeline. If no
world cursor has been authored, creation introduces that cursor at the explicit
requested point; an implicit zero is not used as a lower bound.

**Advance** requires an active scene and a point on the same timeline at or after
the shared cursor. It updates the world and every active scene cursor. An
optional location change moves only the characters present at the requested
point. Those characters require an explicit `event`; an empty scene can change
location without one. An event supplied without `locationId` or without
present characters is refused. This operation never adds participants.

**Close** requires an active scene and a point at or after the shared cursor on
the same timeline. It sets only that scene's status, current time and end. The
world and other scene clocks remain unchanged; omitted participant endpoints
inherit the scene end while explicit `until` values remain intact. Closing
creates no event.

**Move with a scene** requires the destination to match that active scene's
location. It ends eligible presence in other active scenes, transfers listed
carried objects, synchronizes all active cursors, and appends absent participant
and object IDs to the destination. A previously ended participant cannot reenter
the same scene through this control; author a distinct scene or explicit source
correction instead.

**Move without a scene** creates only the explicit event. It changes no scene
membership, inventory list or shared cursor. The complete candidate must still
pass the placement checks below.

Attendance endpoints are inclusive. A handoff at `tick:order` ends old presence
at the **same tick and order minus one**. Creation and moves into a scene refuse
the minimum i32 order; they never borrow from the preceding tick or silently
change the supplied time. Closed, planned and abandoned scenes are not edited by
the active-scene controls.

## Carried objects and explicit state reconciliation

Transfer reads state immediately before the movement beat. Only objects already
listed in an active source scene are eligible. An object held by a selected
character present there transfers, followed by listed objects recursively
contained in a transferred object. Unlisted inventory is untouched. The planner
changes scene membership lists, preserving object ownership and all object source.

Relocating an entire scene checks the same pre-movement state, including any
intervening authored drops or handoffs. A listed object at the old location blocks
relocation unless it is carried by a present moving character, directly or through
a container. This accessibility traversal can resolve an unlisted container and
guards against cycles. An explicitly supplied location runs this check even when
it equals the existing location. Author the necessary object-state corrections
before requesting the move.
For an advance at the minimum i32 order, the state check uses that coordinate
itself, matching the pinned clamp; it never invents a preceding tick. Handoffs
retain the separate minimum-order refusal above.
Container references follow their declared entity-kind constraint; the planner
does not add a kind restriction to an unconstrained state declaration.

After ordinary complete-candidate validation, these controls also reconcile all
active scenes against replayed state at their cursors. Present characters must
have matching locations; listed objects must be there, held by a present
character, or recursively accessible through a container. An object cannot be
listed in two active scenes. Canonical events cannot place the same participant
at different locations at the same exact coordinate. Existing exclusive state
write rules also apply. Explicit events supply these facts; the planner does not
repair conflicting events or assume a journey from prose.

Placement evidence uses ordinary unbound scalar entity state in `location`,
`holder` and `container`. Missing/cleared state and literal strings prove no
placement; a bound profile is not interpreted as an ordinary reference. Existing
active scenes that lack placement evidence may need explicit state corrections
before using these controls. These additional checks apply to coordinated
candidates; other native reads and the global validator retain their existing
scope.

## Complete results and source preservation

Every generated update is coalesced per record before construction against the
captured base. The existing maximum of 64 Plan operations and complete encoded
Plan/preview bounds apply; affected scenes, characters or inventory are never
silently omitted. The complete output budget includes both the exported Plan and
its consequence preview. Invalid coordinated candidates are refused; generic
`consequences preview` remains available for reviewing arbitrary invalid Plans.
Native preview budgets are 256–1,000,000 Unicode characters; service, CLI and HTTP
budgets are 256–100,000. The browser and HTTP also retain their complete URL-size
bound; download the request and use the CLI when a URL cannot hold it.

Unsubmitted body bytes and unrelated fields remain intact. A touched mapping or
array retains authored order and untouched values; null optional scene fields
are omitted only when resubmission requires the existing typed codec's absence
spelling. Changed frontmatter is reserialized by that codec. Exact no-ops retain
original source bytes. Previews report `writesPerformed:false` and
`canApply:false`; an export grants no application approval.

This scope follows the four implemented actions and helpers in pinned Python
`authoring.py` and the continuity checks in `validation.py` at
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`, with exact native
IDs/times and the current `until` attendance spelling. Upstream's optional-time
order allocator and temporary record IDs are not part of this native API.
