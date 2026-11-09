# Native authoring reference

The static authoring reference describes native consequence Plans and editable
record fields. It is available without opening a world:

```sh
wedl-rs authoring schema
wedl-rs --compact authoring schema
```

The CLI emits the ordinary `wedl-rust/v1` envelope with a `result`, without
`fingerprint` or `revision`. `--revision` is rejected because the reference is
independent of source. The global `--repo` path is not opened. Reading guidance
does not invoke Git, compile, create a cache or change a story.

The author HTTP route is `GET /api/authoring/schema`, with no query parameters.
Immutable and current-worktree author sessions return the same result. A running
session can still supply this static reference when its current source becomes
unavailable or invalid; ordinary source-dependent reads retain their refusals.

## Contents

| Field | Meaning |
| --- | --- |
| `protocol`, `operation`, `sourceSchema` | Native v1 contract, `authoring.schema`, and `wedl/v1`. |
| `sourceIndependent`, `writesPerformed`, `validationComplete` | Respectively `true`, `false`, `false`. |
| `requestSchema` | Self-contained JSON Schema for the consequence Plan, including optional semantic analysis. |
| `recordKinds` | The 28 creatable kinds and update-only world: creation support, required/optional create and update fields, literal `createDefaults`, and status choices. |
| `vocabularies` | Native knowledge, acquisition, story-point and generational transition choices. |
| `bounds` | Native byte, structural and collection limits. |
| `nativeChecks` | Checks that require the native decoder or source-aware preview. |

Record field lists come from the same metadata used by the typed editor. Update
requests require `kind`, preserve omitted fields and do not accept a new `id`.
The world cannot be created through `record.create`; use `init`. A field being
optional means it can be omitted, not that explicit `null` removes it. Hypotheses
retain their narrower field set and separate noncanonical semantics.

## Plan syntax and native validation

The Plan contains `expectedFingerprint`, 1–64 `operations`, and optional
`analysis`. The two operations are `record.create` with a retained YAML `draft`,
and `record.update` with exact `entityId` and retained YAML `patch`. See
[CONSEQUENCES.md](CONSEQUENCES.md) for complete preview semantics.

Analysis retains its exact horizon, required/advisory policy and up to 100
expectations. Its seven predicate variants are described in `requestSchema`.
Analysis ticks/orders and exact numeric operands keep their native string encoding.
Time fields inside retained record YAML use the native YAML integer encoding.
Omitting analysis is allowed; explicit `null` is not. Optional relationship
comparison fields also distinguish omission from null.

JSON Schema checks the request structure. The native decoder additionally checks
duplicate keys, complete UTF-8 byte/structural budgets, exact numeric ranges and
retained operands. Typed YAML, world declarations, references, timeline rules
and whole-candidate consistency still require the existing native preview.
Schema validation alone never authorizes a write or establishes complete domain
conformance. Read the returned `nativeChecks` with the schema.

## Browser

Open **Consequences → Authoring request reference → Load reference**. The record
kind selector shows each kind's exact field guidance; **Download JSON** saves the
complete native result, including `requestSchema`. The full JSON is also
available under **Complete machine-readable guidance**.

Reading or selecting the reference leaves the unsent Plan buffer and story
horizon unchanged. Source refresh and horizon changes retain the static result.
Leaving the tab clears it; delayed older responses cannot restore it or enable
a newer pending read. Malformed/incomplete reference responses leave download
disabled.

## Story choices

`authoring context [--max-items N]` reads complete author choices from one
validated source capture. Unlike the static schema, it uses `--repo` and accepts
`--revision` for an original committed snapshot. The author HTTP route is
`GET /api/authoring/context?maxItems=4096`. Current-worktree sessions capture
current source; immutable sessions retain their selected snapshot.

The ordinary envelope and result both carry `fingerprint` and `revision`.
The result has `operation: authoring.context`, `sourceIndependent: false`,
`writesPerformed: false`, and `validationComplete: false`. Its main fields are:

| Field | Contents |
| --- | --- |
| `worldId`, `defaultTimeline`, `currentTime` | Exact world identity and native current time; tick/order are decimal strings. |
| `capabilities`, `families` | Declared capabilities and availability/requirements for all nine starter families. |
| `timelines` | IDs, labels, source fields and complete `definitionYaml`, including optional origins. |
| `records` | Exact record targets, labels, kinds, statuses, paths and available `scaffoldFamilies`. |
| `members` | Exact section/transition targets, owner kinds, labels and original source fields. |
| `declarations` | Presence and complete YAML for profiles, predicates, state/profile bindings, state keys and relationship metrics. |
| `profiles`, `predicates`, `stateProfileBindings`, `stateKeys`, `relationshipMetrics` | Useful identifier choices plus retained definitions where applicable. |
| `selectionCount`, `maxItems` | Complete selection-row count and requested bound. |

Member choices reuse native brief references: description sections; world
calendars, eras and anchors; generational initialization/transitions; story-point
lifecycle transitions; and knowledge/relationship transitions. They are full
author choices, with no character knowledge or admission grants.

Opaque declarations use `{present: false, yaml: null}` when absent and
`{present: true, yaml: "..."}` when supplied. YAML strings preserve native scalar
types and large integers. These strings are serialized values, not copies of
original comments or lexical formatting. Empty declarations remain distinct
from omitted declarations. Exact retained record bytes belong to scaffolds.

The row bound is 1–4096, default 4096, counting families, timelines, records,
members and all declaration-choice rows together. The complete result is limited
to 1 MiB of encoded JSON. Overflow refuses the whole read without truncation.

## Editable starters

Use exact IDs and the fingerprint returned by context:

```sh
wedl-rs --repo WORLD authoring scaffold
wedl-rs --repo WORLD authoring scaffold --family descriptions --entity "$ENTITY_ID" --expected-fingerprint "$FINGERPRINT"
wedl-rs --repo WORLD authoring scaffold --family generational --id "$NEW_ORGANIZATION_ID" --title "New organization" --expected-fingerprint "$FINGERPRINT"
```

The CLI defaults to `changeset`, using the captured world record if no entity
is selected. Omitting `--expected-fingerprint` uses that command's own source
capture. Supplying it requires an exact match. Historical reads may generate a
Plan against their original snapshot; publication retains the ordinary current
source guards.

HTTP uses `GET /api/authoring/scaffold?request=...` with a URL-encoded closed JSON
request. `expectedFingerprint` and `family` are required. Optional `entity`, `id`
and `title` must be nonempty strings when present; explicit nulls, unknown fields
and repeated fields are refused. Requests are bounded to 16 KiB; the complete
result is bounded to 1 MiB.

| Family | Selection and required capabilities |
| --- | --- |
| `changeset` | Any existing editable record; defaults to world. No extra capability. |
| `lore` | Exact existing editable record. No extra capability. |
| `briefs` | Context brief; `briefs-v1`. |
| `assertions` | Assertion, knowledge or world; `lore-core-v1` and `assertions-v1`. |
| `descriptions` | Any non-hypothesis editable owner; `descriptions-v1`. |
| `spatial-history` | Location or route; `spatial-core-v1` and `spatial-history-v1`. |
| `existence` | Existence episode; `generational-core-v1` and `existence-v1`. |
| `governance` | World, lore concept or assertion; `generational-core-v1`, `lore-core-v1` and `collective-parties-v1`. |
| `generational` | New organization with explicit `id` and `title`, no `entity`; `generational-core-v1`. |

Existing-record starters include a native `plan`, exact `target`, original
`retainedSource`, and names of protected/direct-source `retainedFields`. The
patch uses the editor's own field metadata, includes authored editable fields
and the exact Markdown body, and adds no omitted defaults. Existing typed
values outside the update codec are refused rather than silently discarded.
Native preview must prove a valid, byte-identical no-op before the starter is
returned. After editing, ordinary preview and writer encoding rules apply.

Description/spatial-history results also supply `memberField`, `memberPresent`
and a collision-free local `nextMemberId`; `membersYaml` is included only when
the source array is present. The helper does not append a member or turn an
absent array into an empty one.

The generational starter creates one organization with the explicit new ID/title,
an initialization at the exact native current time, and editable
`organization_kind: house`. Existing IDs are refused. It never enables a missing
capability. Its native preview must validate the candidate before returning it.

The result carries `operation: authoring.scaffold`, the same source metadata and
read-only flags as context, `canonicalNoOp`, and `previewValid`. A starter is not
an approval: edit and preview the returned `plan` through the existing consequence
or saved-edit/outcome workflow before publication.

## Browser story helpers

Open **Consequences → Story choices and editable starters → Load story choices**.
Choose an available family and eligible owner, or enter the new organization's
ID/title. **Generate starter** shows the native Plan and its retained details.
**Download choices** exports complete context; **Download Plan** exports only the
editable Plan. **Replace plan editor** explicitly replaces that tab's Plan buffer.
Loading, selecting, generating and downloading preserve an existing unsent Plan.

Selector changes invalidate a starter; tab departure and source refresh clear
context and starters. Older responses cannot restore stale output or clear a
newer pending request. Story-horizon changes leave these source-bound choices
alone: organization creation uses the story's native current time. The static
reference above remains independent of source refresh.

Shell completion is documented in [COMPLETION.md](COMPLETION.md). The contextual
helpers complete the named author-tooling slice; integrated correctness/scope
review remains in [CORRECTNESS_REVIEW.md](CORRECTNESS_REVIEW.md).
