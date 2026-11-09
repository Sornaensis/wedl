# Explicit consequence previews

A consequence plan describes exact record creates and updates. It does not
resolve an intention, infer an outcome, approve source, or publish files. The
native preview constructs a complete candidate in memory and returns its exact
source, diffs and validation. Both `writesPerformed` and `canApply` are always
`false`, including for a valid candidate.

Private human proposals retain these plans as immutable authored history. Their
lifecycle, answer snapshots and current-worktree checks are specified in
[ACTIONS.md](ACTIONS.md). The [worked proposal guide](../examples/actions/PROPOSALS.md)
uses the complete existing authoring world without changing its canonical source.

## Closed plan grammar

The JSON object requires `expectedFingerprint` and `operations`, with one optional
`analysis` object for explicit semantic checks and a same-horizon delta:

```json
{
  "expectedFingerprint": "<64 lowercase hexadecimal characters>",
  "operations": [
    {"type": "record.create", "draft": "kind: object\nid: obj_00000000000000000000000050\ntitle: Proposed object\n"},
    {"type": "record.update", "entityId": "char_00000000000000000000000001", "patch": "kind: character\n"}
  ]
}
```

`draft` and `patch` are **exact YAML strings**, not nested JSON objects. They use
the existing typed grammars in [AUTHORING.md](AUTHORING.md), including explicit
body replacement. Keeping them as strings preserves integer tokens, CRLF,
whitespace and literal reference-shaped prose through JSON/browser round trips.
Optional `analysis` retains an explicit horizon, required/advisory policy and up
to 100 expectations. It is included in the immutable plan and every complete
review hash. `null` is not an analysis; omit the field when no analysis is wanted.
See [SEMANTIC_CONSEQUENCES.md](SEMANTIC_CONSEQUENCES.md) for the closed predicates.

The existing typed codec determines the resulting canonical Markdown bytes;
the request string itself is not a source file.

Every update selects an exact record ID, including supported namespaced spatial
IDs. Titles and aliases are not update selectors. Each record may be written
only once in a plan. Creates require explicit stable IDs; no temporary IDs,
implicit aliases, deletion, merge, resolver policy or operation inferred from
prose is supported. Unknown or duplicate JSON/YAML keys are refused by their
respective decoders. Update identity, kind and source path remain fixed.

Plans contain 1–64 operations. An empty plan is invalid. An explicit no-op update,
such as the `kind: character` patch above, can represent a reviewed alternative
with no canonical change. No-ops still appear in the ordered operation and file
lists, with exact before/after source and hashes, `changed:false` and an empty
diff. They still require all bindings and complete bounds.

Historical plan validation checks the closed structure, identities and bounds.
It does not reinterpret retained YAML or resolve its references against a later
world. A newly appended proposal must additionally pass actual preview
construction against its current captured source.

## Construction and final validation

The supplied World must be valid and match `expectedFingerprint`. Every typed
operation is constructed against that **same captured base World**. The preview
collects all candidate records, checks identity and case-insensitive source-path
collisions, then validates the complete final world. Ordinary forward and
mutual references can therefore resolve across creates and updates, independent
of operation order. Reordering retains its own plan identity even when the
resulting source fingerprint is identical.

Construction still uses the existing typed editor. An operation needing a rich
profile binding or a target kind introduced by another operation may fail at
this boundary. Such failure identifies the zero-based operation index and the
construction error. No placeholder records, speculative retries or second
source grammar are used. Existing declarations and ordinary references do not
need that extra context; complete final validation checks every affected record.

A malformed request, unavailable exact update target, duplicate write, path
conflict, source mismatch or exhausted bound returns an error. A constructible
but invalid final candidate returns complete `validation` diagnostics with
`validation.valid:false`. It may be retained as an inert private proposal for
review. Validity alone never supplies human sufficiency or approval, and the
validator continues to disclose its implemented-coverage limits. Invalid
candidates report semantic analysis as unavailable and are never folded. A valid
candidate can still fail required expectations and remain an inert proposal.

The pure native preview reads only the supplied World. It makes no filesystem
or cache calls and can inspect a captured historical World. It does not claim
that the worktree still matches that capture. The saved-proposal wrapper
separately requires a current valid worktree, exact current canonical source,
exact stored private-file SHA-256, an active proposal, and authored/effective
phase `proposed`. It refuses stale or superseded selections before returning a
candidate. Previewing an unchanged saved proposal is independent of tab-local
unsaved edits; the browser refuses that ambiguity by requiring its draft to
match the saved document exactly.

## Complete report and hashes

The pure report includes `protocol`, `baseFingerprint`, `baseRevision`,
`candidateFingerprint`, `planSha256`, ordered `operations` and `files`,
`validation`, `changedFiles`, `canonicalNoOp`, `maxCharacters`,
`writesPerformed:false`, `canApply:false`, a read-only notice and `previewSha256`.
When analysis is selected, `semanticAnalysis` adds the complete same-horizon delta
and candidate expectation results. Source differences and semantic differences
remain separate; a nonempty source diff can have an empty semantic delta.

Each operation reports its zero-based index, type, exact entity ID, source path
and changed flag. Each file reports exact `beforeSource`/`afterSource`,
`beforeSha256`/`afterSha256` and a complete line `diff`. Creation has a null
before source and hash. No-op files remain present. All source strings preserve
their exact UTF-8 bytes, line endings and final-newline state.

Diffs remove a common full-line prefix and suffix and emit one replacement
hunk. They are deterministic and complete, without a minimal-edit guarantee.
Missing final newlines are marked. Header paths containing control characters,
quotes or backslashes use JSON string quoting, so valid unusual filenames
cannot introduce ambiguous header lines. The exact source strings and hashes
are authoritative.

| Digest | Exact input |
| --- | --- |
| File SHA-256 | Raw source bytes, with no prefix. |
| `planSha256` | `wedl-consequence-plan\0` followed by compact serialized Plan JSON, including operation order and raw YAML strings. |
| Pure `previewSha256` | `wedl-consequence-preview\0` followed by the complete compact report before adding its own digest field. |
| Saved proposal `proposalSha256` | Compact serialized complete Proposal JSON. |
| Saved wrapper `previewSha256` | `wedl-action-proposal-preview\0` followed by the complete compact wrapper before adding its own digest field. |

The preview hashes bind the selected budget as well as the results. The saved
wrapper also binds current source, action ID, exact private-file SHA, original
base, proposal/context identity, effective phase and nested consequence report.
Hashes are integrity bindings, not authenticated approval or proof of reading.

## Limits and interfaces

| Boundary | Complete limit |
| --- | --- |
| Plan JSON | 262,144 bytes, both raw decode input and compact encoded Plan. |
| JSON structure | Depth 32 and 16,384 visited values. |
| Operations | 1–64. |
| Each retained YAML input | Nonblank, at most 1 MiB; the smaller whole-plan limit also applies. |
| Expanded YAML before value allocation | At most 1 MiB UTF-8 strings, counting keys and each alias copy; 131,072 values/keys and depth 128. |
| Preview `maxCharacters` | 256–1,000,000 Unicode scalar values of the complete compact result, including hashes and diagnostics. |
| Combined before/after source and diff bytes | 8 MiB. |
| Complete encoded pure report | 8 MiB. |
| Saved-preview request | 16 KiB; its complete wrapper must also fit `maxCharacters`. |

An exceeded complete bound returns an error. Sources, history, diffs and
diagnostics are never silently truncated. The containing private Draft has its
own 262,144-byte limit, so a maximum standalone plan may not fit in a proposal.

```sh
wedl-rs --repo my-world consequences preview --file plan.json --max-characters 100000
wedl-rs --repo my-world action-drafts preview-proposal --file saved-preview.json
```

`--file -` accepts standard input. Standalone source/diff plan preview is a CLI/native/service
operation. Saved proposal preview uses the compact GET selectors in
[ACTIONS.md](ACTIONS.md). Separate impact and grouped-consequence GET previews
accept a complete explicit plan only within the existing 16 KiB query bound;
larger requests use exact downloads and the CLI. There is no consequence apply route or command.
Separate saved-outcome review, explicit approval, recoverable publication and
exact-source receipt verification are implemented in [OUTCOMES.md](OUTCOMES.md).
There is no standalone plan apply. Explicit expectation checks and same-horizon
deltas are implemented in [SEMANTIC_CONSEQUENCES.md](SEMANTIC_CONSEQUENCES.md).
[Retained edits](EDITS.md) build one explicit update through this same candidate
engine. [Three-way revision review](REVISION_REVIEW.md) compares independently
admitted original commits. [Event and grouped analysis](EVENT_CONSEQUENCES.md)
reuse the same candidate preparation, without extending this Plan grammar.
[Typed retcon impact](RETCON_IMPACT.md) and [saved review](RETCON_REVIEWS.md)
retain explicit dependency occurrences and original analysis independently of
publication approval. Candidate validation and required semantic expectations
remain the same gates across these consumers.
