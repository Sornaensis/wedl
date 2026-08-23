import { authorText, safeDisplayName } from "./lore.mjs";

const FRIENDLY_HEADINGS = {
  "verbatim-turn": "the canonical transcript",
  "subjective-recollection": "a subjective recollection",
  "scene-observation": "what happens in the scene",
  observation: "what happens in the scene",
  "event-effect": "what changes after the event",
  effect: "what changes after the event",
  claim: "an authored knowledge note",
  transition: "a recorded story change",
};

// These documents exist to make a record discoverable by its authored name,
// but their generated fields are audit/search infrastructure rather than lore.
// An author should never be shown a card grounded only in those fields.
const METADATA_ONLY_DOCUMENT_KINDS = new Set(["entity-author-metadata"]);
const INTERNAL_METADATA_HEADINGS = new Set([
  "aliases", "domain", "id", "kind", "schema", "source-path", "sourcepath", "tags",
]);

// Search snippets come from an indexed document and may contain FTS highlight
// markup.  The compendium renders plain, author-facing copy only.
export function plainSearchText(value, registry) {
  return authorText(String(value || "").replace(/<[^>]*>/g, "").replace(/(^|\n)\s{0,3}#{1,6}\s+/g, "$1").replace(/\s+/g, " ").trim(), registry);
}

export function authorSearchHeading(value, registry) {
  const rawHeading = String(value || "").replace(/<[^>]*>/g, "").trim();
  const normalized = rawHeading.toLocaleLowerCase().replace(/[\s_]+/g, "-");
  if (INTERNAL_METADATA_HEADINGS.has(normalized)) return "";
  return FRIENDLY_HEADINGS[normalized] || plainSearchText(rawHeading, registry);
}

export function isAuthorSearchResult(result) {
  if (!result || METADATA_ONLY_DOCUMENT_KINDS.has(String(result.documentKind || ""))) return false;
  const heading = String(result.heading || "").replace(/<[^>]*>/g, "").trim()
    .toLocaleLowerCase().replace(/[\s_]+/g, "-");
  return !INTERNAL_METADATA_HEADINGS.has(heading);
}

export function presentSearchResults(payload, registry) {
  const results = Array.isArray(payload && payload.results) ? payload.results : [];
  const seen = new Set();
  return results.flatMap((result) => {
    if (!isAuthorSearchResult(result)) return [];
    const entity = registry.get(result && result.entityId);
    if (!entity || seen.has(entity.id)) return [];
    seen.add(entity.id);
    const heading = authorSearchHeading(result.heading, registry);
    const title = safeDisplayName(entity, registry, entity.kind);
    const rawSnippet = plainSearchText(result.snippet, registry);
    const snippet = rawSnippet.toLocaleLowerCase().startsWith(title.toLocaleLowerCase())
      ? rawSnippet.slice(title.length).trim()
      : rawSnippet;
    return [{
      entity,
      match: {
        heading: heading && heading !== title ? heading : "",
        snippet,
      },
    }];
  });
}

// Search remains full-text, but the compendium can narrow its presented
// results to one lore kind without teaching the search API a second query
// language.  This keeps a "People" search meaningfully different from an
// all-lore search while preserving the server's author-time filtering.
export function filterSearchResultsByKind(results, kind = "") {
  return (Array.isArray(results) ? results : []).filter((item) => !kind || (item.entity && item.entity.kind === kind));
}

// A changed author horizon needs a fresh, time-bounded search.  If that read
// fails, retain the prior cards and let the caller make the stale context
// explicit instead of presenting an empty result set as a genuine no-match.
export async function refreshAuthorSearch(read, present, previousResults = []) {
  try {
    return { error: null, results: present(await read()) };
  } catch (error) {
    return { error, results: Array.isArray(previousResults) ? previousResults : [] };
  }
}
