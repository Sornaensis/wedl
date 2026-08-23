# wedl 0.5.1 Story-Authoring Validation

**Date:** 2026-08-20  
**Purpose:** Validate the application after using it to turn the Ash Archive fixture into a coherent completed mystery and novella.

## Release result

**PASS**

The exact populated source ZIP was extracted after packaging and all **37 tests passed** from that extraction:

| Test group | Passed |
|---|---:|
| Source, completed-story, and context/search tests | 15 |
| API, changeset, conversation, cache, and semantic tests | 14 |
| Independent FTS/vector/profile tests | 8 |
| **Total** | **37** |

The installable wheel was also installed into an empty target directory with `--no-deps`, initialized a fresh example repository, validated it, compiled it, and generated a bounded epilogue context successfully.

## Completed Ash Archive inventory

| Kind | Count |
|---|---:|
| Characters | 13 |
| Conversations | 16 |
| Environments | 15 |
| Events | 48 |
| Knowledge records | 63 |
| Locations | 20 |
| Objects | 24 |
| Relationships | 24 |
| Scenes | 16 |
| Story points | 22 |
| World | 1 |
| **Total** | **262** |

Additional narrative provenance:

- **138 canonical verbatim turns**
- **58 character recollections**
- Final closed scene: **An Honest Absence**, tick 205–208
- No active scene after completion
- Novella: **5,295 words**, 14 chapters

## Canonical ending checks

The following long-arc story points resolve in the final world:

- Build the Incomplete Archive
- Decide the Fate of the Descendant Ledger
- Survive the Vault Alarm
- Confront Caldrin Vey
- Question Sable at the River Gate
- Recover the Restricted-Vault Key

The final state also records:

- proof under notarial custody;
- the route under hydraulic custody;
- identity packets held separately from proof and route;
- the reconstruction cross-index destroyed under witness;
- Sable's map returned;
- Caldrin suspended from Archive authority;
- Rusk suspended from Archive access and retained as a compelled witness;
- the first Ledger of Honest Absences;
- Ilyra's restricted-vault key accepted by Mara under review.

## Perspective and historical checks

### Late-entry dialogue

- Sister Ansel hears only the final nine turns of `The Choice of Records`, beginning with her entrance line.
- Captain Rusk hears only the final eleven turns of `River Gate Pursuit`, beginning at tick 184 with “Give me the route key.”
- Oren hears only the two doorway turns in the epilogue conversation.

### Historical search regression

A defect found during authoring allowed future knowledge prose to enter a timeless generic character-self index. The generic section compiler now excludes knowledge records; knowledge is indexed only through its time-scoped lane.

At tick 121, Mara's character search cannot return the later knowledge record IDs for:

- threefold custody;
- the honest-absence rule;
- Caldrin's suspension.

Associative lexical/vector matches to older records remain possible and are not treated as temporal leakage.

### Belief coherence

After Ilyra's return, Mara, Nessa, and Oren receive explicit rejected transitions for the proposition that Ilyra remains missing. Oren's tick-208 query returns the missing-status claim with state `rejected` and cites the canonical event in which he sees her alive.

### Author-only markers

Character search at the epilogue returns no results for the fixture's author-secret markers. Author search continues to retrieve the relevant Ilyra and sealed-letter sources.

## Context validation

A fresh wheel-installed repository generated Mara's `An Honest Absence` context under a requested 5,000-character limit:

- Serialized response: **4,954 characters**
- Included context items: 15
- Omitted candidates: 51
- Perspective: character
- Exact epilogue dialogue included
- Completed custody and omission knowledge included
- Obsolete `Find Ilyra` pressure absent

## Compilation profiles

The completed 262-record fixture was compiled through all four profiles:

| Profile | Search documents | Database size |
|---|---:|---:|
| `state` | 0 | 1,495,040 bytes |
| `fts` | 1,171 | 2,981,888 bytes |
| `vector` | 1,171 | 6,639,616 bytes |
| `hybrid` | 1,171 | 7,135,232 bytes |

Hybrid vector storage:

- Document-vector links: 1,171
- Unique normalized vectors: 683
- Reused vectors in the forced validation compile: 683
- Newly generated vectors: 0

Independent search probes over `legal authority evidence custody` produced distinct FTS and vector rankings; hybrid fused both lanes.

## Packaging checks

### Source ZIP

- Root: `wedl-python-0.5.1/`
- Fixture Markdown records: 262
- Novella included: `docs/THE_ASH_ARCHIVE.md`
- Novella bytes: 33,275
- Only zero-length files: two intentional namespace package `__init__.py` markers
- ZIP integrity: passed

### Wheel

- Version: 0.5.1
- Fixture Markdown records: 262
- Fresh initialization: passed
- Validation: 262 records, zero diagnostics
- Bounded context generation: passed

### Completed-world Git archive

A separate archive preserves the actual Git repository used for authoring, excluding only disposable `.wedl/` compiled state. Its history includes:

1. Initial 0.5.0 world
2. Provisional custody proposal
3. Complete threefold-custody ending
4. Post-ending coherence corrections

## Authored artifacts

- `docs/THE_ASH_ARCHIVE.md` — close-third novella
- `docs/COMPLETED_STORY_WALKTHROUGH.md` — canonical plot walkthrough
- `docs/STORY_AUTHORING_NOTES.md` — interactive use findings and bug analysis
- `examples/complete-ash-archive-ending.json` — the large atomic ending changeset
- `examples/post-ending-coherence-cleanup.json` — epistemic cleanup after the ending
