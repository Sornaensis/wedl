+++
schema = "adrai/decision/v1"
adr = "A01M48VQ7R714VYPET6C6DSHN2Z"
record = "R01M48VQ7YGJF06SE80F182WBAV"
title = "Reader admission for event consequence reports"
summary = "Record revision-bound report admission, public source-blob enrichment and bounded asynchronous reader behavior."
domains = ["reader"]
+++

# Reader admission for event consequence reports

## Authority and provenance

Migrated from `docs/EVENT_CONSEQUENCES_READER.md` at WEDL source revision
`4a06930096b2d46c1c0311e040dfb9fde218e634`, with existing request/enrichment and
asynchronous behavior read in `static/query.mjs`, `static/app.js`, `static/api.js`
and `static/lore.mjs`. This records the current reader contract without claiming
a new approval. The shared consequence grammar, scope, folds and outcomes are
defined by ADRAI stable ID `A01M48VHYA0RGNJWBFZJZMGVZX6`.

## Report request and presentation

The author lens requests the revision-pinned `wedl-event-consequences/v1` report
for the selected event, timeline and inclusive reading horizon. Full story uses
the terminal supported signed tick/order boundary of that selected timeline.
It supplies no scene-cursor/current-time inference and does not combine timelines.
The presentation lens is local state and is not sent as an authorization grant;
character views do not request the author consequence report.

The request accepts an exact revision, nonempty event reference, matching
timeline and horizon, and an integer limit of 1..1000. The current default is
1000. The reader does not submit expectation checks or write consequences.
It accepts successful reports only when protocol, revision, event identity/kind,
author-as-of scope and both returned horizon copies match the captured request.
Success requires empty expectations, `applyAllowed:true`, the seven report
collections as arrays, total collection length within the request limit and a
serialized payload no larger than 262144 bytes. Closed invalid/unavailable/limit
reports have only protocol, outcome, code and message.

The article retains its prose and direct authored effects while report loading
or failure occurs. Report presentation keeps event-local changes, explicitly
caused transitions, outcome links, later causal events and current-at-horizon
contributions distinct. Superseded accepted beliefs remain recorded history;
a later authored rejection may replace their current contribution. Empty
sections state only that the report contains no recorded links, never story
completeness, truth verification or an applied write.

## Authenticated admission and public source detail

The report endpoint uses the local author session. Optional labels for an
admitted knowledge or relationship record use the existing public source-detail
endpoint `/api/entities/{id}`. That endpoint remains a public read even when
a session token is sent. Its matching source blob proves content identity;
the authenticated report supplies admission. The detail fetch does not establish
or widen the report's authorized record scope.

Collect enrichment candidates only from the report's admitted knowledge and
relationship caused-transition entries. Require the citation's record identity
to match that record, source provenance at the captured report revision and a
full source blob OID. Conflicting kind/blob proofs exclude that record.
Read at most 24 distinct admitted records with four requests in flight.

Accept a detail only with the admitted record identity, kind and source blob.
If revision, perspective, effectiveTime or timeScope metadata is present,
reject conflict with the captured revision, author perspective and exact report
horizon. Copy only the declared knowledge knower/claim statement or relationship
from/to endpoints. Never consume its temporal fields or other source content.
Missing proof or failed detail reads retain the report's recorded name.
No source scan, title inference or guessed direction supplies enrichment.

## Asynchronous context and bounds

Capture the revision, event, timeline, horizon and limit in the request key.
Accept a response only while its navigation generation, detail request, selected
article and current revision/request key still match, and its abort signal remains
live. Abort a previous consequence controller before starting a replacement.
Check this context again after enrichment, and exclude stale responses from
rendering and caching. Detail workers stop when that context is superseded.
Browser Back/Forward retain the reading context.

The current implementation stores at most 64 completed report/enrichment entries
keyed by the exact captured request. Revision refresh aborts pending work and
clears this cache. This bounded cache is presentation state, not a new authority,
report protocol or source index.

Loading/failure status uses a polite live region and the report section's busy
state. Unavailable, invalid, limited and transport-error reports retain the
article and direct effects. Retrying by reopening the entry or choosing a reading
horizon captures a fresh request. Automatic revision refresh uses the server
runtime's WebSocket support; explicit page refresh reloads current source when
that channel is unavailable.

## Existing conformance

The existing `tests/web_ui.test.mjs` and `tests/web_ui_dom.test.mjs` cover request
matching, source-blob admission, conflicting context, bounded enrichment, safe
labels/direction, stale responses and article/history behavior. The procedural
walkthrough is [the reader guide](../../../../docs/guides/event-consequences-reader.md).

<!-- @adrai:eyJhIjp7ImkiOiJkb2NzLWNsZWFudXAtZGV2ZWxvcGVyIiwiayI6ImxsbSIsIm0iOiJncHQtNi4xLXNvbCJ9LCJiIjoiNDU4MzVhOWI3NDM5NTgyMDBkMmQzZDQ4MGU3MTBkYTRjOGQzZmYzOSIsImkiOiJzaGEyNTY6WjYxbEZFU2t2Y3VsSjB6TlZxeUlkTEtfWmljS2pxV2J5ZFI5SVBDOVdzWSIsImsiOiJkZWNpc2lvbi5jcmVhdGUiLCJvIjoiUjAxTTQ4VlE3WUdKRjA2U0U4MEYxODJXQkFWIiwib3AiOiJPMDFNNDhWUTdZR0pGMDZTRTgwRjE4MldCQVYiLCJyIjoibWFzdGVyIiwicyI6InNoYTI1NjpxQWpLeTVlN2FDOHEzUzZYdG94Z2hxcEg2bUMyS3k1OW51bDROZEFEaThnIiwidCI6MTc5MTI5ODg3MTI0OCwidiI6MSwieCI6ImFkcmFpLzEuMC4wIn0 -->
