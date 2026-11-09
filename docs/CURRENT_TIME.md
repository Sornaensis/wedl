# Explicit shared current time

An author can preview an exact change to the shared story cursor. One native
[consequence plan](CONSEQUENCES.md) updates `world.current_time` and the
`time.current` of every active scene. The request supplies the complete point
and the exact source fingerprint. No event, observation, knowledge, movement,
scene status or conversation cursor is inferred or changed.

The source schema remains `wedl/v1`; the service contract remains
`wedl-rust/v1`. The planner and preview are pure reads of one captured World.
They do not read live files, refresh the fingerprint, approve an outcome or
publish source.

For coordinated lifecycle, attendance and movement, use the separate
[scene controls](SCENE_CONTROL.md), which preserve the distinction between a
shared-clock update, closing one scene and creating an independent movement event.

## Request and transport

```json
{
  "expectedFingerprint": "<exact lowercase 64-character source fingerprint>",
  "at": {"timeline": "main", "tick": "12", "order": "0"}
}
```

The JSON object has exactly `expectedFingerprint` and `at`. The `at` object has
exactly `timeline`, `tick` and `order`. All are required strings. Duplicate
keys, unknown fields, nulls and numeric JSON coordinates fail. Tick and order
use canonical signed decimal strings within i64 and i32 respectively:
`"9007199254740993"` is exact; `"01"`, `"+1"`, `"-0"` and `"1.0"` fail.
Timeline is an exact declared ID, nonblank and at most 128 UTF-8 bytes. The
request is at most 16 KiB. There are no time defaults or automatic next-order
allocation.

```sh
wedl-rs --repo my-world current-time preview --file current-time.json --max-characters 100000
```

`--file -` reads standard input. The matching closed GET route is
`/api/current-time/preview`, with required URL-encoded `request` JSON and optional
`maxCharacters`. CLI and service requests permit 256–100,000 characters,
defaulting to 100,000. The native Rust function permits 256–1,000,000.

The local browser provides the same preview and exports the exact Plan for the
existing Consequences workflow. Every returned preview is read-only. Applying
a plan uses the separate reviewed [outcome workflow](OUTCOMES.md), including its
own snapshot, approval and publication checks. No current-time apply endpoint
or independent source writer is introduced.

## Complete plan and candidate validation

The base snapshot must be valid and its fingerprint must exactly match the
request. A stale request is refused. An undeclared timeline is refused before
planning.

The plan contains the world operation first, then every active scene in stable
exact-ID order. Planned, closed and abandoned scenes, conversations and all
other records stay unchanged. Each changed scene keeps its existing start,
end, location, participants, object list, observations, body and unrelated
metadata. A supplied world clock uses numeric source scalars:

```yaml
kind: world
current_time: {timeline: main, tick: 12, order: 0}
```

All operations are evaluated as one complete candidate through the ordinary
consequence preview. A direct world-only clock update can fail if active scenes
would no longer match it. A generated plan changes those cursors together;
it does not make intermediate mismatched source publishable.

The explicit operation may move backward or select another declared timeline.
The candidate still has to satisfy all existing source invariants. For example,
a requested scene cursor before its start, after its end, or on a timeline that
does not match its endpoints produces a complete invalid preview. The planner
does not rewrite intervals to make the request pass. With no active scenes, a
world clock can change timelines without changing unrelated historical records.

An already equal effective point becomes a kind-only no-op operation, preserving
the exact existing source bytes, including comments, CRLF, omitted coordinate
defaults and an explicit null scene end. A missing world cursor is materialized
even when its derived effective time already equals the request. When a scene
cursor changes and its source end is null, the generated time mapping omits
that absent endpoint, as required by the existing typed scene-update grammar.

## Result and finite bounds

The result contains:

| Field | Meaning |
| --- | --- |
| `operation` | `current-time-preview` |
| `scope` | `local-author-current-time` |
| `request` | The exact fingerprint and requested point |
| `previousTime` | The captured snapshot's effective cursor |
| `activeSceneIds` | Every active scene, in plan order |
| `plan` | The complete exportable native consequence Plan |
| `preview` | The unchanged complete consequence preview, including candidate validation, exact source and diffs |
| `maxCharacters` | The requested bound on the entire returned JSON |
| `complete` | `true`, because no operations, files or diagnostics are truncated |
| `validationComplete` | `false`; this focused preview does not declare all parity work complete |
| `writesPerformed`, `canApply` | Always `false` |

The nested `preview.validation.valid` reports the candidate result. Neither it
nor the candidate fingerprint grants application authority. Historical
snapshots retain their original revision and fingerprint; the preview never
substitutes current working-tree source.

The existing Plan limit is 64 operations: one world and at most 63 active
scenes. More active scenes fail the whole request. Plans retain the existing
256 KiB bound. The complete wrapper, including the Plan, source, diffs and all
metadata, must fit both the caller's Unicode-scalar character limit and the
8 MiB serialized JSON limit. Overflow returns an error; no partial preview or
truncated Plan is returned. The nested consequence preview also retains its
existing complete source/diff bounds.

## Typed world updates

The existing `record update` grammar now accepts world `current_time` as a
closed mapping containing exactly `timeline`, `tick` and `order`. Source tick
and order must be integers within i64 and i32; source strings, booleans, nulls,
missing coordinates and extra fields fail. The normal writer validates the
complete candidate and binds publication to the reviewed source fingerprint
and hash. Other world fields and opaque body text retain their existing rules.

## Pinned behavior and remaining scope

The reference is
[`Sornaensis/wedl` at `f0e885ccebc279eff65a5db2984d01ebc6647b2b`](https://github.com/Sornaensis/wedl/tree/f0e885ccebc279eff65a5db2984d01ebc6647b2b).
In its
[`src/wedl/authoring.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/authoring.py),
`current-time.set` calls `_active_cursor_operations`, which updates the world
and every active scene together. `_require_not_before_world_cursor` explicitly
exempts an explicit current-time set from its advance-only rule. The retrieved
file was verified against the pinned tree's Git blob
`9a061c38f98011fd5320ebfaacec3642f3a826b1`.

This slice implements that explicit shared-clock behavior through native v1
Plans. Higher-level scene creation/advance/close actions, movement and object
reconciliation, hypotheses and broader world-schema editing remain separate
work. Generic typed scene edits retain their existing contract; this operation
does not add a second lifecycle engine.
