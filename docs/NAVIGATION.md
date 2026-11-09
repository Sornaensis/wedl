# Author navigation

Navigation reads one immutable source world through the shared library, CLI and
local GET API. Coordinates remain exact signed decimal strings in JSON. Ticks
are ordinal positions, not elapsed time. Every dated read includes the selected
order and excludes other timelines and later evidence.

| Read | CLI | GET route |
| --- | --- | --- |
| Authored timeline | `timeline [--timeline NAME] [--tick T --order O] [--limit N]` | `/api/timeline` |
| Character whereabouts | `whereabouts --tick T [--character ID] [--limit N]` | `/api/whereabouts` |
| Causal graph | `causal EVENT --tick T [--direction upstream\|downstream\|both] [--limit N]` | `/api/causal` |
| All story points | `story-points --tick T [--scene ID] [--limit N]` | `/api/story-points` |
| One story-point trail | `story-point ID --tick T [--scene ID] [--limit N]` | `/api/story-point` |
| Pair interactions | `interactions FIRST SECOND --tick T [--limit N]` | `/api/interactions` |
| Selected memberships | `thread-memberships --record ID --thread ID [--tick T] [--limit N]` | `/api/thread-memberships` |

All dated commands accept `--timeline` and `--order`; omitted order means the
end of the selected tick. Only timeline and memberships have an explicit
all-time mode by omitting the horizon. CLI character, record and thread flags
can repeat. HTTP uses JSON arrays in `characters`, `records` and `threads`;
these are exact IDs, not aliases. The shared decoder sorts selections and
rejects duplicates. See the README route table for other parameter names.

## What the reads establish

- [Whereabouts](WHEREABOUTS.md) derives journeys only from explicit initial
  location state and admitted canonical set/clear effects. Scene presence
  remains separate. Prominence uses the complete admitted canonical/retired
  character cohort before the requested-character filter.
- [Timeline and causality](TIMELINE_NAVIGATION.md) retain all authored event
  statuses in chronology, dated lifecycle points and scene/conversation spans.
  Causal graphs use canonical events and authored cause edges. `both` follows
  the connected component; shared cast or location never creates an edge.
- [Story navigation](STORY_NAVIGATION.md) retains dated lifecycle transitions,
  original causing-event citations, declared outcomes and continuity advisories.
  An explicit scene changes scene-condition evaluation without filtering the
  candidate list. Pair interactions require both named event participants.
  Selected thread membership never returns unselected memberships.

Every referenced label must be independently admitted. Hidden or future
records cannot supply a label, link or evidence contribution merely because
an admitted owner mentions them. Character knowledge and reader access remain
separate; these are local-author reads.

## Bounds and browser behavior

`limit` is 1–1000 and defaults to 1000. It is a complete-result bound, not a
pagination size. Exceeding a bound refuses the read without returning a partial
answer. Each detailed contract specifies which rows, members and internal
evidence counts it bounds. Thread requests select at most 256 records and 32
threads; whereabouts requests select at most 32 characters.

The Navigation tab exposes whereabouts, causal direction, one story-point
trail, pair interactions and selected memberships. Timeline and Story points
retain their own tabs, with explicit all-time timeline and scene selectors.
Browse also offers a selected character's whereabouts and a story-point trail.
These controls issue GET requests only.

Changing selectors or the shared horizon invalidates displayed and pending
navigation results. Tab departure and source refresh invalidate pending reads.
The initial world response fills the complete current horizon only while that
horizon has neither been edited nor used; a delayed response cannot replace
the coordinates behind an already requested result. Source refresh retains
the user's selectors.
