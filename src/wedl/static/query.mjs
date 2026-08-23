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

export function authorSearchRequestPath(query, at = null) {
  const parameters = new URLSearchParams({ q: query, perspective: "author" });
  if (at) {
    parameters.set("timeline", at.timeline);
    parameters.set("tick", String(at.tick));
    parameters.set("order", String(at.order ?? 0));
  } else parameters.set("allTime", "true");
  return `/api/search?${parameters}`;
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
