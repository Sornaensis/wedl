# Generational query and context

`wedl.generational_query` provides API-neutral reads over the compiled v0.7
evidence. A transport adapter authenticates the principal and constructs a
`TrustedViewerScope` with the pinned revision, mode, timeline, exact StoryTime,
server-derived audiences and perspectives, negotiated capabilities, and, for
character mode, the authenticated character ID. None of these values may come
from a raw query selector. The downstream CLI/HTTP task owns authentication,
principal binding, input parsing, and status mapping.

Use `TrustedViewerScope.from_trusted_adapter` for canonical signed decimal
`tick` and `order` strings. `query_generational(repository, scope, request)`
checks the selected source revision against the compiled database, rebuilding
the disposable cache unless `require_compiled=True`. `author-as-of` uses an
explicit StoryTime or the authored world/active-scene cursor; absent both, the
request is invalid. `author-all-time` requires a selected timeline and forbids
`at`. Character mode requires an authenticated character and an exact `at`.
Cross-timeline, malformed, or budget-invalid requests return closed `invalid`;
missing capability returns `unavailable`.

Requests select `operation`, `subject_id`, optional `target_id` or structural
`text` (one normalized word of at most 32 characters), `depth` (0–32),
`items` (1–500), and an opaque scope/revision-bound
cursor. The operations are `parents`, `ancestors`, `descendants`, `relatives`,
`union`, `organization`, `legacy`, `vital`, and `search`. Traversal is
cycle-safe and indexed. If depth or item bounds would cut off a derived path,
the whole result is `limit` (`GEN-LIMIT-001`) with no partial inference.
Missing affirmative evidence is `unknown`; an empty authorized private search
is `available` with no results. Every returned derived edge cites its authored
record path and exact applicability. Claims remain separate from literal
tenure, and a transfer never implies the target holds.

Filtering checks capability, timeline, horizon, record audience/perspective,
applicability, and character evidence before labels, path expansion, ranking,
pagination, or serialization. Search uses the private structural projection
and normalized token-prefix postings indexed by audience, perspective,
timeline, and first applicability. Matching is prefix-based, not arbitrary
substring matching. Each authorized lane reads at most 501 distinct records;
a larger discovery set returns closed `limit` without a partial page. Repeated
transitions on one record consume one slot. Withheld vital histories have no
postings, and vital initialization alone never produces a result before birth
or existence start.
Matches paginate with an `(applicability, source ordinal, record ID)` keyset.
Cursors bind the normalized request, trusted scope, and revision. The `items`
bound also applies to union participants and explicit all-time history arrays;
the roster bound counts active affiliations in as-of mode.

The current knowledge, scene-observation, and conversation-recollection source
grammar has no validated exact reference that says a character knows a given
generational structural fact. Prose, a visible character identity, and
`source_entity` do not establish one. Character generational reads therefore
return `unknown` until an approved source/ADR contract supplies that evidence;
they cannot report even the existence or count of hidden records.

`build_generational_context` calls only authorized query operations. It has
depth (0–16), item (1–100), and final serialized-character budgets. It measures
the complete JSON packet, reports truncation, and does not summarize hidden
evidence. A traversal `limit` in any subquery closes the whole packet. A packet
too small for the closed envelope also returns `limit`.
