# Private reveal plans

Reveal plans use the existing private `wedl-author/v1` Markdown store. A plan
selects one work and associates explicit admissions with exact authored
occurrences. It does not reorder the work, derive story time, alter character
knowledge, or supply reader authority. Its title, body notes and source metadata
remain private author input.

The fixed executable reference is
[`reveal_plans.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/reveal_plans.py)
and [its tests](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/tests/test_reveal_plans.py)
at `f0e885ccebc279eff65a5db2984d01ebc6647b2b`. Native storage and complete input
bounds follow [MANUSCRIPT.md](MANUSCRIPT.md).

## Closed source

The exact path is `author/reveals/reveal_<26 uppercase Crockford characters>.md`.
The document has exactly `schema`, `kind`, `id`, `title`, `work` and `entries`.
Common identity/title rules are shared with segments and works.

```yaml
schema: wedl-author/v1
kind: reveal-plan
id: reveal_00000000000000000000000001
title: Private reveal outline
work: {kind: author-record, id: work_00000000000000000000000001}
entries:
  - milestone:
      chapter_id: chapter_00000000000000000000000001
      segment_index: 0
    admissions:
      - kind: record
        target: {kind: record, id: loc_00000000000000000000000001}
      - kind: section
        target:
          kind: section
          owner_id: char_00000000000000000000000001
          id: description_00000000000000000000000001
      - kind: text
        id: revealtext_00000000000000000000000001
        markdown: "Exact explicitly revealed prose."
```

`work` is an exact `author-record` reference to a work. Each entry contains only
`milestone` and `admissions`. A milestone contains exactly `chapter_id` and
`segment_index`: the chapter ID and zero-based integer position within that
chapter. The index must be in 0–1,023 and resolve an actual occurrence. It is
not a segment ID, story clock or flattened ordinal. Reused segments therefore
retain distinct possible milestones.

Entries must be unique and strictly increasing in the work's authored order.
Placement and chronology never sort them. `entries` and each `admissions` array
may be empty and have at most 1,024 entries; the complete source graph and byte
budgets also apply. A plan with no entries may select an empty work.

| Admission kind | Exact fields | Source validation |
| --- | --- | --- |
| `record` | `kind`, `target: {kind: record, id}` | The canonical record exists at the selected source |
| `section` | `kind`, `target: {kind: section, owner_id, id}` | Exactly one authored `descriptions` member exists on that owner |
| `text` | `kind`, `id`, `markdown` | A valid local `revealtext_` ID and exact string, including an empty string |

Canonical admissions can refer to replaced description members; display headings,
chronology members and prose headings are not description identities. Transition,
work and private segment admissions are unsupported. Text preserves the decoded
literal Markdown string exactly, including Unicode, CRLF and trailing spaces.

Admission identities `(kind, owner ID when present, ID)` are unique across the
entire plan. A text ID cannot be reused with changed wording. A record admission
and a section admission on that owner remain distinct inputs; their eventual
projected units may overlap and are deduplicated by the reader consumer.

## Resolution and private saving

`manuscript::resolve_reveal_plan(world, authors, plan_id, max_occurrences)` is a
pure source validator/compiler. It checks exact raw/projection/fingerprint binding,
matching source revision, canonical validation, the selected work, every distinct
referenced segment, all milestones and all canonical admissions. The compiled
`RevealPlan` carries plan/work IDs, ordered occurrences, and typed entries with
derived flattened ordinals. It contains no copied manuscript prose or private
notes. It is not an authorization object.

Private author resolution accepts a coherent current capture (`revision: null`)
or one full lowercase 40- or 64-character Git commit OID. This current-source
option is a deliberate native adaptation for the existing working-file writer.
Reader consumers require original committed source and independently installed
ordinary authority. A working source fingerprint is not a reader publication.

List/detail remain structural source inspection, so a plan with a broken canonical
link, absent work or unresolved milestone can still be inspected if its closed
source shape is valid. Complete resolution refuses such semantic failures.
Selected resolution ignores an unrelated document's broken semantic link;
complete candidate saves validate every segment, work and reveal plan. A work edit
that would invalidate an existing reveal milestone is refused as a single-file
save. No multi-file private transaction is inferred.

Plans share the manuscript fingerprint and all aggregate document/raw/expanded
source budgets with segments and works. Existing canonical/whole-private/prior-file
pins, writer lock, bounded regular-file reads and atomic single-target publication
apply unchanged. Editing reveal notes changes the private fingerprint but never
the canonical fingerprint, search corpus or character knowledge. Native saves do
not commit Git history.

## Author interfaces

Use the existing `manuscripts list`, `detail`, `preview-save` and `save` commands
for exact plan source. `list.kind` additionally accepts `reveal-plan`; detail
accepts exact `reveal_` IDs. List, detail and preview have GET-only author HTTP
counterparts. Current local-author sessions can save the same source through
POST `/api/manuscripts/save` or download its request for the CLI. Historical
selections remain read-only. See [browser publication](BROWSER_PUBLICATION.md).

`reveal-plans resolve --file FILE` and `GET /api/reveal-plans/resolve?request=JSON`
accept this closed request:

```json
{
  "revision": null,
  "planId": "reveal_00000000000000000000000001",
  "expectedAuthorFingerprint": null,
  "maxOccurrences": 4096,
  "maxCharacters": 100000
}
```

All fields are required, including nullable ones. `expectedAuthorFingerprint`
is null or an exact lowercase SHA-256. Complete bounds are 1–4,096 occurrences,
256–1,000,000 serialized Unicode scalar characters, and 8 MiB of JSON. Read request
JSON and author URL queries retain their existing 16 KiB bounds. The standalone
CLI resolver rejects a global `--revision`; use its explicit request revision.

The author result has operation `reveal-plans-resolve` and scope
`private-reveal-plans`. It contains:

- The canonical fingerprint, selected revision and complete manuscript fingerprint.
- `plan` metadata, exact private `bodyMarkdown`, original source path/hash/blob and `workId`.
- The same `work`, ordered `chapters`, unique `segments`, occurrence/prose counts and bounds as work resolution.
- Ordered `entries`, each with camelCase `milestone`, derived `ordinal` and admissions retaining the source record/section/text shapes.

The complete serialized report, including all revealed text and private source
metadata, must fit the requested character bound. Results are never truncated.
A successful author result reports `complete: true`, `readerAuthorized: false`,
`writesPerformed: false`, `canApply: false` and `validationComplete: false`.
Its private work/segment/plan content must never be substituted for reader output.

The installed retcon consumer still covers manuscript segments and works. Reveal
plans share structural source capture but do not acquire invented retcon nodes or
dependency edges. Reveal-only semantic changes are outside that consumer's witness;
this does not authorize disclosure or claim that a reveal plan remains valid.
