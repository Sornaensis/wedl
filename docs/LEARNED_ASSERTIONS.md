# Explicitly learned assertion claims

A character's learned words are separate from the current authored assertion.
Learning does not copy the assertion's value, title, applicability, classification,
correction, evidence or body. A character may remember an expired proposition,
reject a previously learned claim, or retain words that disagree with current
canon.

## Source

Declare `lore-core-v1` and `assertions-v1`. An ordinary `knowledge` record may
use this closed claim instead of an ordinary or generational claim:

```yaml
kind: knowledge
id: know_00000000000000000000000001
title: An author label for this learning history
knower: char_00000000000000000000000001
claim:
  key: archive-rule
  statement: The archive told me the door was blue.
  assertion_id: assertion_00000000000000000000000001
transitions:
  - id: learned-the-rule
    time: {timeline: main, tick: 5, order: 0}
    state: suspected
    confidence: 0.4
    acquisition: heard
```

The claim contains **exactly** `key`, `statement` and `assertion_id`. The first
two are nonblank authored strings. The third is an exact current assertion ID,
not its subject or title. Ordinary `subject`, `predicate`, `object`, `truth` and
`genealogy` fields cannot be mixed into this claim. The existing finite value
guard applies. All retained knowledge statuses validate their dependencies.

Knowledge retains its ordinary transition grammar: unique member IDs, strict
native time ordering on one declared timeline, known states/acquisition tokens,
finite confidence from 0 to 1, and validated optional source/causing-event
references. No transition or affirmative acquisition is required merely to save
a valid record; source without admitted learning returns no character claim.

Optional root `audience` and `perspectives` arrays constrain knowledge admission.
They are unique nonblank string arrays without an ordering requirement. Omission
adds no constraint; an empty array admits nothing. Character audiences are
`public` or the exact character ID, and perspectives are `ordinary` or that ID.
Caller-selected labels cannot enlarge these sets.

## Acquisition and held state

At a required exact horizon, select canonical knowledge belonging to the exact
character and admitted by its labels. Read the latest transition at or before
that horizon on the same timeline. At least one earlier or equal affirmative
transition must exist: `accepted`, `suspected`, `uncertain` or `remembered`.

| Current/history condition | Character result |
| --- | --- |
| No transition yet | No claim |
| Rejection without earlier affirmative learning | No claim |
| An affirmative state at the horizon | Learned claim |
| Rejection after affirmative learning | Learned claim, explicitly rejected |
| Latest state is forgotten | No claim |
| An explicit later held state after forgetting | Learned claim if an earlier affirmative transition exists |

Forgetting controls the latest held state; it does not delete source or impose a
new history-reset rule. Canonical assertion validity, retirement, hidden labels
or replacements never change this admission. A correction to a character's
words must itself be authored through knowledge source; canonical correction
does not rewrite the belief or infer a second learned claim.

## Character reads

```sh
wedl-rs --repo my-world character assertions char_00000000000000000000000001 assertion_00000000000000000000000001 --tick 6 --order 0
```

The native operation is
`learned_assertions::list(world, viewer, target, at, limit)`. The closed GET route
is `/api/character/assertions`, using `character`, `target`, required `tick` and
optional `order`, `timeline` and `limit`. The ordinary author assertion operation
stays separate.

The target matches an exact assertion ID from the learned claim **or** the
knowledge record ID. It never resolves canonical subjects, titles or aliases.
Unavailable, unlearned, other-character and forgotten targets return empty
items without canonical labels. Invalid character selectors share the existing
`character view is unavailable` error.

Each item contains `knowledgeId`, `transitionId`, `assertionId`, the literal
`claim`, its `statement`, current `state`, `confidence`, latest transition `time`,
a transition `target` and an empty `citations` array. The target is
`{kind: transition, ownerId: KNOWLEDGE_ID, id: TRANSITION_ID}`. Native tick/order
values use exact decimal strings on the wire. No path, revision, fingerprint,
blob ID, canonical content, acquisition token, author note or source-entity
metadata enters this projection. The transition identity and time describe the
actual learned history without granting access to author source.

Results sort by latest held time, then knowledge/transition ID. Independent
claims remain separate even when their keys or assertion links match. Admission
and target filtering precede the complete result limit of 1–1000; overflow
returns an error, never a truncated list. The envelope is a character response
with `basis: explicitly-learned-assertions` and no global source metadata.

Native public lists validate the full captured source and fingerprint before
projection, returning a generic character-evidence error for invalid source.
The shared internal projection supplies the same safe items to generic character
context/search and other narrative consumers. Search indexes learned words, not
canonical assertion wording; hidden/future edits cannot change admitted counts,
ranking or packet-budget behavior on otherwise valid snapshots.

## Editing and boundaries

The existing reviewed `record create` and `record update` writer accepts linked
claims and knowledge labels. Submitted claims and transition arrays replace the
whole field; omitted fields, body and unrelated metadata are preserved. Preview
validates the complete candidate and apply uses the existing source fingerprint
and candidate hash. No separate writer or automatic belief replacement is added.

Author `knowledge` reads still expose their author source details and can include
forgotten records when explicitly requested. Character reads do not reuse that
author payload. Learning an assertion ID does not grant access to its canonical
record, current subject, descriptions or related concepts. The project remains
on its current v1 labels.

## Evidence and verification

The implementation follows the pinned
[`assertions.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/assertions.py)
linked-claim grammar and
[`assertion_runtime.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/assertion_runtime.py)
affirmative-learning/current-held-state rules. The pinned assertion source/runtime
tests cover late learning, rejection, forgetting and canonical-edit independence.
Rust retains its existing exact native time source, complete-or-error limits and
safe character envelope instead of cursor/revision mechanics. Focused tests in
`tests/learned_assertions.rs` cover these semantics, source validation, exact
selectors, scope, limits, author/character separation and full response invariance.
