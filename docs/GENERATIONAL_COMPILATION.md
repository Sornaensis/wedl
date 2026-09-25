# Generational compilation and replay

The v0.7 source grammar and validation are defined by ADR 0005 and
`GENERATIONAL_SOURCE_CONTRACT.md`. Markdown in Git remains canonical. The
compiler creates a disposable SQLite evidence model from validated source;
deleting the cache does not alter authored history.

## Stored evidence

`generational_record` carries kind, source ordinal/path, status, world-only
capability, timeline, audience, and perspectives. Typed tables retain organization containment,
directed parentage, n-ary union participants per literal transition,
affiliations and roles, legacies,
separate tenures and claims, and optional vital history. No ancestry closure,
successor, holder from a claim, or spatial topology is inferred or stored.

`generational_transition` stores every literal initialization and transition,
including its ID, kind, exact signed tick/order, inclusive interval end when
present, source ordinal, payload, cause, replacement target, and authored
citation. Rows sort by `(timeline, tick, order, source ordinal, transition ID)`.
The citation identifies the source path, blob, revision, and exact transition
section. Cause events retain their own source citation. A winning replacement
retains citations to its superseded chain.

`generational_current` records a deterministic fold at the selected world
cursor. `generational_candidate` retains private per-transition structural
fields with their timeline, applicability bounds, audience, perspectives, and
citation. Record-level labels are omitted there because they can summarize a
later transition.
Generic metadata, body, titles, aliases, tags, and transitions from these eight
kinds do not enter `search_document`, FTS5, or vector training/retrieval.

`generational_discovery_name` is a private, disposable title/alias posting
table for the read-only generational interface. A canonical authored fact
admits a linked character or cause event only at its first applicable
StoryTime, under that fact's audience and perspective. Organization and legacy
names come from their literal initialization or rename payloads, not from a
later source summary title. The small `generational_discovery_lens` table lets
the local author adapter derive its trusted lanes without scanning source
records. `generational_discovery_time` ranks exact `(tick, order)` instants
separately in each audience/perspective lane;
`generational_discovery_segment` stores private time-prefix postings for
character/event names and horizon interval postings for organization/legacy
names, so each name range is sought only among rows eligible at the requested
horizon. Interval lookup has a fixed depth independent of later source instants.
An overlong literal title closes that organization or legacy while
it is current, without suppressing its earlier bounded title. Static titles
longer than 256 UTF-8 bytes close their entities; overlong aliases are omitted.
Ready-cache discovery reads use these compiled rows without building
a whole-world `World`. If the schema or named index shape is stale, the cache
is rejected and rebuilt from the unchanged Git source.

## Internal replay

`fold_record` takes an explicit StoryTime and an already visibility-filtered set
of candidate record IDs. It applies only authored transitions on that timeline
up to the exact `(tick, order)` boundary. The inclusive vacancy operation
returns `vacant-interval` through its last instant and a nonpersisted `expired`
state after it. A transfer vacates its source tenure; it never creates an
implicit hold on its target. Legal and de-facto literal holds remain distinct,
and claims never count as tenure.

`cited_ancestors` and `cited_containment` use indexed direct edges and bounded,
cycle-safe breadth-first traversal. Ancestry sorts each whole depth by authored
applicability StoryTime, source ordinal, and stable edge ID. Distinct authored
edges to one parent retain separate citations while that parent expands once.
Callers must provide depth and item bounds;
if either is exhausted, the result is a closed `limit` outcome with no partial
path. A detected containment cycle has the same closed outcome. Returned edges
cite authored records. These primitives do not select a
viewer, authorize source, implement complete author-as-of or character
visibility, or expose public query responses. That policy belongs to the
downstream generational query layer, which must filter before traversal or
ranking.

## Cache compatibility

The disposable schema identifier and generational compiler generation token
change when this projection changes. Both direct and in-memory authoring-byte
compilation use the same projection insertion path. A schema/token mismatch,
missing or malformed required table or index, failed SQLite integrity check, or failed
foreign-key check rejects the cache; a complete validated candidate replaces
it atomically. Full and fast-forward rebuild modes create the same generational
rows from the same source.
