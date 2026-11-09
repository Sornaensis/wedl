# Coordinated conversation controls

These controls produce a complete ordinary [consequence Plan](CONSEQUENCES.md)
from one exact source fingerprint. Preview is read-only. Publication uses the
existing saved [Actions](ACTIONS.md), outcome review and explicit approval path.

## Interfaces and request

Use `conversation-control preview --file FILE [--max-characters N]`; `--file -`
reads stdin. The equivalent route is
`GET /api/conversation-control/preview?request=JSON&maxCharacters=N`.
Native callers use `conversation_control::decode`, `plan` and `preview`.
The Writing browser supports request import/export, complete preview, exact Plan
download, inspection in Consequences and copying into an editable Actions draft.
Copying performs no save, proposal append, approval or canonical write.

```json
{
  "expectedFingerprint": "<exact lowercase source fingerprint>",
  "at": {"timeline": "main", "tick": "9007199254740993", "order": "8"},
  "action": {
    "type": "conversation.create",
    "conversationId": "conv_00000000000000000000000001",
    "title": "Harbor conversation",
    "sceneId": "scene_00000000000000000000000001",
    "characterIds": ["char_00000000000000000000000001"]
  }
}
```

Replace the fingerprint placeholder with the captured value. IDs, scene, cast
and time are explicit. Coordinates are canonical signed decimal strings within
i64/i32 on a declared timeline. Requests are closed: missing, unknown and
duplicate fields fail. Optional fields use omission; explicit null fails.

| Action | Required fields in addition to `type` | Optional fields |
| --- | --- | --- |
| `conversation.create` | `conversationId`, `title`, `sceneId`, distinct nonempty `characterIds` | None |
| `conversation.append`, `kind: "speech"` | `conversationId`, local `turnId`, `kind`, `text`, `speakerId` | Exact `addresseeId`, boolean `interruptLast` |
| `conversation.append`, `kind: "action"` | `conversationId`, local `turnId`, `kind`, `text`, distinct nonempty `actorIds` | None |

A local beat ID is a nonblank string unique in this conversation; it is not a
record ID. Beat text must be nonblank and is otherwise preserved exactly.
The browser retains imported title, beat ID, text and timeline literals until
that field is edited, including line endings. An action beat rejects speech-only
fields, including `interruptLast: false`. Omitted `interruptLast` means false;
an explicitly supplied false is retained.

## Creation

Creation requires a new conversation ID, an active scene and selected characters
present at the exact requested point. Presence uses inclusive authored scene and
participant endpoints, so an open scene can admit a prospective point beyond
its old cursor. Creation cannot precede authored world `current_time`; when
that field is absent, the selected scene's `time.current` supplies the floor.

The Plan creates an active conversation at the scene's location, starting at
`at`, with explicit participant `from` values and empty turns/recollections.
The same Plan appends its ID to the scene's conversation collection. Existing
links, scene fields and bodies are retained. The two changes validate together.

## Appending

Append requires an active conversation, a new local beat ID and a point within
its interval. Existing source turn times and the new time must be strictly
increasing on one timeline. Speakers, actors and any addressee must be present
conversation participants at the new point.

Speech and action beats receive the explicit `participants` audience. When
`interruptLast` is true, the planner points the new speech's `interrupts` field
at the last earlier speech, skipping action beats. Omitted kind on an existing
beat means speech. Absence of an earlier speech is an error.

The one-record update replaces only `turns`, retaining earlier rows,
recollections, other fields and Markdown body. The existing typed codec
reserializes the changed record's YAML header, so comments/formatting can
normalize; retained structured values and body bytes remain exact. Unchanged
records retain their original bytes. Native interruption validation
requires an exact earlier local speech at a strictly earlier time. Author
transcripts retain the reference; character transcripts include it only when
both beats are admitted. An interruption supplies no knowledge or memory.

Both controls leave world and scene clocks unchanged because their request
already supplies the complete explicit time. Use the separate
[shared-time control](CURRENT_TIME.md) when a clock change is intended.
Creation and appending infer no events, effects, relationships or learning.
Ordinary typed conversation editing remains available for explicit corrections;
it does not acquire these coordinated-control rules automatically.

## Complete bounds and review

Requests are at most 16 KiB and titles at most 1,024 UTF-8 bytes. Native previews
accept 256–1,000,000 characters; CLI, HTTP and browser use 256–100,000.
The existing Plan, source/diff and complete 8 MiB result bounds also apply.
Overflow refuses the entire preview; no truncated Plan is marked complete.

Results report `operation: "conversation-control-preview"`, the exact request,
Plan and complete candidate preview, with `writesPerformed: false` and
`canApply: false` even for a valid Plan because preview cannot publish.
An invalid candidate fails with its validation diagnostics instead of returning
an ordinary wrapped preview; `validationComplete` remains false. Exact source pins and the existing saved
review/approval checks remain required for publication.

The browser rejects malformed UTF-8 imports, retains the original source pin
until explicit capture, and invalidates previews on input, navigation or source
refresh. Late responses cannot replace newer input. Plan transfer into Actions
requires a current editable draft with no unsaved proposal input.
