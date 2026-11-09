# Social and language context

The readers compose explicit language, terminology and cultural records at one
exact story horizon. The author reader reuses admitted lore values, classified
assertions and dated descriptions. The character reader uses independently known
concepts, dated sections and explicitly selected learned words. Matching words
do not merge identities, and links do not
infer translation, membership, doctrine, proficiency or character knowledge.
All project labels remain v1.

## Ordinary authored source

The world must explicitly enable `lore-core-v1`, `descriptions-v1` and
`assertions-v1`. Existing current profile declarations supply these exact roles:

| Profile ID | Context role | Ordinary authored fields |
| --- | --- | --- |
| `social-language` | Language | Optional script references and note |
| `social-sense` | Sense | Language reference and literal gloss |
| `social-era-spelling` | Spelling | Language/sense references and literal form; optional scripts, register and unresolved null |
| `social-script` | Script | Authored note |
| `social-register` | Register | Language reference and literal label |
| `social-culture` | Culture | Optional language/religion references and note |
| `social-religion` | Religion | Optional culture/language references and note |
| `social-membership` | Membership | One explicit member/group/relation tuple |

The [social profile fragment](../examples/profiles/social-language-profiles.json)
supplies these ordinary declarations and the `social-doctrine` predicate. The
worked authoring sequence preserves existing profiles/predicates while adding
them through the existing writer. These are current lore profiles, with exact
string profile IDs and no separate profile versions. Generic profile validation checks every field's declared type
and every typed reference's existence/kind. It does not infer a social subtype
from an ordinary concept reference. A profile's familiar field names alone never
establish reference semantics.

Keep separate concepts for languages, senses and spellings. Two languages may
have spellings with the same literal `form` and different sense glosses. Give
older/current/later spellings independent source IDs and explicit inclusive
`valid` intervals. Overlapping forms remain independent alternatives; there is
no preferred spelling or calendar/era conversion.

A membership record describes its explicitly authored tuple. Cultural membership
does not create religious membership, language use or an inverse membership.
Timed `concept_links` retain their ordinary authored relation and applicability;
a `schism-from` relation is not interpreted as a computed historical conclusion.

Use ordinary assertions for doctrine and individually attributed accounts, with
the author's chosen predicate and classification. Use explicit descriptions for
dated diaspora, migration or exception wording. Ordinary record body text remains
opaque, undated authored prose. The reader neither extracts facts from it nor
relabels it as a dated section.

## Selection and reads

```sh
wedl-rs --repo my-world social context LANGUAGE_ID --tick 3 --order 0
wedl-rs --repo my-world social context LANGUAGE_ID --sense SENSE_ID --form bank --context CULTURE_ID --context RELIGION_ID --tick 3 --order 0
```

Substitute exact concept IDs for the uppercase placeholders. Titles and aliases
are not selectors. The horizon is required; order defaults to the end of the
chosen tick and timeline defaults to the world default. Audience and view
default to `author` and retain their ordinary literal presentation-label meaning.
This author operation does not supply an all-time mode. Character reads use the
separate operations below.

The closed GET route is `/api/social/context`, with required `language` and
`tick`; optional `sense`, `form`, `contexts`, `order`, `timeline`, `audience` and
`view`. `contexts` is a JSON array of exact IDs, not a comma-separated string.
The browser exposes the same selections. CLI `--context` can be repeated.

After the worked authoring sequence, this concrete query selects River language,
its river-bank sense, the culture, religion and one explicit membership:

```sh
wedl-rs --repo my-world social context concept_00000000000000000000000013 --sense concept_00000000000000000000000016 --context concept_00000000000000000000000023 --context concept_00000000000000000000000024 --context concept_00000000000000000000000025 --tick 3 --order 0
```

It returns nine admitted records, including both `bank` and the overlapping
`banque`, with separate dated sections and classified doctrine accounts. Selecting
Hill language (concept 14) and its sense (concept 17) with form `bank` retains a
different authored sense and spelling identity.

Selection proceeds through the shared admission gate before context records or
bounds are counted:

1. The language must be an admitted `social-language` concept. An optional exact
   sense must be an admitted `social-sense` with a declared entity reference to
   that language.
2. Up to eight distinct context roots must be admitted cultures, religions or
   membership concepts. Their cultural links remain in the native items; these
   roots do not trigger a global social traversal.
3. Admitted senses, spellings and registers are selected by their explicit typed
   language reference. Supplying a sense restricts sense/spelling identity;
   the language's registers remain separate selected records.
4. A spelling also requires an admitted matching sense/language association and
   an authored string form. `--form` filters spelling rows only, using exact
   literal text. It performs no case folding, Unicode normalization or alias
   expansion. Omission means no form filter; an explicit empty string matches
   only an authored empty form. Numeric wire strings cannot become word forms.
5. Selected language and spelling records contribute only their explicitly
   typed script arrays and register reference. These associations are one step;
   scripts/registers do not create additional traversal roots.

Missing projected incidental references are skipped without exposing hidden
endpoints. Explicit required roots that are missing, hidden, outside their
interval, noncanonical or of the wrong role share one unavailable error. A
visible selected association with an incompatible role or conflicting
sense/language identity also refuses the complete context. Ref-shaped literal
objects remain values and are never followed as relationships.

## Separate evidence and citations

The result retains the protocol, author perspective, exact `at`, audience, view,
selection and `basis: authored-social-language-context`. `records` contains
`{role,item}` rows ordered by concept ID, and `count` counts those rows. Each
`item` is the complete existing admitted lore representation: exact values, used
type shapes, body, validity, concept links and original source/reference citations.
Filtering does not renumber original array reference paths.

`canon` and `sections` contain `{ownerId,result}` entries with the complete
existing assertion or description response. Evidence owners are the selected
language, the explicitly requested sense, and the explicit context roots, in
that order. Automatically discovered senses, spellings, registers and scripts
do not become extra evidence owners.

These independent streams keep native admission, corrections and citations.
Two attributed accounts do not vote to establish truth; unresolved accounts
remain unresolved. Dated description corrections retain their stable local
targets and source positions. Empty evidence results mean no matching admitted
account was supplied; they do not establish absence or falsehood. Historical
assertion evidence remains syntax-only source, so the native result continues
to emit `evidence: []` and cite actual current source.

The reader uses one already loaded immutable `World`. Current source and existing
Git snapshot loading use the same context function; it adds no second revision
protocol, cursor system, cache, writer or retry framework.

## Completion bounds and remaining work

The reader accepts at most eight distinct context roots and 4,096 UTF-8 bytes in
the optional literal form. It admits at most 32 selected records, ten explicit
evidence owners, and 512 combined assertion/description items. The complete
compact JSON encoding, including one newline, must fit 262,144 bytes. Pretty CLI
formatting can be larger; the bound applies to the shared compact representation.
An exceeded bound refuses the entire report instead of clipping source or
returning a partial context. These are complete-result guards, not a claim of
bounded total source work or indexed performance.

Ordinary reviewed world profile/predicate replacement, concept edits, assertion
edits and description-array replacement remain the source write paths. Complete
candidate validation preserves unrelated metadata and body bytes. Current
local-author sessions support [reviewed browser publication](BROWSER_PUBLICATION.md)
and capture fresh source for each canonical request. Historical sessions remain
fixed and GET-only.

## Character language and culture

```sh
wedl-rs --repo my-world character social CHARACTER_ID LANGUAGE_ID --sense SENSE_ID --context CULTURE_ID --belief-target ASSERTION_ID --tick 12 --order 0
```

The native operation is
`social::character_context(world, viewer, selection, belief_targets, at)`.
The closed GET route is `/api/character/social`, with `character`, `language`
and `tick`; optional `sense`, `form`, `contexts`, `beliefTargets`, `order` and
`timeline`. The two lists are JSON arrays. This route accepts no author audience,
view, scene or arbitrary known-ID grants. The existing three capabilities remain
required. The browser provides separate author and character controls.

The shared [character admission](NARRATIVE_ADMISSION.md) scope is computed from
the captured world, viewer and exact horizon. Every root and traversed endpoint
must already be independently known and pass its own labels, canonical status
and validity. Selecting a language, sense, culture or learned assertion target
does not teach it. A known language's typed script link does not make an unknown
script visible. Admission occurs before language matching, spelling/form
selection, reference traversal, sorting and bounds. This prevents private or
future records from changing visible counts or consuming a visible-result limit.

Character `records` retain role, exact concept identity, admitted values, used
types, validity and safe typed/concept-link targets. They omit current author
titles, untimed bodies, provenance, source paths, source array positions and
citations. Temporary traversal paths are derived from the already projected
values and removed before publication. Adding an inaccessible array member
therefore leaves the complete character result unchanged. Literal Ref-shaped
objects remain literal values and do not grant the identities written in them.

`sections` still contains native `{ownerId,result}` streams for only the selected
language, explicit sense and explicit cultural roots. Each section follows its
own scope, exact validity and native correction rules. Automatically discovered
spellings, scripts and registers do not become section owners. Empty section
streams make no assertion about the world's facts.

`beliefTargets` defaults to empty and permits up to eight distinct exact
assertion or knowledge IDs. Each requested target contributes a separate
`beliefs` row `{targetId,result}`. The native result contains only the viewer's
currently held literal linked claim, selected transition, belief state, numeric
confidence and exact transition time. An unlearned, forgotten or unknown target
produces an empty native item list. It supplies no canonical assertion title,
subject, value, classification or correction result. Beliefs are never assigned
to the selected language or culture by reading canonical subject metadata.
The same explicit learned claim may disagree with current author truth without
either replacing the other.

The character response carries `perspective: character`,
`scope: character-as-of-social-context`, `characterId`, exact `effectiveTime`,
the selection, records, beliefs, sections and `complete: true`. It has no `canon`
stream or snapshot metadata. Its 512-item evidence bound counts admitted dated
sections and emitted selected learned rows together; the existing 32-record and
262,144-byte complete-report bounds also apply. Limit failures return no partial
packet.

Exact [writing frames](WRITING_FRAMES.md) can explicitly include this family
using the same selections and the frame's admitted scene scope. The six-field
frame stays unchanged. Ordinary character context/search still does not import
typed family concepts automatically. Explicit [semantic/hybrid retrieval](RETRIEVAL.md)
uses that same search corpus. Actionable captures and manuscript/reader workflows
remain later work.

## Pinned evidence

The author and explicit character-selection behavior was checked against commit
`f0e885ccebc279eff65a5db2984d01ebc6647b2b`, especially
[social_context.py](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/src/wedl/social_context.py),
[the social context guide](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/docs/guides/social-context.md)
and [the ordinary profile pack](https://github.com/Sornaensis/wedl/blob/f0e885ccebc279eff65a5db2984d01ebc6647b2b/docs/examples/social-language-profiles.json).
Rust retains current profile definitions, source-backed complete reads and the
existing integer codec. Character belief targets follow the pinned explicit
target selection and learned-word projection. It checks declared/used reference types rather than
following literal objects, and skips inaccessible incidental endpoints before
selection. `tests/social.rs` covers author selection, projection, exact time/form
boundaries, evidence preservation and complete limits. `tests/family_character.rs`
and `tests/writing_family_context.rs` add character admission, independent beliefs,
private-change invariance, scene scope and separated frame components.
