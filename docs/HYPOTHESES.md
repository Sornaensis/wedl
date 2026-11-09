# Author hypotheses

Hypotheses retain explicit author possibilities across all story time. They are
always `nonCanonical: true`. Creating, adopting, or rejecting one changes only
its authored record through the existing reviewed record workflow. Adoption
does not create its supporting records, execute an event, establish truth,
advance a cursor, or teach a character anything.

The implementation is compared with WEDL commit
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`, specifically
[`query.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/query.py),
[`authoring.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/authoring.py),
[`validation.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/validation.py),
[`search.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/search.py),
and [`lexical.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/lexical.py).
These source blobs were verified against the pinned Git tree. The native
request, projection, and search envelopes below are explicit adaptations, not
claims of upstream wire compatibility.

## Reads

The author CLI provides `hypotheses list`, `hypotheses detail HYPOTHESIS_ID`,
and `hypotheses preview --file REQUEST.json`. The corresponding GET routes are
`/api/hypotheses`, `/api/hypothesis`, and `/api/hypotheses/preview`. These
operations have no character perspective or author horizon argument.

List accepts `status` (`open`, `adopted`, or `rejected`), a literal `text`
substring, `maxItems`, and `maxCharacters`. Unicode casefolding applies to
`title + "\n" + statement + "\n" + body`; alternatives, context, aliases, and
tags are not list-filter fields. Matching records are ordered by exact ID. If
all matches do not fit `maxItems`, the read fails instead of returning a partial
collection. Detail accepts an exact `hypothesisId` and `maxCharacters`.

The native result reports `perspective: "author"`,
`scope: "author-all-time-hypotheses"`, its source fingerprint and revision,
`nonCanonical: true`, and `complete: true`. List returns `hypotheses`, `count`,
and the filters; detail returns `hypothesis`. Each projection preserves the
statement, ordered alternatives including duplicates, context, placement, and
resolution. Referenced subjects and placement/resolution targets contain exact
`id`, `kind`, and `title`; the hypothesis has a native source citation. Body is
searchable but is not included in this uncertainty projection.

Pinned upstream projections intentionally omit opaque IDs and expose titles
and kinds. Native exact IDs and citations support this application's existing
author selection and review controls. They do not give these records canonical
or character visibility.

## Closed workflow request

Every request has an exact lowercase source fingerprint and one `action`.
Unknown or duplicate JSON fields, scalar coercions, and explicitly null
optional values are rejected. Optional fields are omitted. Requests preserve
literal nonblank strings, including surrounding whitespace.

```json
{
  "expectedFingerprint": "<64 lowercase hexadecimal characters>",
  "action": {
    "type": "create",
    "id": "hyp_00000000000000000000000001",
    "title": "Possible transcription error",
    "statement": "The repair entry may have been copied.",
    "subjects": ["obj_00000000000000000000000001"],
    "alternatives": ["A copied entry", "An incomplete repair"],
    "context": "Compare the log headings.",
    "placement": {"timeline": "main"}
  }
}
```

Create requires a new exact hypothesis ID, title, statement, nonempty distinct
existing subject IDs, nonempty alternatives, and context. Optional placement
may contain `timeline`, `sceneId`, `eventId`, and `locationId`, each resolving
against the captured source. It supplies no tick, order, or current-time
default. The source record starts with `status: open`,
`domain: author.possibilities`, placement context copied from `context`, and the
body `# {title}\n\n{statement}\n`.

Adopt uses `{"type":"adopt","hypothesisId":"…","canonicalRecords":["…"]}`
with an optional nonblank `note`. Reject uses
`{"type":"reject","hypothesisId":"…","note":"…"}` and requires that note.
Adoption requires distinct already existing targets with these statuses:

| Target kind | Adoptable status |
| --- | --- |
| Hypothesis | None |
| Scene or conversation | `active`, `closed`, `retired` |
| Every other supported kind | `canonical`, `retired` |

The target must also satisfy its own native source validation. Settlement is
independent of story time: a future canonical event is eligible. There is no
extra kind whitelist and no invented terminal transition; an explicitly
authored resolution may replace an earlier adoption or rejection.

`hypotheses::plan` produces one ordinary `record.create` or `record.update`
operation. Preview includes this exact exportable Plan and the existing complete
consequence report: candidate source, hashes, diff, and diagnostics. It reports
`scope: "local-author-hypotheses"`, `writesPerformed: false`, and
`canApply: false`. Source publication remains in the existing guarded
record/Actions workflow. Historical snapshots permit pure preview and export;
they do not authorize a write against today's source.

## Source validation and preservation

Raw hypotheses require explicit status, nonblank statement/context, a nonempty
array of distinct existing subjects, nonempty literal alternatives, and a
nonempty placement mapping. Placement permits only `timeline`, `scene`,
`event`, `location`, and `context`. Timelines must be declared; nonnull links
must reference the specified kind. Raw placement links may be null, and context
is optional inside placement when another permitted placement key is present.

Open records have no resolution or `resolution: null`. Adopted resolution
permits only nonempty distinct settled `canonical_entities` and an optional
nonblank note. Rejected resolution permits only a required nonblank note.
The typed record codec can explicitly reopen a note with `status: open` and
`resolution: null`.

The closed nonoperative frontmatter vocabulary is schema, kind, id, title,
domain, status, tags, aliases, section_audiences, provenance, statement,
subjects, alternatives, context, placement, and resolution. Native hypotheses
continue to reject canonical concept links; pinned upstream's optional lore
extension is not implemented for them. Occurrence clocks, state, effects,
threads, and other operational fields are rejected. The validator does not
interpret placement as StoryTime.

Adopt/reject replace only status and resolution. All other frontmatter values
and body are preserved; an unchanged semantic update retains exact original
bytes. ID-looking statements, alternatives, context, and notes remain literal
strings. The generic codec leaves reference existence to complete-candidate
validation so explicit forward references in ordinary Plans still work.

## Explicit search lane

`search QUERY --include-hypotheses` adds `hypothesisMatches` beside the unchanged
canonical `matches`. This separate lexical lane searches title, statement,
alternatives, context, and body, including all hypothesis statuses and all
story time. It remains lexical when canonical retrieval is semantic or hybrid.
Any selected thread filter suppresses the lane, as does a different kind filter.
Limit applies independently to each lane; at most twice that many rows return.

The hypothesis score is four times each distinct quoted phrase's occurrence
count plus each distinct term's word-boundary occurrence count, with pinned
stopword filtering and fallback. Matches sort by descending score and exact ID.
Quoted escapes, failed-boundary overlaps, and nonoverlapping accepted matches
follow the pinned lexical rules. These occurrence scores do not rank or change
canonical BM25/similarity results, inputs, model hashes, or statistics.

The dependency-free native implementation uses the existing Unicode 17.0 full
default casefold table and a generated Unicode 17.0 letter/number-plus-underscore
word table. Combining Alphabetic marks are excluded, matching Python's word
category rule; the Unicode version is explicit rather than dependent on the
Rust or Python runtime. The table's primary source is
[`UnicodeData.txt`](https://www.unicode.org/Public/17.0.0/ucd/UnicodeData.txt),
SHA-256 `2e1efc1dcb59c575eedf5ccae60f95229f706ee6d031835247d843c11d96470c`.

## Bounds

| Input or output | Native bound |
| --- | --- |
| Complete workflow request JSON | 262,144 UTF-8 bytes |
| Title | 1,024 UTF-8 bytes |
| Statement, context, note, each alternative | 65,536 UTF-8 bytes each |
| Subjects, alternatives, canonical records | 1–1,000 members each |
| Placement timeline | 128 UTF-8 bytes |
| List text filter | 4,096 UTF-8 bytes |
| Complete matching list | `maxItems` 1–1,000 |
| Complete native read/preview result | `maxCharacters` 256–1,000,000 Unicode scalars; also 8 MiB encoded JSON |
| Search query | 1–4,096 UTF-8 bytes, at most 64 whitespace-delimited words |
| Search results | 1–100 per lane |
| Hypothesis search input | 1,024 records and 524,288 UTF-8 bytes of complete joined search text |

The generated Plan also obeys its independent 262,144-byte encoded bound, which
can reject a request that fits before its statement is repeated in the body.
Complete read budgets include metadata, escaping, expanded reference titles,
and all list entries; serialization checks them before bulk projection
allocation. Preview budgets include the request, Plan, and full candidate
report. Nothing is clipped to manufacture `complete: true`.

Workflow input bounds do not retroactively add those text/collection limits to
otherwise valid raw source. Source validation remains explicitly partial:
`validationComplete: false` is unchanged. The
[executable example](../examples/hypotheses/README.md) covers reviewed creation,
author reads, separate search lanes, and adoption/rejection Plans.
