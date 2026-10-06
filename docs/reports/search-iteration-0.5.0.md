# 0.5.0 Search Iteration Notes

> Historical report. Original versions, fixture counts, outcomes and measurements below belong to the recorded exercise. This migration performs no fresh release/performance qualification. Architectural rationale is in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; custody is in ADRAI A01M49735W4D3CZ2PJ129HTJCGG. Read these with `adrai --repo WEDL_SOURCE_CHECKOUT show ADR_ID --json` in the WEDL development/source checkout.

## Baseline problem

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.


## Changes made

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.


## Interactive observations

### Lexical query

For `counterseal fragment`, FTS and vector both ranked **Archive Counterseal
Fragment** first. Hybrid strengthened records supported by both lanes and kept
FTS snippets.

### Semantic query

For `legal authenticity coercion`, FTS found four records with direct lexical
support. The vector lane recovered twenty records, headed by **The Notary at the
Seventh Drawer**, **Ysabet Crane**, **The Seventh Drawer**, and **Council
Inventory Writ**. Hybrid retained sixteen vector-only results and three results
supported by both lanes.

### Technical-story query

For `subterranean water pressure records`, vector retrieval grouped **Pressure
Gate Seven Key**, **Pressure Gate Seven**, **The Pressure Gate**, and **Rising
Flood Pressure** without requiring all query words to appear in every chunk.
Hybrid preserved those semantic connections while allowing exact lexical clues
to outrank weaker similarities.

### Confidentiality

The character-scoped query for `ASH-SECRET-LETTER-CONTENTS-7F3Q` returned no
results in hybrid mode. The recorded design explanation is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z.

## Performance interpretation

The local LSA provider moves cost from query time to cold compilation. On the
238-record fixture, vector/hybrid cold builds take roughly half to three
quarters of a second; warm forced builds take about a tenth of a second and
exact revision reuse about six milliseconds. Query medians remain single-digit
milliseconds.

On 4,303 records, cold LSA fitting takes several seconds and exact vector scans
remain in tens of milliseconds. Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.

## Remaining search work

Historical architectural rationale is preserved in ADRAI A01M4971W13BPKM0G1FPZ86MA9Z; read with `adrai --repo WEDL_SOURCE_CHECKOUT show A01M4971W13BPKM0G1FPZ86MA9Z --json` in the WEDL development/source checkout.


Raw profile observations are retained in [SEARCH_PROFILE_BENCHMARKS.json](SEARCH_PROFILE_BENCHMARKS.json) and [MEDIUM_SEARCH_PROFILE_BENCHMARKS.json](MEDIUM_SEARCH_PROFILE_BENCHMARKS.json).
