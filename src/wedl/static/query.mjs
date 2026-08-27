export function entityRequestPath(kind, text) {
  const parameters = new URLSearchParams();
  if (kind) parameters.set("kind", kind);
  if (text) parameters.set("text", text);
  return `/api/entities${parameters.size ? `?${parameters}` : ""}`;
}

export function contextRequestPath(character, scene, query) {
  const parameters = new URLSearchParams({ character });
  if (scene) parameters.set("scene", scene);
  if (query) parameters.set("q", query);
  return `/api/context?${parameters}`;
}

export function authorSearchRequestPath(query, at = null, selectedThreadIds = null) {
  const parameters = new URLSearchParams({ q: query, perspective: "author" });
  if (at) {
    parameters.set("timeline", at.timeline);
    parameters.set("tick", String(at.tick));
    parameters.set("order", String(at.order ?? 0));
  } else parameters.set("allTime", "true");
  if (typeof query === "string" && query.trim()) {
    const ids = Array.isArray(selectedThreadIds) ? selectedThreadIds : [];
    for (const id of [...new Set(ids.filter((value) => typeof value === "string" && value.trim()))].sort()) parameters.append("threadId", id);
  }
  return `/api/search?${parameters}`;
}

export function threadMembershipRequestPaths(recordIds, selectedThreadIds, batchSize = 256) {
  const records = [...new Set((Array.isArray(recordIds) ? recordIds : []).filter((value) => typeof value === "string" && value.trim()))].sort();
  const threads = [...new Set((Array.isArray(selectedThreadIds) ? selectedThreadIds : []).filter((value) => typeof value === "string" && value.trim()))].sort();
  if (!records.length || !threads.length) return [];
  const size = Number.isInteger(batchSize) && batchSize > 0 ? Math.min(batchSize, 256) : 256;
  const paths = [];
  for (let index = 0; index < records.length; index += size) {
    const parameters = new URLSearchParams();
    for (const recordId of records.slice(index, index + size)) parameters.append("recordId", recordId);
    for (const threadId of threads) parameters.append("threadId", threadId);
    paths.push(`/api/thread-memberships?${parameters}`);
  }
  return paths;
}

function temporalEntityRequestPath(entityId, suffix, at) {
  const parameters = new URLSearchParams({
    timeline: at.timeline,
    tick: String(at.tick),
    order: String(at.order ?? 0),
  });
  return `/api/entities/${encodeURIComponent(entityId)}/${suffix}?${parameters}`;
}

export function entityStateRequestPath(entityId, at) {
  return temporalEntityRequestPath(entityId, "state", at);
}

export function characterKnowledgeRequestPath(characterId, at) {
  return temporalEntityRequestPath(characterId, "knowledge", at);
}

export function storyPointsRequestPath(scene) {
  const parameters = new URLSearchParams({ scene });
  return `/api/story-points?${parameters}`;
}

export function conversationRequestPath(conversationId, scope, at = null) {
  const parameters = new URLSearchParams({ perspective: "author" });
  if (scope === "all-time") parameters.set("allTime", "true");
  else if (at) {
    parameters.set("timeline", at.timeline);
    parameters.set("tick", String(at.tick));
    parameters.set("order", String(at.order ?? 0));
  }
  return `/api/conversations/${encodeURIComponent(conversationId)}?${parameters}`;
}

// The whereabouts projection deliberately has no all-time form.  With no
// horizon, the server resolves the world's shared current cursor; with one,
// it returns the same evidence as-of the selected author horizon.
export function whereaboutsRequestPath(at = null) {
  if (!at) return "/api/whereabouts";
  const parameters = new URLSearchParams({
    timeline: at.timeline,
    tick: String(at.tick),
    order: String(at.order ?? 0),
  });
  return `/api/whereabouts?${parameters}`;
}
