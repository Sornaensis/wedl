// Timeline coordinates are transported as decimal strings.  JSON numbers
// cannot represent every signed-64 tick exactly in JavaScript.
export function compareStoryTime(first, second) {
  const timeline = String(first.timeline || "").localeCompare(String(second.timeline || ""));
  if (timeline) return timeline;
  const tick = BigInt(String(first.tick)) - BigInt(String(second.tick));
  if (tick) return tick < 0n ? -1 : 1;
  const order = BigInt(String(first.order ?? 0)) - BigInt(String(second.order ?? 0));
  return order === 0n ? 0 : (order < 0n ? -1 : 1);
}

export function chronologyEntries(payload) {
  const points = Array.isArray(payload.points) ? payload.points : [];
  const spans = Array.isArray(payload.spans) ? payload.spans : [];
  const entries = points
    .filter((item) => item && item.at && item.entity)
    .map((item) => ({ at: item.at, boundary: "point", entity: item.entity, kind: item.kind, lifecycleState: item.lifecycleState || "", status: item.status || "", summary: item.summary || "", location: item.location || null, participants: item.participants || [] }));
  for (const item of spans) {
    if (!item || !item.start || !item.entity) continue;
    const shared = { entity: item.entity, kind: item.kind, lifecycleState: "", status: item.status || "", summary: item.summary || "", location: item.location || null, participants: item.participants || [] };
    entries.push({ ...shared, at: item.start, boundary: "start" });
    if (item.current) entries.push({ ...shared, at: item.current, boundary: "current" });
    // A close is semantically meaningful even when it shares a coordinate
    // with the current boundary, so it must remain a distinct card.
    if (item.end) entries.push({ ...shared, at: item.end, boundary: "end" });
  }
  const boundaryRank = { start: 0, current: 1, end: 2, point: 3 };
  return entries.sort((first, second) => compareStoryTime(first.at, second.at)
    || first.kind.localeCompare(second.kind)
    || (boundaryRank[first.boundary] ?? 99) - (boundaryRank[second.boundary] ?? 99)
    || String(first.entity.title || "").localeCompare(String(second.entity.title || ""))
    || String(first.entity.id || "").localeCompare(String(second.entity.id || "")));
}

// A scene interval is inclusive at both ends, like WEDL's source model.  The
// browser keeps this small temporal predicate here so the timeline rail and
// the author-facing whereabouts panel cannot drift apart.
export function sceneContainsMoment(scene, at) {
  if (!scene || !scene.start || !at || scene.start.timeline !== at.timeline) return false;
  if (compareStoryTime(scene.start, at) > 0) return false;
  return !scene.end || (scene.end.timeline === at.timeline && compareStoryTime(at, scene.end) <= 0);
}

// Shared by the whereabouts projection and per-card cast labels.  Presence
// intervals are inclusive at both endpoints, matching WEDL source semantics.
export function referencePresentAt(reference, at) {
  if (!reference || !at || typeof reference !== "object") return Boolean(reference);
  const from = reference.from || null;
  const until = reference.until || reference.to || null;
  if (from && (from.timeline || at.timeline) !== at.timeline) return false;
  if (until && (until.timeline || at.timeline) !== at.timeline) return false;
  if (from && compareStoryTime({ ...from, timeline: from.timeline || at.timeline }, at) > 0) return false;
  if (until && compareStoryTime(at, { ...until, timeline: until.timeline || at.timeline }) > 0) return false;
  return true;
}

// The API continues to return ordinary scene spans.  This projection makes a
// compact author view from them, while accepting the richer reference objects
// supplied by concurrent-scene capable servers.  `activeSceneIds` prevents a
// full-story read from claiming that every historical scene is happening now.
export function sceneSnapshotsAt(payload, at, { activeSceneIds = [] } = {}) {
  const activeIds = new Set(activeSceneIds.filter(Boolean));
  const spans = Array.isArray(payload && payload.spans) ? payload.spans : [];
  return spans.filter((scene) => scene && scene.kind === "scene" && scene.entity && scene.start)
    .filter((scene) => at ? sceneContainsMoment(scene, at) : activeIds.has(scene.entity.id))
    .map((scene) => ({
      entity: scene.entity,
      location: scene.location || null,
      participants: (Array.isArray(scene.participants) ? scene.participants : []).filter((participant) => !at || referencePresentAt(participant, at)),
    }));
}

// A shared coordinate is simultaneous information, not a sequence.  Keep
// same-coordinate cards together so the renderer can label mixed material
// "Meanwhile" without inventing duration, routes, or another chronology.
export function timelinePresentationGroups(entries, horizon = null) {
  const groups = [];
  for (const entry of entries || []) {
    const previous = groups[groups.length - 1];
    const sameMoment = entry && previous && compareStoryTime(previous.at, entry.at) === 0;
    if (sameMoment) previous.entries.push(entry);
    else groups.push({ type: "entry", at: entry && entry.at, entries: [entry] });
  }
  return groups.map((group) => {
    const scenesOnly = group.entries.length > 1 && group.entries.every((entry) => entry.chronologyKind === "scene" && entry.boundary === "current");
    return {
      ...group,
      type: group.entries.length === 1 ? "entry" : (scenesOnly ? "concurrent-scenes" : "meanwhile"),
      classification: classifyStoryMoment(group.at, horizon),
    };
  });
}

export function selectedTimelineId(requested, defaultTimeline, declarations) {
  return requested || defaultTimeline || (declarations[0] && declarations[0].id) || "";
}

export function horizonForTimeline(horizon, timelineId) {
  return horizon && horizon.timeline === timelineId ? horizon : null;
}

// A horizon is inclusive: a beat at the same ordinal coordinate is part of
// the author's current context.  Keep this here rather than comparing Numbers
// in the UI, since ticks are signed-64 values transported as decimal strings.
export function isAtOrBeforeHorizon(at, horizon) {
  return !horizon || classifyStoryMoment(at, horizon) !== "later";
}

// Timeline cards, article filtering, and visual state all share this inclusive
// classification.  Do not use Number arithmetic: signed-64 ticks arrive as
// decimal strings and may be adjacent beyond JavaScript's safe-integer range.
export function classifyStoryMoment(at, horizon) {
  if (!horizon) return "full-story";
  if (!at || at.timeline !== horizon.timeline) return "later";
  const order = compareStoryTime(at, horizon);
  return order < 0 ? "past" : (order === 0 ? "current" : "later");
}

export function firstAppearance(entries, entityId, timelineId) {
  return entries.find((entry) => entry && entry.entity && entry.entity.id === entityId
    && entry.at && entry.at.timeline === timelineId && entry.boundary !== "end") || null;
}

export function isEntityAvailable(entries, entityId, horizon) {
  if (!horizon) return true;
  const appearance = firstAppearance(entries, entityId, horizon.timeline);
  // Records without a declared beat are enduring reference material.  The
  // timeline contract cannot make a temporal claim about them, so leave them
  // readable rather than guessing an introduction point.
  return !appearance || isAtOrBeforeHorizon(appearance.at, horizon);
}

export function originLabel(declarations, timelineId) {
  const timeline = declarations.find((item) => item && item.id === timelineId);
  return timeline && timeline.origin && typeof timeline.origin.label === "string" ? timeline.origin.label : "";
}

export function timelineDisplayLabel(timeline) {
  return timeline && typeof timeline.label === "string" && timeline.label.trim() ? timeline.label.trim() : "Untitled chronology";
}

export function trailPositionLabel(at, horizon) {
  return ({ "full-story": "Full story", past: "Past", current: "At selected moment", later: "Beyond selected moment" })[classifyStoryMoment(at, horizon)];
}
