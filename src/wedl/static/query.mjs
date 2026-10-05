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

const EVENT_PROTOCOL = "wedl-event-consequences/v1";
const EVENT_FIELDS = ["effects", "changes", "causedTransitions", "outcomes", "causalSuccessors", "currentAtHorizon", "advisories"];
function decimal(value, low, high) {
  if (typeof value === "number" && !Number.isSafeInteger(value)) return null;
  const text = String(value);
  if (!/^(?:0|[1-9][0-9]*|-[1-9][0-9]*)$/.test(text)) return null;
  const number = BigInt(text);
  return number >= low && number <= high ? text : null;
}
function reportTime(at) {
  if (!at || typeof at.timeline !== "string" || !at.timeline.trim()) return null;
  const tick = decimal(at.tick, -(2n ** 63n), 2n ** 63n - 1n);
  const order = decimal(at.order ?? 0, -(2n ** 31n), 2n ** 31n - 1n);
  return tick !== null && order !== null ? { timeline: at.timeline, tick, order } : null;
}

// Lens is local presentation state and is never sent as an authorization grant.
export function eventConsequenceRequest({ revision, event, timeline, horizon = null, lens = "author", limit = 1000 }) {
  if (lens !== "author" || !/^[0-9a-f]{40}$/.test(revision || "") || typeof event !== "string" || !event.trim()
      || !Number.isInteger(limit) || limit < 1 || limit > 1000) return null;
  const at = reportTime(horizon || { timeline, tick: "9223372036854775807", order: "2147483647" });
  if (!at || at.timeline !== timeline) return null;
  return { protocol: EVENT_PROTOCOL, revision, event, at, limit };
}

export function eventConsequenceKey(request) {
  return JSON.stringify([request.revision, request.event, request.at.timeline, request.at.tick, request.at.order, request.limit]);
}

export function eventConsequenceMatches(payload, request) {
  if (!payload || payload.protocol !== EVENT_PROTOCOL || !request) return false;
  if (["invalid", "unavailable", "limit"].includes(payload.outcome)) {
    return Object.keys(payload).length === 4 && ["protocol", "outcome", "code", "message"].every((key) => typeof payload[key] === "string");
  }
  if (payload.outcome !== "ok" || payload.revision !== request.revision || payload.event?.id !== request.event
      || payload.event?.kind !== "event" || payload.timeScope?.mode !== "author-as-of") return false;
  const sameTime = (at) => at?.timeline === request.at.timeline && at?.tick === request.at.tick && at?.order === request.at.order;
  if (!sameTime(payload.at) || !sameTime(payload.timeScope.at) || !Array.isArray(payload.expectations) || payload.expectations.length
      || payload.applyAllowed !== true || !EVENT_FIELDS.every((field) => Array.isArray(payload[field]))) return false;
  if (EVENT_FIELDS.reduce((count, field) => count + payload[field].length, 0) > request.limit) return false;
  try { return new TextEncoder().encode(JSON.stringify(payload)).length <= 262144; } catch { return false; }
}

// Enrichment uses only records explicitly admitted by this report. It never
// reconciles HEAD or promotes a mismatched detail into the article cache.
export async function eventConsequenceDetails(report, request, read, isCurrent, signal) {
  const records = [...new Map(report.causedTransitions.filter((item) => ["knowledge", "relationship"].includes(item.kind))
    .map((item) => [item.record.id, item.kind])).entries()].slice(0, 24);
  const details = new Map(); let next = 0;
  const sameTime = (at) => { const normalized = reportTime(at); return normalized && Object.keys(request.at).every((key) => normalized[key] === request.at[key]); };
  await Promise.all(Array.from({ length: Math.min(4, records.length) }, async () => {
    while (isCurrent() && !signal?.aborted && next < records.length) {
      const [id, kind] = records[next++];
      const params = new URLSearchParams({ perspective: "author", ...request.at });
      try {
        const detail = await read(`/api/entities/${encodeURIComponent(id)}?${params}`, { signal });
        if (!isCurrent() || signal?.aborted) return;
        if (detail?.revision !== request.revision || detail.id !== id || detail.kind !== kind
            || (detail.perspective !== undefined && detail.perspective !== "author")
            || (detail.effectiveTime !== undefined && !sameTime(detail.effectiveTime))
            || (detail.timeScope !== undefined && (detail.timeScope.mode !== "author-as-of" || !sameTime(detail.timeScope.at)))) continue;
        details.set(id, detail);
      } catch { /* The report record's admitted name remains the fallback. */ }
    }
  }));
  return isCurrent() && !signal?.aborted ? details : new Map();
}
