# Read recorded event consequences

1. Open an event in the lore reader and select the author lens.
2. Choose the timeline and an author reading horizon. Choose Full story to read
   through the end of that timeline's supported range.
3. Read the Explicit event consequences section alongside the event's article.
   Follow named links to inspect the recorded lore entries.

The sections show changes around the event, transitions recorded with that event
as their cause, outcome links, later causal events and the contribution still
current at the selected horizon. A previously accepted belief can remain visible
in its history after later evidence rejects it. Empty sections mean that no links
were recorded in that report; use the underlying records when investigating gaps.

Loading and failure messages appear in the report's status region. If a report is
unavailable, invalid or too large, keep reading the article and its direct effects.
Reopen the entry or choose another reading horizon to retry. Browser Back and
Forward preserve the reading context.

Refresh the page to load current source if automatic revision refresh is
unavailable. Automatic refresh requires the server runtime's WebSocket support.
To author a new consequence, use the [preview walkthrough](event-consequence-preview.md).

Read the reader admission contract in the WEDL development/source checkout:
`adrai --repo WEDL_SOURCE_CHECKOUT search 'Reader admission for event consequence reports' --mode fts --json`.
