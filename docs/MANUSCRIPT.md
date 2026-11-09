# Private manuscripts

Native manuscript source uses `wedl-author/v1`. Segments, works and reveal plans
are private author records, separate from canonical `story/` records, canonical source
fingerprints, ordinary retrieval and character knowledge. Resolving a work does
not authorize reader disclosure or publish its prose.

The executable reference is WEDL commit
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`, specifically
[`manuscript_source.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/manuscript_source.py),
[`author_data.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/author_data.py)
and their manuscript/work tests. Native adaptations are explicit below.

## Source identity

| Kind | Exact private path | ID prefix |
| --- | --- | --- |
| `manuscript-segment` | `author/segments/<id>.md` | `segment_` |
| `work` | `author/works/<id>.md` | `work_` |
| `reveal-plan` | `author/reveals/<id>.md` | `reveal_` |

The suffix is exactly 26 uppercase Crockford characters. The filename, envelope
ID and kind must agree. Every envelope requires `schema`, `kind`, `id` and
`title`; title is nonblank and at most 256 Unicode scalar characters. Unknown
fields, duplicate keys and incorrectly typed values are refused.

Sources use a Markdown YAML envelope opened and closed by standalone `---`
lines. LF and CRLF envelopes are accepted. The body remains exact UTF-8,
including its line endings, leading blank lines, trailing whitespace and lack
of a final newline. Raw bytes determine source hashes; parsing or reading does
not rewrite source.

### Segment

```yaml
schema: wedl-author/v1
kind: manuscript-segment
id: segment_00000000000000000000000001
title: Arrival
source_links:
  - kind: record
    id: loc_00000000000000000000000001
placement: {timeline: main, tick: 9007199254740993, order: 0}
pov_character_id: char_00000000000000000000000001
```

`source_links` is required and may be empty. Its order and duplicates remain
authored data. Each link has one of these exact shapes:

| Link | Fields | Resolution |
| --- | --- | --- |
| Record | `kind: record`, `id` | Exact canonical record ID |
| Description | `kind: section`, `owner_id`, `id` | Exact authored `descriptions` member on that owner |
| Transition | `kind: transition`, `owner_id`, `id` | Exact declared transition member on that owner |

A description link can retain an older member subsequently replaced by another
description. Display headings, prose headings and chronology members are not
description identities. Transition members are `transitions[].id` on knowledge
and relationships, `lifecycle.transitions[].id` on story points, and
`transitions[].transition_id` on the supported generational kinds. Generational
initialization is not a transition-link target in this source contract.

`placement` and `pov_character_id` are optional. Placement requires all three
fields shown above: a declared timeline, a signed 64-bit integer tick and a
signed 32-bit integer order. Strings, booleans, floats, null and omitted fields
are refused. POV resolves an exact character. Omission preserves an unplaced
segment or absent POV; scenes and prose never supply defaults. A link, placement
or POV grants no learning, canonical effect or disclosure permission.

### Work

```yaml
schema: wedl-author/v1
kind: work
id: work_00000000000000000000000001
title: River crossing
duplicate_policy: allow-explicit-reuse
chapters:
  - id: chapter_00000000000000000000000001
    title: Arrival
    segments:
      - kind: author-record
        id: segment_00000000000000000000000001
```

`chapters` and `duplicate_policy` are required. Each chapter has exactly `id`,
`title` and `segments`. Chapter IDs are unique within the work and use the
`chapter_` prefix. Chapter titles have the same text bounds as document titles.
Each chapter contains a nonempty array of exact `author-record` segment
references. An empty work, with no chapters, is valid.

`reject` forbids repeated segment use anywhere in that work, including across
chapters. `allow-explicit-reuse` retains every authored occurrence. Independent
works may reference the same segment under either policy.

## Reveal plans

[Reveal plans](REVEAL_PLANS.md) add exact occurrence-bound record, description and
literal-text admissions to one work. They share source parsing, independent
inspection, fingerprints, aggregate bounds and guarded saves with segments and
works. Their private notes and manuscript source are not reader output. The
separate reader consumer intersects admissions with independently supplied ordinary
authority; placement, POV and source links do not grant disclosure.

## Resolution and revisions

Occurrence identity is the work ID plus chapter ID and zero-based segment index
within that chapter. A flattened zero-based ordinal is derived from the literal
chapter and segment array order. Placement, source chronology, filenames and
IDs never reorder a work. Inserting an earlier segment changes later indexes;
an occurrence does not acquire a separate persistent ID.

The pure resolver receives an explicit canonical `World` and private manuscript
`Snapshot`. Both select the same revision: current source uses `null`, while
history requires one full lowercase 40- or 64-character commit OID. Historical
work, segment and canonical sources are read from that same original commit;
dirty working files cannot replace historical bytes. Symbolic revisions such
as `HEAD` and `WORKTREE` are refused by historical request admission.

Snapshot construction validates closed source structure and exact raw/projection
identity. Source listing and detail inspection remain available when a canonical
link is broken. Complete candidate save validation checks every segment, work and reveal-plan
dependency. Selected work resolution validates that work and every distinct
referenced segment; an unrelated segment's broken canonical link does not
change the selected work.

The `manuscripts-resolve` report contains private work metadata and notes,
ordered `chapters[].occurrences`, and a unique `segments` table. Each occurrence
contains `chapterId`, `segmentIndex`, `ordinal` and `segmentId`. Segment prose,
ordered `sourceLinks`, optional placement/POV, source path, SHA-256 and historical
blob identity occur once per unique segment. The segment table is sorted by ID;
the chapter occurrence arrays are the authority for work order. Placement tick
and order use decimal strings in JSON to retain exact values in browsers.

`proseCharacters` counts Unicode scalar characters in segment prose across all
occurrences, including reuse. `maxCharacters` instead bounds the **complete
serialized JSON report**, including labels, metadata and structure. Neither
bound truncates output. A successful report has `complete: true`,
`readerAuthorized: false`, `writesPerformed: false`, `canApply: false` and
`validationComplete: false`; the last field reflects the native project's
partial overall WEDL validation coverage.

## Native and author interfaces

All requests are closed JSON objects. Required nullable fields must be explicitly
present; duplicate keys, including escaped duplicates, are refused.

| CLI subcommand after `manuscripts` | Exact request fields |
| --- | --- |
| `list --file FILE` | `revision`, `kind`, `maxItems` |
| `detail --file FILE` | `revision`, `id`, `expectedSha256` |
| `resolve --file FILE` | `revision`, `workId`, `expectedAuthorFingerprint`, `maxOccurrences`, `maxCharacters` |
| `preview-save --file FILE` or `save --file FILE` | `expectedFingerprint`, `expectedAuthorFingerprint`, `expectedSha256`, `source` |

`revision` is null or a full commit OID; independent reads reject a global
`--revision` option. `kind` is null, `work`, `manuscript-segment` or `reveal-plan`; `maxItems` is
1–1,024 and filtered overflow refuses the complete catalogue. `expectedSha256`
is null or a lowercase SHA-256; null means no exact-file assertion on detail and
requires an absent target on save. `expectedAuthorFingerprint` is nullable only
for resolution. Both fingerprints are required exact lowercase SHA-256 values
for a save. `source` is the complete UTF-8 Markdown document, not a body patch.
Read request JSON is limited to 16 KiB; save request JSON is limited to 8 MiB,
with decoded source still subject to its 2 MiB document bound.

`GET /api/manuscripts/list`, `/detail`, `/resolve` and `/preview-save` accept the
same JSON in the single `request` query parameter. The author query limit is
16 KiB. Larger read/preview inputs can be downloaded intact and passed to the
CLI. Current local-author sessions accept the exact save request at POST
`/api/manuscripts/save`, including its canonical, whole-private and prior-file
pins. There is no rename or delete route. See
[browser publication](BROWSER_PUBLICATION.md) for the write boundary. The source
read envelope omits unrelated service-world fingerprints; the result identifies
its own selected source.

Detail returns exact `rawSource`, exact `frontmatterYaml`, `bodyMarkdown`, path,
source SHA and original blob metadata. Its structured frontmatter projects
placement tick/order as decimal strings; the original YAML remains unchanged.
List/detail require valid private structure but do not resolve canonical links.
Resolution validates its selected work closure. Saves validate every candidate
private document, including dependencies from other works and reveal plans, against current
canonical source. Preview/save use bounded canonical capture before validation.
A stale canonical, whole manuscript or original-file pin refuses even a
same-content retry. A successful same-content save performs no write.

Save previews report both before/after manuscript fingerprints and document
hashes, exact before/after source, and `canSave`; actual save reports exact after
source. Their `authorFingerprint` is the current pre-save fingerprint for a
preview and the resulting fingerprint after save. Neither operation grants
canonical application approval or writes Git history. The Manuscripts browser
retains original source bytes separately from its textarea until the user edits,
keeps original file pins when capturing current source pins, and invalidates
late reads after selection, navigation, imports or edits. Historical documents
remain read-only. The browser can save current private source or download the
same request for the CLI; saving does not approve canonical publication or
reader disclosure. See the executable
[manuscript walkthrough](../examples/manuscripts/README.md).

## Complete bounds and native adaptations

| Input or output | Bound |
| --- | --- |
| Raw document | 2 MiB |
| Markdown body or one source scalar | 1 MiB |
| Expanded source scalar bytes | 2 MiB per document |
| Expanded source graph | Depth 16; 16,384 nodes, including mapping keys |
| Source collections | 256 mapping entries; 1,024 entries per array |
| Manuscript snapshot | 1,024 documents; 16 MiB raw source |
| Complete expanded manuscript snapshot | 16 MiB string/body bytes; 1,000,000 nodes |
| Resolver `maxOccurrences` | Caller selects 1–4,096 |
| Resolver `maxCharacters` | Caller selects 256–1,000,000 serialized Unicode scalar characters |
| Complete report | 8 MiB serialized JSON |

The work source contract has no global 1,024-occurrence ceiling. For example,
1,024 occurrences in one chapter and one occurrence in another are valid when
the complete graph fits. The resolver's explicit result ceiling is separate.

The native YAML parser admits finite aliases by expanding them under the full
node, depth, collection and scalar budgets before constructing the source value.
The complete snapshot also charges expanded string and Markdown body bytes and
nodes incrementally before retaining each document; small aliased source files
cannot bypass the combined allocation bounds.
This differs from upstream's refusal of repeated container aliases. Recursive
or explosive aliases, custom tags, nonfinite values, nonstring mapping keys and
duplicate keys are refused. No source is partly admitted after a bound fails.

Current private writes reuse the native writer lock and atomic file replacement,
with current canonical, complete manuscript and exact file pins. Manuscript
fingerprints hash length-framed paths and exact raw bytes in a separate domain;
revision and blob identity are retained separately. Private saves introduce no
automatic Git commit, canonical publication or reader authorization.
