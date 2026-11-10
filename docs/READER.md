# Reader presentation v1

This native slice implements the reader disclosure boundary from upstream commit
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`. Private sources are described in
[REVEAL_PLANS.md](REVEAL_PLANS.md), and pure admission in [REVEALS.md](REVEALS.md);
they grant no reader authority by themselves. The native
wire protocol is `wedl-reader-presentation/v1`.

## Separate authority and origins

`PresentationService::new(root)` does no I/O and installs no policy. A valid
request with no installed policy returns the same unavailable result before
checking whether its repository or sources exist. Wrong publication, revision,
work/plan selection, or progress ceiling has the same behavior.

A trusted operator captures `reader::repository_identity(root)` and installs a
`PublicPolicy` or `SimulationPolicy`. The identity binds the exact worktree, Git
directory, and common Git storage paths. Policies are owned Rust values, have no
request deserializer, and never come from reader headers, cookies, source files,
query parameters, or the public body. Each policy pins one full immutable commit,
one work and reveal plan, a world/record/section whitelist, an explicit generation,
a maximum zero-based occurrence, and a 32-byte cursor secret. Public policy also
pins the publication handle, story horizon, and literal-text permission.

Simulation is independently installed, with its own principal and author
permissions. Effective permissions are the intersection of the simulation
whitelist and these author permissions. Having access to the author browser or
private sources does not install simulation permission. Author preview selects a
horizon and text policy within that installed simulation context.

Cloned services share installed state. `install_public(None)` and
`install_simulation(None)` revoke their respective policies. Every response,
including a source error or limit result, rechecks the captured policy's exact
installation identity immediately before emission. Reinstalling an equal-valued
policy therefore also invalidates an in-flight response. Source work rechecks
repository routing before and after capture; a changed or unavailable repository
identity suppresses source diagnostics.

The isolated listener binds only loopback and serves exactly:

| Route | Method | Purpose |
|---|---|---|
| `/` | GET | Reader page |
| `/reader.js` | GET | Reader script |
| `/api/reader-presentation/page` | POST | One admitted page |
| `/api/reader-presentation/search` | POST | Search admitted items |
| `/api/reader-presentation/choices` | GET | Installed public edition and named positions |
| `/api/reader-presentation/read` | POST | Browser reading position and revealed items |
| `/api/reader-presentation/find` | POST | Browser search at that reading position |

It mounts no author, session, source, policy, entity, private search, preview, or
mutation routes. The reader requires one loopback Host header; an Origin, when
present, must identify that exact HTTP authority. Duplicate Host/Origin headers,
foreign origins, malformed ports, and rebinding host names are refused. No CORS
grant is returned. Responses use `Cache-Control: no-store`, `Vary: Origin`,
`X-Content-Type-Options: nosniff`, and a policy forbidding external content,
frames, and inline scripts. Reader prose is rendered as inert text; raw HTML,
images, executable URLs, and authored WEDL links are not followed.

The local author browser retains its separate GET
`/api/reader-presentation/preview?request=JSON` route. Its envelope contains only
`protocol: "wedl-rust/v1"` and the presentation `result`; it does not attach an
unrelated loaded world's fingerprint or revision. The author query decoder's
16 KiB **encoded query** bound also applies. Use the CLI for requests that fit the
presentation body bound but exceed that encoded GET bound.

## Operator entry points

```sh
wedl-rs --repo /absolute/world serve-reader --policy /absolute/public-policy.json
wedl-rs --repo /absolute/world reader page --policy /absolute/public-policy.json --file page.json
wedl-rs --repo /absolute/world reader search --policy /absolute/public-policy.json --file search.json
wedl-rs --repo /absolute/world reader preview --policy /absolute/simulation-policy.json --file preview.json
wedl-rs --repo /absolute/world serve --reader-simulation-policy /absolute/simulation-policy.json
```

The default reader port is 8766; the author port is 8765. Omitting a policy starts
or executes the closed boundary and yields unavailable reads. Reader CLI paths
bypass generic current-world loading. They accept the revision inside the request
and policy, rather than a separate `--revision`. CLI success exits 0; a closed
failure exits 2 while printing its presentation JSON.

Policy files are explicit operator inputs, at most 1 MiB, decoded once at startup
or invocation. They are not repository discovery files. Restart the CLI server
to reload a changed file; embedding callers can install/revoke policies directly.
No HTTP endpoint changes policy.

A public configuration has these required fields (substitute real identifiers,
commit, whitelist, and a privately generated 32-byte secret):

```json
{
  "schema": "wedl-reader-policy/v1",
  "kind": "public",
  "revision": "FULL_LOWERCASE_COMMIT_OID",
  "workId": "work_00000000000000000000000001",
  "planId": "reveal_00000000000000000000000001",
  "canonicalScope": {
    "worldId": "EXACT_WORLD_ID",
    "recordIds": ["EXPLICIT_CANONICAL_RECORD_ID"],
    "sections": null
  },
  "generation": "edition-one",
  "maxOccurrence": 2,
  "cursorSecret": "64_LOWERCASE_HEX_CHARACTERS",
  "publicationId": "example-edition",
  "at": {"timeline": "main", "tick": "20", "order": "0"},
  "textPolicy": "none"
}
```

The optional public `presentation` object publishes browser labels for the pinned
work and reveal plan. Use an `edition` string and a `positions` array of objects
such as `{"occurrence": 0, "label": "Arrival at the Quay"}`. Both fields may be
omitted. Each label is nonblank, at most 120 Unicode scalars and 480 UTF-8 bytes,
and has no control characters. Occurrences must be unique integers from 0 to
1023. Choices expose only positions that exist in the pinned work and are
within `maxOccurrence`. The edition defaults to **Published edition**, and
eligible unnamed positions remain available through previous and next navigation.

Simulation uses `kind: "author-simulation"`, replaces `publicationId`, `at`, and
`textPolicy` with `principalId` and `authorPermissions`, and otherwise has the
same common fields. `authorPermissions` has the same shape as `canonicalScope`.
Unknown or duplicate fields, duplicate record/path entries, coercions, and
structural budget excess are rejected. Configuration decoding does not infer a
whitelist from author access.

The browser requests choices without a body or query. Reading and search submit
`edition` and `position` opaque selectors, a `cursor` (initially `null`), and
`query` for search only. Selectors are scoped to the installed public policy and
rechecked against its pinned source for every request. Unavailable positions,
including bookmarks from replaced policies, share the same recovery state. The
browser receives published labels, permitted text, counts and opaque navigation.
The machine page and search protocol below remains available separately.

`sections: null` permits the selected records' ordinary fields. A map is an
explicit whitelist: missing owners and empty path arrays permit no fields.
Paths include `frontmatter.title`, `bodyMarkdown.<heading-or-source>`, and
`descriptions.<description_id>`; parent paths permit descendants. Ordinary owner
eligibility requires `frontmatter.title` permission even for an exact section;
that gate does not disclose the owner's title. Scope membership still intersects
audience, perspective, and time.
Even an ordinarily permitted record appears only after its explicit reveal
admission. An exact-section admission never attaches its owner's title or body.

## Closed requests and results

Public page bodies contain exactly:

```json
{
  "protocol": "wedl-reader-presentation/v1",
  "publicationId": "example-edition",
  "revision": "FULL_LOWERCASE_COMMIT_OID",
  "occurrence": 0,
  "limit": 20,
  "cursor": null
}
```

Search adds a required `query`; page forbids it. Preview replaces publicationId
with `workId`, `planId`, `operation` (`page` or `search`), `at`, and `textPolicy`.
Preview search additionally requires `query`. It has no grant or principal field.

Bodies are UTF-8 JSON, at most 16 KiB, with duplicate and unknown fields rejected.
The revision is a full lowercase 40- or 64-hex commit OID; symbolic refs and
abbreviations are invalid. `occurrence` is an integer 0..1023; `limit` is 1..100;
`cursor` is required and is null or the exact native cursor below. Queries are
nonblank, at most 256 Unicode scalars and 1024 UTF-8 bytes. Preview `at` contains
exact `timeline`, `tick`, and `order`; tick/order are canonical signed decimal
strings fitting i64/i32, without plus signs, leading zeroes, or `-0`. Public
requests cannot override their installed horizon. Query parameters are forbidden
on public POST routes.

An available result contains exactly `protocol`, `mode`, `operation`, `outcome`,
`code`, `revision`, `occurrence`, `at`, `navigation`, `items`, `count`, and
`nextCursor`. `mode` is `reader` or `author-preview`; code is
`READER-PRESENTATION-OK`. Each item contains only `kind`, `id`, `ownerId`, `title`,
and `markdown`; absent owner/title values are null. `count` counts all matching
admitted units, before pagination. An empty admitted result is available.

Reveal progress follows literal authored work order, including explicit segment
reuse; story chronology never reorders occurrences. Admissions are cumulative.
Record and exact-section paths deduplicate the same resulting section. Ordinary
public/ordinary/time filtering precedes disclosure; text is admitted verbatim
only under `all-at-pinned-plan`. `none` removes literal text before search,
counts, dataset digests, cursors, and result bounds. It does not bypass source
validation or source-size limits.

Navigation contains only `previousOccurrence` and `nextOccurrence`, bounded by
the work and installed ceiling. Responses omit private work/plan/chapter labels,
segment IDs or prose, source paths, blob hashes, source citations, raw
frontmatter, hidden totals, private diagnostics, and policy metadata. Exact
sections use their own heading. Section-only admissions do not include sibling
sections, backlinks, or owner prose.

Search is a deterministic substring test of admitted title or Markdown under
full default Unicode **17.0.0** case folding. It preserves admission order, adds
no ranking or index, and performs no normalization or locale-specific Turkic
fold. The generated C/F mapping table records the official source URL, SHA-256,
and Unicode notice in `src/reader/casefold.rs`. Expansions such as `ß → ss`, final
sigma, and ligatures are supported. Search cannot reduce source or whole-admitted
budgets.

## Cursors, refusal, and bounds

The native cursor is exactly 72 lowercase hex characters: a four-byte big-endian
offset followed by a 32-byte HMAC-SHA256. The MAC covers a mode-specific domain,
a deterministic context digest, and the offset. Context includes repository and
immutable source identities; work/plan blobs; public handle or simulation
principal/permissions; installed and effective scopes; horizon; generation;
text policy; ceiling; operation; occurrence; limit; exact query or omission; and
the complete ordered admitted dataset digest. Only offset and MAC appear in the
cursor. Changed query case invalidates a cursor even if its folded search matches.
Changed policy, secret, source data, mode, or storage also invalidates it. Cursors
confer no authority: installation checks run again before their source context
is resolved. HMAC uses the existing SHA-256 dependency and RFC 4231 vectors.

Every closed failure contains exactly `protocol`, `mode`, `outcome`, `code`, and
`message`, with no partial items, navigation, counts, cursor, or source details:

| Outcome | Code | Message | HTTP |
|---|---|---|---|
| unavailable | READER-PRESENTATION-UNAVAILABLE | Reader presentation unavailable. | 409 |
| invalid | READER-PRESENTATION-SOURCE | Invalid reader presentation source. | 400 |
| limit | READER-PRESENTATION-LIMIT | Reader presentation limit exceeded. | 422 |

`invalid` source is available only after simulation authorization; public source
failures are uniformly unavailable. Known admitted resource excess is limit.
Malformed transport remains the generic `usage_error` JSON and HTTP 400.

| Complete resource | Native ceiling |
|---|---:|
| Canonical source records / raw bytes | 4096 / 32 MiB |
| Combined manuscript/works/reveal source documents / raw bytes | 1024 / 16 MiB |
| Each canonical or private source blob | 2 MiB |
| Work occurrences | 1024 |
| Admitted units / admitted Markdown UTF-8 | 1024 / 2 MiB |
| Complete selected JSON after escaping, before pagination | 4 MiB |

Authenticated Git trees and object-size metadata are bounded before full source
capture. Existing original-object verification, tree traversal, expanded YAML,
node, depth, and private-source budgets also apply. A typed metadata preflight
classifies known count/blob/aggregate byte excess as limit. Existing source parser
refusals, including expanded YAML exhaustion, remain uniform public unavailable
or authorized preview invalid; error-message text is never used to classify them.
The complete selected JSON is bounded before slicing any page. Original admitted
items are bounded before search, so an unmatched query cannot evade those limits.

All read paths are pure: no cache, database, source, policy, learned-history, lock,
Git object, or readiness writes. Broken or changed live files cannot alter the
pinned edition. The native source resolver may accept up to 4096 work occurrences
for author use; that never widens this service's independent 1024 ceiling.

## Native scope

This is an isolated anonymous reader authority for explicit local operation;
it does not authenticate individual people or provide public Internet hosting.
The native transport uses closed POST JSON, fixed hex cursors, SHA-1/SHA-256 Git
pins, and the existing tighter manuscript snapshot budgets. These are explicit
native choices; interoperability with upstream Python cursor bytes, authentication
sessions, remote publication infrastructure, and richer Markdown/link rendering
are not claimed.
