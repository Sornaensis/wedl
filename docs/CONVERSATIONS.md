# Conversation provenance

A wedl conversation has two deliberately separate layers: immutable historical
beats and subjective memory. Speech is verbatim provenance; action is canonical
choreography, not a quote or durable state mutation.

## Canonical transcript

```yaml
kind: conversation
status: active
time:
  start: {timeline: main, tick: 176, order: 0}
scene: scene_...
location: loc_...
participants:
  - character: char_mara
    role: proposer
    from: {timeline: main, tick: 176, order: 0}
  - character: char_ansel
    role: hydraulic-keeper
    from: {timeline: main, tick: 178, order: 0}
turns:
  - id: turn_...
    # kind may be omitted for this legacy speech form.
    kind: speech
    at: {timeline: main, tick: 178, order: 10}
    speaker: char_ansel
    addressee: char_mara # or `participants`
    text: The shutters are closing. Argue while walking.
    delivery: urgent
    audience: [participants]
  - id: turn_...
    kind: action
    at: {timeline: main, tick: 178, order: 20}
    actors: [char_mara, char_ansel]
    text: Mara takes the map while Ansel holds the closing shutter open.
    audience: [participants]
  - id: turn_...
    kind: speech
    at: {timeline: main, tick: 178, order: 30}
    speaker: char_ansel
    interrupts: turn_...
    text: Move.
    audience: [participants]
```

`kind` defaults to `speech`, so existing records remain valid. Speech beats
permit `speaker`, optional `addressee`, optional `delivery`, and optional
`interrupts`; an interruption must target an earlier speech beat. Action beats
require non-empty `actors`. The two forms are discriminated: action beats may
not carry speaker/addressee/delivery/interruption fields, and speech beats may
not carry `actors`.

Speech text is verbatim provenance. Action text is a canonical description,
displayed as action rather than quoted dialogue. A conversation action never
mutates location, custody, knowledge, relationships, or any other durable
state; record that consequence in a canonical event. A participant may retrieve only beats whose
audience admits them and whose time lies inside their presence interval. In the
active example conversation, Sister Ansel joins at tick 178 and cannot retrieve
the six earlier turns heard by Mara, Ilyra, Nessa, and Ysabet.

## Subjective recollection

```yaml
recollections:
  - id: recol_...
    character: char_nessa
    at: {timeline: main, tick: 178, order: 30}
    state: remembered
    summary: Mara divided temporary custody among three people.
    interpretation: The arrangement accepts distributed custody but remains provisional.
    emotional_impression: relief constrained by urgency
    confidence: 0.93
    exact_turns: [turn_...]
    remembered_quotes: []
```

Recollections are not canonical transcripts. They may:

- omit details;
- emphasize one line;
- preserve exact turn IDs;
- remember approximate wording;
- attach an interpretation or emotional impression;
- become uncertain, distorted, or forgotten;
- be superseded by a later recollection for the same character.

Validation prevents a character from marking an inaudible turn as exact memory.

## Query behavior

- Live-scene context includes only a short recent window of audible speech and
  witnessed action beats. Speech remains quoted; action is visibly labelled and
  never turned into an invented quote.
- Later context prefers current subjective recollection rather than replaying an
  old conversation perfectly.
- A bounded author view can compare only transcript turns and recollections at
  or before its effective time. Its natural default is the conversation end;
  author-only `--all-time` explicitly returns the complete history.
- Search indexes spoken transcript turns and recollections as separate documents
  with distinct audience and fidelity metadata; action beats retain their
  canonical conversation projection without becoming remembered quotations.

## Mutation operations

Conversations can be created and extended through the ordinary atomic changeset
layer:

- `conversation.create`
- `conversation.turn.append`
- `conversation.recollection.record`

Temporary IDs for appended turns and recollections are deterministically mapped
to typed `turn_…` and `recol_…` IDs. The mapping is returned by preview/apply and
is stable under idempotent replay.

See [`../examples/choice-of-records-followup.json`](../examples/choice-of-records-followup.json).
