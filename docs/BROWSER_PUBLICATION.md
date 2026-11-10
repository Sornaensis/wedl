# Reviewed browser publication

The local author browser can save private actions, retained edits, manuscripts
and retcon reviews, and apply explicitly approved saved action/edit candidates.
It uses the existing native request formats, review admission and writer. All
source, API and private-document labels remain v1.

## Author workflow

Start `wedl-rs --repo WORLD serve --live-author` and open the printed loopback URL.
The header reports whether publication is available. Plain `serve` provides
read-only worktree browsing. A server opened with `--revision` serves an immutable
read-only snapshot even with `--live-author`. Downloads remain available in all modes.

Open **Author Workbench** at `/workbench` for named Writing choices, private
drafts, structured reviews, and the Publication receipt. **Native author tools**
at `/legacy-author` remain available for exact file workflows. The Workbench
opens its own writing context and does not move the Compendium's reading position.

1. In **Actions**, create or import a draft and choose **Save private draft**.
   List and load the saved draft before continuing. Resolution-needed saves and
   appended proposals still require a fresh complete context review and explicit
   sufficiency assessment. This assessment does not approve publication.
2. Build or import the explicit Plan, append its proposal, review the completed
   private draft and save it. List and load that exact saved proposed action.
3. Choose **Review saved outcome** and inspect the complete candidate, source
   changes, validation and any semantic expectations. Check the separate approval
   box, then choose **Apply approved outcome**.
4. The source refreshes after success. Inspect the exact result in **Publication**.
   A confirmed write stays confirmed if refresh fails. For an unconfirmed result,
   refresh the source and inspect the receipt before another submission.

**Edits** follows the shorter save → load → review → explicit approval → apply
path for one retained patch/body. Unfinished patches may be saved privately;
only a valid exact reviewed candidate can be applied. **Manuscripts** and
**Retcons** offer private saves with their existing source/private-file pins.
Reveal-plan source uses the same manuscript save route.

Saves never silently replace a tab's buffer or change its original pins. This
also holds when the author types or changes tabs while a write is pending. Load
the saved document explicitly to continue from its new hash. **Refresh source**
is available in both worktree modes. It invalidates cached results and approvals
while retaining local inputs, selected story coordinates and original pins.
Capturing new pins is a separate action.

## HTTP contract

`http::router(Service)` retains its immutable GET-only contract.
Current-worktree `serve` uses the same fenced read capture as `serve --live-author`,
with only the read routes registered. Current-worktree `serve --live-author` uses
`http::live_router(Service)`. Historical input uses the immutable router.
`GET /api/session` returns capabilities in the ordinary `wedl-rust/v1` envelope.
Plain worktree mode reports `mode: "read-only-author"` and `writesEnabled: false`.
The live router reports `mode: "live-author"` and `writesEnabled: true`.

| Exact POST route | Existing native request | Maximum body bytes |
| --- | --- | ---: |
| `/api/action-drafts/save` | Action private save | 524,288 |
| `/api/action-drafts/apply-outcome` | Approved saved outcome | 16,384 |
| `/api/edit-drafts/save` | Retained edit private save | 524,288 |
| `/api/edit-drafts/apply` | Approved saved edit | 16,384 |
| `/api/manuscripts/save` | Exact manuscript source save | 8,388,608 |
| `/api/retcon-reviews/save` | Private review save | 4,194,304 |

Bodies are the unchanged closed native JSON requests, decoded as strict UTF-8.
Duplicate/unknown fields and native structural limits remain enforced. Write
URLs accept no query, including an empty query suffix. There is no raw Plan
application route or HTTP source/body patch bypass.

The Workbench requests a typed receipt using
`Accept: application/vnd.wedl.workbench+json` on those same routes. The exact
native request and all writer checks still apply. Other clients receive the
original `wedl-rust/v1` response envelope.

Each POST requires one canonical loopback `Host`, one exact matching
`Origin: http://HOST`, one JSON `Content-Type` and `X-WEDL-Write: 1`. Missing,
duplicate, null or foreign origins and malformed authorities are refused.
No CORS permission is granted. These guards protect the local browser boundary;
the header is not an approval token. Publication approval remains the native
exact review digest and explicit `approved: true`.

One live execution gate serializes source capture and operation dispatch.
Canonical operations capture a fresh immutable source using the existing bounded
current loader: at most 4,096 records, 8 MiB per file and 32 MiB total, plus
existing YAML bounds and before/after file-image and publication fences. Capture
failure never substitutes an old canonical snapshot. Native writers retain their
own cross-process locks, repeated pins and recoverable publication rules.

Independent manuscript/reveal, revision and retcon reads retain their original
root/source selectors and remain available independently of malformed live canon
when their own contracts allow it. Their live responses use `protocol` and
`result`, without misleading outer canonical fingerprint/revision fields; their
result-level source pins retain their existing meaning. Reader preview retains
the same installed presentation service and its emission-time policy fence.

Write responses retain the native Service envelope. Its outer fingerprint is
the snapshot **before dispatch**, not a claim about the post-write source. The
browser fetches fresh status separately. Private saves may leave the canonical
fingerprint unchanged; a canonical no-op application can still create a terminal
private receipt. Exact approved application retries are decided by the existing
receipt verifier, without a generic pre-dispatch fingerprint rejection.

## Failure and exactness

The browser permits one submitted write at a time. A write acknowledgement is
retained across tab changes. If the write succeeds but refresh fails, the UI
reports success and the separate refresh failure. The typed response reports
`refused` only when the request was stopped before source capture and native
dispatch. Capture or native writer errors, including conflicts, can leave the
operation `unconfirmed`. A lost or undecodable acknowledgement has the same
effect. Refresh source and inspect the saved document or receipt before
resubmitting. The exact last submitted request remains downloadable in the tab.

Source refresh clears approvals, invalidates open Writing and Plan results, and
disables further writes until it succeeds. The exact Writing frame pins stay
available for revalidation.
Imported exact prose and line endings remain unchanged until explicitly edited.
The existing 16 KiB browser URL bound still applies to GET review/preview input;
larger complete requests can be downloaded for their native CLI operation.
See [ACTIONS.md](ACTIONS.md), [OUTCOMES.md](OUTCOMES.md), [EDITS.md](EDITS.md),
[MANUSCRIPT.md](MANUSCRIPT.md) and [RETCON_REVIEWS.md](RETCON_REVIEWS.md).
