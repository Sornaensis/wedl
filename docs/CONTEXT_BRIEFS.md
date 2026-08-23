# Prose-first context briefs

The LLM-facing artifact is a curated Markdown writing packet, not a procedural
serialization of every relevant row. Structured metadata remains available to
tools, but the writing model receives a compact packet that reads like an
editorial brief.

## Character packet order

1. **Voice and intention** — one compact synthesis rather than repeated dossier
   sections.
2. **Present moment** — scene, location, currently present people, timed
   observations, and active environmental pressure.
3. **Conversation now** — a small recent window of audible canonical turns.
4. **What matters** — selected beliefs and suspicions ranked by query relevance,
   confidence, recency, and scene entity overlap.
5. **Remembered conversations** — the character’s current subjective
   recollections, not a perfect replay of old transcripts.
6. **Relationship pressure** — prose labels such as cautious trust, distrust,
   fear, obligation, and the relationship’s current facets.
7. **Useful recall** — a small number of authorized search fragments not already
   represented by stronger sections.
8. **Writing boundary** — a direct reminder not to import author truth or
   another mind.
9. **Provenance** — compact source markers grouped by entity ID and local
   section.

## Selection model

Context is assembled from small atoms. The composer:

- reserves the viewpoint profile, present frame, and perspective boundary;
- guarantees cross-section coverage before filling by raw score;
- enforces per-section quotas;
- removes near-duplicate statements;
- filters before retrieval ranking;
- keeps at least one useful live-conversation, belief, relationship, and memory
  atom when available and when the budget permits;
- applies the limit to the complete compact JSON response, not merely the
  Markdown string.

A 5,000-character request on the expanded Ash Archive remains below 5,000
serialized characters while still carrying live dialogue, current beliefs,
relationship pressure, provenance, and a perspective boundary.

## Author and dramatic-irony modes

Author context is separately composed from canonical scene constraints,
author-only prose, active/eligible story points, current transcript evidence,
and author retrieval.

Dramatic-irony mode produces two physically separate strings:

- `characterPrompt`
- `authorMargin`

The author margin is never merged into the character packet. This makes it
possible for an orchestration layer to give the writer both views without
quietly teaching the acting character the secret.

## Observations from use

The most useful packets were not the largest. Mara’s packet improved when the
composer stopped spending several slots on slightly different evidence
snippets and instead guaranteed one current exchange, one relevant memory, and
one present relationship. Sister Ansel’s packet remained much smaller because
she enters the active conversation late and has fewer accumulated Archive
beliefs; this is desirable rather than a missing-data problem.

The context protocol is `wedl-context/v3`.
