# Generational authoring

The `generational` authoring intents compile to ordinary
`wedl-changeset/v1` `entity.create` and `entity.update` operations. They use
the v0.7 source grammar and the existing preview, confirmation, atomic Git
transaction, idempotency receipt, and compiled-cache deferral. No relative,
organization member, successor holder, claim decision, birth, or event is
inferred. Start with `wedl generational scaffold --output intent.json` and
inspect `wedl generational schema` or `GET /api/generational/schema`.
The starter requires a v0.7 world declaring `generational-core-v1`; a
spatial-only v0.7 world returns an upgrade-required error before offering it.

An intent always contains the exact current `expectedHead` and a unique
`idempotencyKey`. Preview the same JSON through `wedl author request preview
intent.json` or `POST /api/authoring/preview`. If its `preview.valid` is true,
apply **the identical intent** with `wedl author request apply intent.json
--confirm TOKEN` or `POST /api/authoring/apply` with
`X-Wedl-Confirmation: TOKEN`. `--yes` cannot bypass this confirmation for
generational intents. A retry with the same key and intent returns the stored
receipt; changing the intent under the same key is a conflict. A changed HEAD
or confirmation token cannot write any source file.

The starter is a complete explicit organization creation. A parentage
example, using names resolved against that exact HEAD, is:

```json
{
  "action": "generational.create",
  "expectedHead": "0123456789abcdef0123456789abcdef01234567",
  "idempotencyKey": "adoptive-parent-1",
  "kind": "parentage",
  "title": "Mara adopts Ilyra",
  "tags": ["adoptive"],
  "audience": ["public"],
  "perspectives": ["ordinary"],
  "fields": {"child_id": "Ilyra Sorn", "parent_id": "Mara Vale"},
  "payload": {"basis": "adoptive"},
  "at": {"timeline": "main", "tick": "-7", "order": "2"}
}
```

Create is a closed variant for each of the eight kinds. `fields` has the
following source-shaped typed references or literal attributes:

| Kind | Required `fields` | Optional `fields` | Initialization `payload` |
| --- | --- | --- | --- |
| organization | `organization_kind` | `parent_id`, `location_id` | `title`, sorted `aliases` |
| parentage | `child_id`, `parent_id` | — | `basis`: biological or adoptive |
| union | sorted `participant_ids` (at least two) | — | sorted `participant_ids` |
| affiliation | `character_id`, `organization_id` | — | `role`: text or null |
| legacy | `legacy_kind` | `organization_id` | `title`, sorted `aliases` |
| tenure | `legacy_id` | `predecessor_tenure_id`, `successor_tenure_id` | `holder_id` (character or null), `basis` (legal or de-facto) |
| claim | `legacy_id`, `claimant_id` | — | sorted `competes_with` claim references |
| vital-history | `character_id`, `disclosure` | — | empty object |

Each typed reference accepts a canonical stable ID, title, or alias. The
server resolves it at `expectedHead` and writes the stable ID into canonical
snake_case Markdown. Optional `id` is a stable ID of the kind; if omitted, a
deterministic stable ID is derived from the exact intent. Initialization and
later transitions receive deterministic `transition_` IDs. The source
`domain` defaults to `history.*`; `tags`, `aliases`, and `threads` default to
empty arrays. `audience` and `perspectives` are required, sorted, nonempty
arrays. No browser editor or collaboration permission is added.

Later lifecycle changes use `generational.append` with `kind`, `record`
(name or ID), a closed `transition` such as `union-form`, its exact `payload`,
`at`, and `cause` (an earlier canonical event name or ID on the same
timeline). `tenure-vacate` uses `interval: {first, last}` instead of `at`;
its endpoints are inclusive and `payload` is `{"holder_id": null}`.
`generational.correct` has the same fields and requires `replaces`, the
existing transition ID at the exact same applicability. The validator
enforces the kind's state table, chronology, reciprocal claims, tenure target
state, and other source invariants. A semantic non-initial transition always
requires its explicit earlier event cause, even where the raw v0.7 grammar
allows an omitted cause.

Use `generational.batch` with 1–32 explicit `items`, each a create, append,
or correct item without its own `expectedHead` or `idempotencyKey`. It makes
one preview and one commit. For reciprocal claims or other new records that
reference each other in one batch, supply their stable `id` values explicitly
and use those IDs in typed reference fields. Adoption can be multiple
explicit parentage creates, and a transfer can append a source transfer and
an independently chosen target hold; the writer never synthesizes the target
hold. Source validation runs over the full candidate before any commit.

All transport StoryTime coordinates use exact signed decimal-string `tick`
and `order`, including negative ticks and same-tick order. The compiled
source stores them as integers. Invalid JSON fields, numeric wire times,
unsupported transition variants, ambiguous or wrong-kind names, absent or
late event causes, and stale heads fail before mutation. A preview may also
return `valid: false` with specific source diagnostics for a complete but
invalid candidate.
