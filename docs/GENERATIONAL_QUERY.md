# Generational query and context

`wedl.generational_query` provides API-neutral reads over the compiled v0.7
evidence. A transport adapter authenticates the principal and constructs a
`TrustedViewerScope` with the pinned revision, mode, timeline, exact StoryTime,
server-derived audiences and perspectives, negotiated capabilities, and, for
character mode, the authenticated character ID. None of these values may come
from a raw query selector. `wedl.generational_api` owns local author binding,
name resolution, wire validation, and CLI/HTTP status mapping.

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

## CLI and HTTP transport

Use `wedl generational parents request.json` or send the same raw JSON body to
`POST /api/generational/parents` with `X-Wedl-Token`. The other leaf actions are
`ancestors`, `descendants`, `relatives`, `union`, `organization`, `legacy`,
`vital`, `search`, and `context`; each has a matching route. The body must
include `protocol: wedl-generational/v1`, the operation, an exact 40-character
Git revision, the canonical capability array from that world, `mode`, and a
declared `timeline`. Author as-of may supply `at` with exact signed decimal
string `tick` and `order`, or use the current world/scene cursor. Explicit
author all-time forbids `at`. Character mode requires `at` and currently
returns closed `unknown`: the repository session token is not a character
identity, and the current evidence grammar cannot prove a positive structural
grant. The request cannot supply a viewer, audience, or perspective.

Use `subject` and, for `relatives`, `target` as a canonical ID, title, or
alias; the server resolves each at the selected revision. Search uses a
normalized single token `text` and an optional revision/scope-bound `cursor`.
`items` is 1–500 and `depth` is 0–32; context additionally requires
`maxCharacters` (80–65536), with `items` at most 100 and `depth` at most 16.
Unknown or ambiguous author names yield bounded suggestions or candidates.
For union, organization, and legacy selectors, name matching and suggestions
include only records visible at the selected horizon and trusted scope. A
future or withheld record has the same name-resolution result as an absent one.
`available` and `unknown` use HTTP 200 and CLI exit 0; `invalid`,
`unavailable`, and `limit` use HTTP 400, 409, and 422 and CLI exit 2. Nested
StoryTime coordinates and citations retain decimal-string ticks and orders.

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
The transport enforces `maxCharacters` again after adding the protocol,
operation, revision, and decimal-string conversion; it removes trailing items
with `truncated: true` before returning an oversized context response.
