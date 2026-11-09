# Pure reader disclosure

`reveal::present` evaluates a private reveal plan against independently supplied
ordinary reader authority. It performs no filesystem, Git, cache, learning or
canonical writes. Private source inspection and work assembly do not create a
reader grant. [REVEAL_PLANS.md](REVEAL_PLANS.md) defines the separate author
source and compiler.

The executable reference is WEDL commit
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`:
[`reveal_plans.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/reveal_plans.py),
[`description_runtime.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/description_runtime.py),
[`audience.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/audience.py),
and the temporal body bounds in
[`search.py`](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/search.py).
The native character and subtree adaptations are explicit below.

## Trusted input and two independent orders

`ReaderGrant` is a Rust issuer input with no request deserializer. It binds the
reader identity, optional exact plan ID, immutable revision, canonical scope,
story horizon, lens, audience labels and perspective labels. The revision must
be a full lowercase 40- or 64-character OID matching both captured sources.
The selected world and timeline must also match. Empty label sets and unknown
character lenses are refused. A grant selecting no plan returns no items.

For a selected plan, `present` verifies the caller's mutable canonical and
private captures against their exact raw source before compiling the full work,
segment and canonical admission closure. A later or scope-denied broken link
still makes that selected plan invalid. The crate-private `project_resolved`
entry point accepts a freshly compiled plan from the installed service, avoiding
a second full source pass. A caller-mutated compiled plan is not that input.

The occurrence argument is the zero-based ordinal in the resolved work. All
entries through that ordinal are cumulative; repeated segments are separate
occurrences. Manuscript placement and POV do not reorder admissions or infer a
story horizon. The grant's independent `(timeline, tick, order)` controls
ordinary dated visibility, using exact signed integer clocks.

## Scope intersection

`CanonicalScope` selects one world and a set of exact canonical record IDs.
An absent `sections` map grants those records' fields. A present map is a
whitelist; an absent owner or empty path set grants no fields. `intersect`
retains common records and the more specific of compatible field paths.
It never widens either input. Trusted author simulation intersects the installed
publication scope with independently installed author permissions.

| Permission path | Meaning in this projection |
| --- | --- |
| `frontmatter.title` | Required for ordinary owner availability; it does not cause a section-only admission to emit the owner title |
| `bodyMarkdown.source` | Body preamble before the first heading |
| `bodyMarkdown.<exact heading>` | One ordinary body section, including its own heading |
| `bodyMarkdown` | All body section paths, still subject to audience and time |
| `descriptions.<exact description ID>` | One authored dated description member |
| `descriptions` | All description member paths, still subject to audience, perspective and time |

Parent paths permit dot-separated descendants. A string prefix without a dot
boundary grants nothing: `bodyMarkdown.Safe` does not permit
`bodyMarkdown.Safe child`. Record scopes are capped at 4,096 IDs, with at most
4,096 field paths per owner. These are trusted metadata bounds, not disclosure
permissions derived from source.

## Admission and ordinary visibility

| Explicit admission | Possible result after ordinary filtering |
| --- | --- |
| `record` | Eligible ordinary body/title for one of the seven body kinds, followed by its admitted descriptions |
| `section` | Only the exact owner-scoped description ID; never owner body, owner title or sibling sections |
| `text` | The exact authored Markdown string, without a title or owner; the installed service separately applies its text policy |

The seven inherited body kinds are `character`, `location`, `object`, `event`,
`scene`, `environment` and `conversation`. A title-authorized eligible record can
produce a record unit with empty Markdown if none of its body sections is
permitted. Other canonical kinds can contribute explicitly admitted descriptions;
their raw bodies are excluded.

Ordinary owner audience/perspective restrictions remain effective. Lore concepts
and context briefs retain their native canonical status and validity checks;
existence episodes retain their own boundary and independently scoped character
endpoint. Description validity is independent of inherited body timing. For
example, a public early description on a future event can appear without that
event's title or body.

| Body kind | Ordinary story-time applicability |
| --- | --- |
| Character, location, object | Timeless body |
| Event | From the exact event clock onward on its timeline |
| Scene, conversation | Only a closed aggregate, from its exact end onward |
| Environment | Its authored start/end interval, inclusive; an absent end remains open |

Descriptions are filtered by ordinary field scope, audience, perspective and
time **before correction suppression**. Eligible replacements suppress their
earlier same-owner descriptions. Exact reveal section matching happens afterward.
Consequently, an admitted old description can disappear under an ordinary-visible
but unrevealed correction; the correction is not disclosed. A scope-denied
correction does not suppress an otherwise visible original.

### Body headings and the native subtree rule

Body projection retains the pinned heading/default audience model. It normalizes
CRLF/CR, splits the complete Python line-boundary set, and joins admitted trimmed
sections with blank lines. Explicit `section_audiences` lookup uses the exact
heading, then its ASCII slug after full Unicode case folding. Mapping keys are
not silently case-normalized. Private heading recognition retains the pinned
Unicode case-insensitive ASCII letter equivalents and whitespace behavior.

The deliberate native adaptation is conservative ancestry: an explicit ancestor
audience restriction or recognized private heading remains effective throughout
its descendants until an explicit nearer audience mapping overrides it. An
ordinary child heading alone cannot reopen a private subtree. If a child is
explicitly public, only that child's admitted section is emitted; private parent
heading/body text is not added. Unrelated top-level headings retain their normal
defaults. Invalid source mappings fail source validation rather than triggering
whole-body output.

### Native character lens

`Lens::Character` derives known owners through the existing native character
admission logic at the grant's clock. It intersects both the issuer grant and
the native public/character audience and ordinary/character perspective sets.
It emits admitted descriptions and explicit reveal text, with no inherited raw
bodies. Placement, POV, source links and reveal text do not teach the character.
This deliberately preserves native character evidence boundaries; it does not
claim upstream inherited character-body parity. The installed reader service's
public and author-simulation policies use the author-as-of public/ordinary lens.

## Minimal output and bounds

Every `ReaderItem` contains exactly `kind`, `id`, `ownerId`, `title` and
`markdown`. The title on a section is its own heading; text has no title. Output
is deduplicated by `(kind, ownerId, id)` in first-seen admission order. It carries
no private plan/work/chapter labels, manuscript prose, source links, paths,
fingerprints, blobs, backlinks or hidden counts. Explicit text preserves its
decoded string exactly, including Unicode, CRLF and trailing whitespace.

The pure compiler supports up to 4,096 complete occurrences within the private
source budgets. The installed presentation service separately caps complete
work occurrences and minimal items at 1,024, Markdown at 2 MiB, and its complete
response at 4 MiB. Installed text policy is applied before item bounds, search,
counts, digests and paging. The pure projector therefore does not prematurely
apply the service item cap to text that the installed policy will discard.

`tests/reveal.rs` covers occurrence reuse, exact text, scope intersection,
section-only privacy, corrections, seven-kind body eligibility, exact clocks,
Unicode heading restrictions, native learning, source binding and hidden-source
invariance. Service and transport tests exercise the installed boundary separately.
