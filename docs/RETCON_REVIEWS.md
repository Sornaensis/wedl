# Saved native retcon reviews — v1

Saved retcon reviews retain an original dependency analysis, ordered human dispositions, and exact commentary. Their private JSON lives at `author/retcon-reviews/review_<26 Crockford characters>.json`. Saving a review changes that private file only. Review decisions do not approve, authorize, or apply a consequence plan, and the native writer does not create a Git commit.

The implementation uses the actual pinned upstream review lifecycle as its behavioral reference while retaining native consequence plans, JSON storage, the existing writer lock, and source/private compare-and-swap pins. It does not add the upstream Git receipt publisher or reproduce receipt-first retries across later heads. Every native request, document, witness, and protocol remains on v1; no migration or alias layer is provided.

## Capture and retain an original

`retcon-reviews preview --file preview.json` takes a closed JSON request:

```json
{
  "baseRevision": "<full lowercase commit OID>",
  "plan": {
    "expectedFingerprint": "<original source SHA-256>",
    "operations": [
      {
        "type": "record.update",
        "entityId": "loc_00000000000000000000000001",
        "patch": "kind: location\nbody: Revised landing.\n"
      }
    ]
  },
  "impactRequest": {
    "target": {"kind": "record", "id": "loc_00000000000000000000000001"},
    "at": {"timeline": "main", "tick": "9007199254740993", "order": "1"},
    "depth": 8,
    "maxItems": 1000
  }
}
```

The base must be the exact captured live `HEAD`, expressed as a literal full 40- or 64-character lowercase commit OID. The complete current canonical source fingerprint must match the original committed source and the plan's expected fingerprint. The current manuscript/work source must likewise match its committed source at that base. Uncommitted changes in either source set therefore require reconciliation before capturing a new review. Private review commentary and other author subtrees do not enter either source fingerprint. These whole-source checks fence the original capture; they do not decide later review currentness.

The canonical original and the manuscript/work source are loaded from verified Git commit/tree/blob bytes at the same base. Missing manuscript directories mean an empty private snapshot. The native candidate builder reconstructs the complete supplied consequence plan against that base; the private source remains unchanged. Source validation and any explicitly required consequence semantic checks must succeed. The retcon engine then computes admitted source dependency occurrences and a witness. The response has `analysis`, `revision`, `comparedRevision`, `comparedFingerprint`, `writesPerformed: false`, and `canApply: false`. Preview allocates no review ID and writes no file, cache, or Git object.

`analysis` contains exactly:

| Field | Retained meaning |
| --- | --- |
| `worldId` | Original canonical world identity |
| `baseRevision` | Original committed source OID |
| `plan` | Exact native consequence plan, including retained YAML strings and optional explicit semantic checks |
| `impactRequest` | Original typed target, decimal-string time, traversal depth, and item bound |
| `planSha256` | Native consequence plan digest |
| `report` / `reportSha256` | Complete original impact report and its domain-separated canonical JSON digest |
| `itemDigests` | Exact ordered report-item digests |
| `witness` / `witnessSha256` | Complete admitted-input witness and its domain-separated canonical JSON digest |

Base report citations retain their original revision and blob identity. Canonical candidate citations identify their original base and plan digest. Unchanged manuscript/work citations retain their original source revision and blob identity in both report snapshots. Current comparison metadata is separate from this immutable analysis.

## Save commentary with two independent pins

Copy the generated analysis into a new document, choose an exact `review_` ID, and provide title, decisions, and body:

```json
{
  "expectedFingerprint": "<current canonical source SHA-256>",
  "expectedSha256": null,
  "document": {
    "id": "review_00000000000000000000000001",
    "title": "Landing dependency review",
    "analysis": "<the complete generated analysis object belongs here>",
    "decisions": [],
    "bodyMarkdown": "Private notes.\r\nTrailing space  "
  }
}
```

The placeholder above represents the complete original JSON object, not a string accepted by the codec. Use the generated object directly; rewriting hashes is not a way to authenticate a different analysis.

`retcon-reviews preview-save --file save.json` reconstructs and validates the complete original, checks both pins and private-store quotas, and returns exact `beforeSource`, `afterSource`, `beforeSha256`, `afterSha256`, `sourceSha256`, `changed`, and `canSave: true`. It performs no writes. `retcon-reviews save --file save.json` performs the same admission, acquires the existing native writer lock, repeats admission, and atomically replaces the private file after final source/file checks.

The save source pin describes **current** canonical source. It is independent of `analysis.baseRevision` and the original plan fingerprint. A stale review can therefore receive notes after unrelated or relevant later source changes, provided its original analysis can still be reconstructed and the save pins describe the current source and private bytes.

For a new document, `expectedSha256` must explicitly be `null` and the private path must not exist. For an update, it must be the exact SHA-256 of the current private source bytes. A same-content retry is a no-op only when that current private pin is supplied; a retry with an obsolete or null prior pin fails. The writer returns `gitCommit: null` and does not make automatic commits. Native writers share an advisory lock; external editors must coordinate with that lock.

Current local-author sessions accept the same save request at POST `/api/retcon-reviews/save`. The browser can save the private review or download its request; this does not authorize canonical application. Historical selections remain read-only, and unsaved review input retains its pins until explicit capture or reload. See [browser publication](BROWSER_PUBLICATION.md) for the write boundary.

The whole original `analysis` is immutable for an existing ID. Changing the base, plan, target, report, or witness requires a new review ID. Title, dispositions, notes, and `bodyMarkdown` remain editable. The body preserves CRLF, Unicode, trailing whitespace, and the absence of a final newline. JSON escaping is storage syntax and does not normalize the decoded body.

Each decision has exactly `itemDigest`, `disposition`, and `note`. Allowed dispositions are `reviewed`, `needs-revision`, and `intentionally-retained`. Decision item digests must form a unique ordered subsequence of the original `itemDigests`; omitted items remain pending. Decisions do not remove items or alter original findings. `pendingCount` is the number of original items without a decision.

## Queue and detail are independent reads

Queue accepts `{"revision":null,"maxItems":1000}`. Detail accepts `{"revision":null,"id":"review_00000000000000000000000001","expectedSha256":null}`. Optional pins are explicit `null|string` fields; omission, duplicate fields, unknown fields, symbolic revisions, and malformed IDs are refused.

A null revision selects current private bytes. A literal revision selects exact historical private bytes from that commit. Historical selection never falls back to current private files. A historical commit without a review directory has an empty queue; requesting a missing detail document fails. Historical source/blob identities remain literal and verified, including in repositories with Git replacement refs.

Both responses expose `revision` for selected private history and independent `comparedRevision` / `comparedFingerprint` for current comparison. Queue items expose `id`, `title`, `path`, `sourceSha256`, `blobOid`, `currentness`, and `pendingCount`. Detail adds the complete original `document` and exact private `source` string. Live private files have null `blobOid`; historical files retain their actual original blob OID.

Every queue/detail operation captures live HEAD once and retains one bounded canonical/manuscript comparison image. Verification rereads fence that image against observed source drift; their values do not replace the retained inputs. All items share these comparison pins. Read operations create no locks, receipts, cache, directories, or updated review files. A complete queue exceeding `maxItems` fails rather than returning a partial queue.

## Currentness follows admitted inputs and retained history

`currentness` has `status`, `code`, `comparedRevision`, and `comparedFingerprint`. Its statuses are:

- `current`, with a null code: the complete comparison was available, every original binding verified, and all admitted witnesses were equal.
- `stale`, with `REVIEW-STALE-001`: at least one relevant admitted witness changed in the current worktree or the complete committed first-parent history.
- `unknown`, with `REVIEW-UNKNOWN-001`: original or comparison inputs, a supported witness reconstruction, or the complete bounded history were unavailable.

For supported witnesses, the reader first rebuilds the original literal committed base and the exact retained plan. It checks the world, plan, report, ordered items, and witness against that capture. Self-consistent client hashes alone cannot establish these bindings. Forged reports and impossible plans are binding errors when the valid original is available. Missing or unusable original source prevents comparison and yields unknown. Unsupported witness versions are retained opaquely within the JSON limits and produce unknown; the other original fields still reconstruct and verify. Such documents cannot be saved as a supported new capture.

The comparison establishes the **complete** original first-parent chain from base to captured live HEAD before trusting any prefix. At most 256 commits after the base are allowed. Raw verified commit parents determine ancestry; a base appearing only through a merge's second parent does not qualify. Missing objects, an overlong/incomplete chain, invalid original images, or any failed witness reconstruction make the result unknown, even if another image looked stale.

All complete intervening committed images and the captured current worktree are compared. Each image pairs canonical and manuscript/work source from its exact revision; historical source never falls back to current private files. A relevant committed canonical or manuscript change followed by a revert stays stale. Native writers do not automatically commit, so the current worktree is also essential: a relevant uncommitted application or private edit stales the review immediately. Uncommitted changes have no retained history; changing and restoring bytes before comparison cannot be detected as a past event.

Witnesses retain original seeds and path hints, then rebuild incoming dependency closure from each comparison source. Newly incoming canonical references, segment source links or POV links, and work occurrences are therefore relevant even when every originally selected record is unchanged. Comparisons use admitted inputs, rather than whole canonical or author-source fingerprints. Unrelated records or private documents, excluded audiences/perspectives, future source, and excluded positions do not stale an otherwise equal witness. An admitted selected manuscript/work contributes its exact ordered frontmatter values and opaque body; prose mentions never create dependencies.

Every comparison private snapshot must be structurally valid and within bounds. New or changed private documents must resolve their references against that comparison image. Exact unchanged private source bytes and paths from the verified original are retained as evidence even if a later retcon removes their canonical target or segment. Those unavailable edges disappear, making the witness stale. Editing such a dangling private document requires valid new links; otherwise currentness is unknown. This exception authenticates original evidence and does not reinterpret links at live HEAD. Other invalid or unavailable comparison inputs produce unknown while preserving readable private originals.

`wedl-retcon-witness/v1` records admitted input refs/paths/presence, ordered sections, source dependency edges, and consulted declarations. Section and declaration values are canonical JSON **strings** under `valueRaw`. This preserves signed/unsigned integer values and integer-versus-float source representation through browsers without JavaScript numeric rounding. Times use canonical decimal strings. The witness carries no live revision, whole-source fingerprint, or current blob hash; those identities belong to separate metadata.

## Bounds and source safety

All bounds are complete-result bounds. There are no silent truncations, fallback current paths for history, or partial reviews.

| Surface | Limit |
| --- | --- |
| Preview request | 512 KiB; native plan at most 256 KiB and 64 operations |
| Save request | 4 MiB |
| Stored document | 2 MiB |
| Title / body / each note | 1 KiB / 128 KiB / 16 KiB UTF-8 bytes |
| Request JSON | Depth 64, 131,072 nodes, scalar bytes bounded by its raw request limit |
| Impact report / witness | 256 KiB / 1 MiB |
| Witness positions | 4,096 combined inputs, sections, edges, and consulted positions |
| Impact request | Depth 1–32; maximum items 1–1,000 |
| Private store | 1,024 documents, 16 MiB, 2,048 directory entries including bounded regular native temporary files |
| Queue/detail response | 8 MiB |
| Original first-parent history | 256 commits after base; 16 MiB commit bytes |
| Canonical capture | 4,096 records, 8 MiB per file, 32 MiB total source, 8,192 walked entries/tree objects |
| Manuscript/work comparison source | 1,024 documents, 16 MiB total, 2 MiB per document; all manuscript structural bounds apply |

JSON duplicate keys are rejected before they can collapse into a map, and structural budgets are charged before value nodes are allocated. The shared source preflight bounds expanded YAML before constructing native records. Retcon admission also applies its complete projected source and validation-work bounds.

Private and canonical source captures reject symlinks and unsafe paths, use bounded regular-file reads, verify exact source identity, and reject observed concurrent drift. Current canonical capture checks the native publication fence before and after reading. Pending publication or invalid current source makes read comparison unknown and blocks saving. Historical originals remain accessible independently when their literal Git inputs are available.

## Validation evidence

`tests/retcon_reviews.rs` exercises the actual native lifecycle and exact pins, pure previews, source/private CAS, immutable analysis, ordered decisions, exact body bytes, original reconstruction despite rehashed forgeries, large numeric witness strings, historical private reads under broken live source, unrelated versus newly incoming canonical and private changes, committed change/revert, unavailable and non-first-parent history, the complete history bound, symlink and source-size refusal, pending publication, and replacement-ref resistance. Manuscript checks also cover exact source provenance on candidate rows, historical reconstruction, placement admission, source-capture fences, and the unchanged-original dangling-link exception. Shared transport/browser interfaces retain these native envelopes and save contracts.
