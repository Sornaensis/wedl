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
`union`, `organization`, `legacy`, `vital`, `search`, `character-unions`, and
`organization-legacies`. Traversal is
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
substring matching. Each authorized lane advances an indexed keyset stream
only until the bounded page is complete; a large authorized set remains
pageable. Repeated
transitions on one record consume one slot. Withheld vital histories have no
postings, and vital initialization alone never produces a result before birth
or existence start.
Matches paginate with an `(applicability, source ordinal, record ID)` keyset.
Cursors bind the normalized request, trusted scope, and revision. The `items`
bound also applies to union participants and explicit all-time history arrays;
the roster bound counts active affiliations in as-of mode. For an organization
author-as-of request with an explicit exact `at`, `includeFormerRoles: true`
adds a separate `formerRoles` array. Each entry is an ended authored
affiliation at that horizon with its last authored `value.role` (including
`null`), admitted citations, and causes. `roles` remains the active roster.
Both arrays share one `items` cap; exceeding it closes the entire response as
`GEN-LIMIT-001`. The option rejects `false`, other modes and operations, an
implicit time cursor, and a request `cursor`. Without the option, the original
active-only response is unchanged.

`character-unions` and `organization-legacies` require explicit author-as-of
`at` and return `unions` or `legacies` as cited, folded records. The former
uses the literal union-participant reverse posting only to find candidates;
current membership and a declared or formed union state are checked at the
exact signed `(tick, order)` before the shared `items` bound or any result is
formed. Reconciled former participants and ended or annulled unions are absent.
The latter follows only explicit `legacy.organization_id` links and returns
visible applicable records with their authored state, including dormant and
dissolved. It does not derive succession or an office holder from the link.
An overflow closes the whole response as `GEN-LIMIT-001` without a partial
list. For `character-unions`, each current union's participant list also
shares the `items` bound. A selector with no admitted evidence returns
`unknown` across direct, CLI, and HTTP reads.

## CLI and HTTP transport

Use `wedl generational parents request.json` or send the same raw JSON body to
`POST /api/generational/parents` with `X-Wedl-Token`. The other leaf actions are
`ancestors`, `descendants`, `relatives`, `union`, `organization`, `legacy`,
`vital`, `search`, `context`, `character-unions`, and
`organization-legacies`; each has a matching route. The body must
include `protocol: wedl-generational/v1`, the operation, an exact 40-character
Git revision, the canonical capability array from that world, `mode`, and a
declared `timeline`. Author as-of may supply `at` with exact signed decimal
string `tick` and `order`, or use the current world/scene cursor. Explicit
author all-time forbids `at`. `character-unions` and `organization-legacies`
require an explicit author-as-of `at`; they do not accept all-time or character
mode. Other character-mode reads require `at` and currently
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
Character selectors now use the same closed admission rule. A standalone
character without an admitted generational fact returns `unknown`, including
when the raw world contains its timeless title. Missing, future, and hidden
character references share this outcome without suggestions.
`available` and `unknown` use HTTP 200 and CLI exit 0; `invalid`,
`unavailable`, and `limit` use HTTP 400, 409, and 422 and CLI exit 2. Nested
StoryTime coordinates and citations retain decimal-string ticks and orders.

### Bounded discovery for the read-only interface

`GET /api/generational/bootstrap` with the session token returns the current
compiled revision, canonical capability list, and declared timeline IDs.
An optional `revision` selects an exact Git commit; `requireCompiled=true`
requires its cache to be ready. It returns no entity names or counts.

Use `wedl generational discover request.json` or
`POST /api/generational/discover` to find admitted titles and aliases. The raw
request uses the common protocol/revision/capabilities, `mode: author-as-of`,
declared `timeline`, and an explicit signed-string `at`, plus `kind` (character,
organization, legacy, or event), a nonblank title/alias prefix `text` (at most
64 characters), optional `items` (1–100), and optional `cursor`. Results have
`id`, `kind`, current `title`, and `matchedName`; a title and alias may produce
separate matches. Pagination orders by normalized matched name and ID. The
cursor binds the normalized filter, horizon, revision, capabilities, and
trusted viewer lanes. An inapplicable or malformed cursor returns `invalid`.
No candidate count is returned. The indexed time-prefix seeks admit exact
signed `(tick, order)` instants before scanning names, including for an empty
page. Response titles and matched names are bounded to 256 UTF-8 bytes;
an entity with a current overlong title has no discovery or label result at
that horizon, and an overlong alias cannot match. An earlier bounded title
remains available at its earlier horizon.

Use `wedl generational labels request.json` or
`POST /api/generational/labels` with the same envelope and 1–100 unique `ids`
to retrieve current display titles for already linked IDs. The `labels` array
omits absent, future, and hidden IDs. Both operations read indexed private
postings on a ready cache and never fall back to `/api/entities` or source
prose. A generated test fixture with at least 5,000 characters and 10,000
parentage edges checks the name index, 20-result response size, and SQLite
virtual-machine work for an indexed page.

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
