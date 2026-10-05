const KIND_LABELS = {
  world: "World overview", character: "Character", location: "Place", object: "Item",
  environment: "Environment", event: "Event", scene: "Scene", knowledge: "Knowledge",
  relationship: "Relationship", "story-point": "Plot thread", conversation: "Conversation",
};

const HIDDEN_KEYS = new Set([
  "schema", "id", "kind", "domain", "source_path", "sourcePath", "revision", "treeOid",
  "citation", "citations", "status", "aliases", "tags", "time", "turns", "recollections",
  "observations", "effects", "lifecycle", "participants", "conversations", "story_points",
  "related_story_points", "causes", "outcome_events", "dependencies", "trigger", "initial_state",
  "title", "provenance", "section_audiences", "inverse", "transitions",
]);

export function kindLabel(kind) { return KIND_LABELS[kind] || humanizeToken(kind || "lore entry"); }

export function humanizeToken(value) {
  return String(value || "").replace(/[_-]+/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

export function createEntityRegistry(entities) {
  return new Map(entities.filter((entity) => entity && entity.id).map((entity) => [entity.id, entity]));
}

export function isOpaqueIdentifier(value) {
  return typeof value === "string" && /^(?:(?:[a-z]+[_-])?[A-Z0-9]{12,}|[A-Z][A-Z0-9]*_[A-Z0-9_]{8,})$/.test(value);
}

export function safeDisplayName(value, registry, fallbackKind = "entry") {
  const entity = typeof value === "string" ? registry.get(value) : value;
  if (entity && typeof entity.title === "string" && entity.title.trim() && !isOpaqueIdentifier(entity.title.trim())) return entity.title.trim();
  return `Unavailable ${kindLabel((entity && entity.kind) || fallbackKind).toLowerCase()} reference`;
}

// Conversation reads deliberately carry both author-facing names and opaque
// identifiers for linking/provenance.  The compendium only needs the former:
// keep this conversion at the presentation boundary so a malformed or older
// payload cannot leak an identifier into the author view.
function conversationPersonName(value, registry) {
  if (value === "participants") return "everyone present";
  if (typeof value !== "string" || !value.trim()) return "Unavailable person reference";
  if (registry && registry.has(value)) return safeDisplayName(value, registry, "character");
  return isOpaqueIdentifier(value) ? "Unavailable person reference" : authorText(value, registry);
}
function conversationPersonId(value, registry) { return typeof value === "string" && registry && registry.has(value) && registry.get(value).kind === "character" ? value : ""; }

// v2 adds `beats`, but retains `verbatimTurns` for speech-only consumers and
// older compiled worlds.  Normalising here means both shapes render through
// one safe, ordered author transcript.
export function conversationTranscriptBeats(payload, registry) {
  const suppliedBeats = Array.isArray(payload && payload.beats) ? payload.beats : null;
  const source = suppliedBeats || (Array.isArray(payload && payload.verbatimTurns) ? payload.verbatimTurns : []);
  const normalized = source.filter((beat) => beat && typeof beat === "object").map((beat) => {
    const kind = beat.kind === "action" ? "action" : "speech";
    const actors = kind === "action" ? (Array.isArray(beat.actors) ? beat.actors : []).map((actor) => conversationPersonName(actor, registry)) : [];
    return {
      // Used only to resolve the canonical interruption link below. It is
      // discarded from the author-facing projection.
      id: typeof beat.id === "string" ? beat.id : "",
      kind,
      text: authorText(beat.text || "", registry),
      speaker: kind === "speech" ? conversationPersonName(beat.speaker, registry) : "",
      addressee: kind === "speech" && beat.addressee ? conversationPersonName(beat.addressee, registry) : "",
      actors,
      actorIds: kind === "action" ? (Array.isArray(beat.actorIds) ? beat.actorIds.map((actor) => conversationPersonId(actor, registry)) : (Array.isArray(beat.actors) ? beat.actors.map((actor) => conversationPersonId(actor, registry)) : [])) : [],
      speakerId: kind === "speech" ? (conversationPersonId(beat.speakerId, registry) || conversationPersonId(beat.speaker, registry)) : "",
      addresseeId: kind === "speech" ? (conversationPersonId(beat.addresseeId, registry) || conversationPersonId(beat.addressee, registry)) : "",
      delivery: kind === "speech" && beat.delivery ? humanizeToken(beat.delivery) : "",
      interrupts: kind === "speech" && typeof beat.interrupts === "string" ? beat.interrupts : "",
    };
  });
  const spokenById = new Map(normalized.filter((beat) => beat.kind === "speech" && beat.id).map((beat) => [beat.id, beat]));
  return normalized.map((beat) => {
    const interrupted = beat.interrupts ? spokenById.get(beat.interrupts) : null;
    return {
      kind: beat.kind,
      text: beat.text,
      speaker: beat.speaker,
      speakerId: beat.speakerId,
      addressee: beat.addressee,
      addresseeId: beat.addresseeId,
      actors: beat.actors,
      actorIds: beat.actorIds,
      delivery: beat.delivery,
      // A named, canonical earlier line works even with intervening beats;
      // it never exposes the opaque source link or changes transcript order.
      interruption: interrupted ? { speaker: interrupted.speaker, text: interrupted.text } : null,
    };
  });
}

export function authorText(value, registry) {
  return String(value || "").replace(/\b(?:(?:char|scene|event|conv|sp|loc|obj|env|rel|know|world|hyp)_[A-Za-z0-9]+|[A-Z][A-Z0-9]*_[A-Z0-9_]{8,}|[A-Z0-9]{12,})\b/g, (token) => {
    const entity = registry.get(token);
    return entity ? safeDisplayName(entity, registry, entity.kind) : "Unavailable reference";
  });
}

export function referenceValue(value) {
  if (typeof value === "string") return value;
  if (value && typeof value === "object") return value.entity || value.character || value.story_point || value.event || value.target || null;
  return null;
}

// Mirror ids.py's stable and spatial ID grammars at the text boundary. Keep
// the older tolerant scrub too, so malformed legacy IDs cannot become prose.
function effectText(value) {
  return String(value).replace(/(?<![A-Za-z0-9])(?:[a-z][a-z0-9-]*_[0-9A-HJKMNP-TV-Z]{26}|(?:map|location|overlay|route|anchor|portal):[a-z0-9][a-z0-9-]{0,127}(?:\/[a-z0-9][a-z0-9-]{0,127})*|(?:char|scene|event|conv|sp|loc|obj|env|rel|know|world|hyp)_[A-Za-z0-9]+|[A-Z][A-Z0-9]*_[A-Z0-9_]{8,}|[A-Z0-9]{12,})(?![A-Za-z0-9])/g, "Unavailable reference");
}

// Only the revision's declared state type can admit a reference leaf. An
// object containing an "entity" field is otherwise ordinary authored data.
export function effectValueParts(value, registry, definition = null) {
  const text = (value) => [{ type: "text", text: value }];
  if (value === null) return text("null");
  if (typeof value === "string") return text(registry.has(value) ? "Unavailable reference" : effectText(value));
  if (typeof value === "boolean" || (typeof value === "number" && Number.isFinite(value))) return text(String(value));
  if (Array.isArray(value)) return [...text("["), ...value.flatMap((item, index) => [...(index ? text(", ") : []), ...effectValueParts(item, registry, definition?.type === "array" ? definition.items : null)]), ...text("]")];
  if (value && typeof value === "object") {
    const entries = Object.entries(value);
    const entityType = definition?.type === "entity";
    const fields = entityType ? entries.filter(([key]) => key !== "entity") : entries;
    const reference = entityType ? (typeof value.entity === "string" ? [{ type: "reference", id: value.entity }] : text("Unavailable reference")) : [];
    if (entityType && !fields.length) return reference;
    return [...reference, ...text(entityType ? " {" : "{"), ...fields.flatMap(([key, item], index) => [...(index ? text("; ") : []), ...text(`${effectText(key)}: `), ...effectValueParts(item, registry, definition?.type === "object" ? definition.properties?.[key] : null)]), ...text("}")];
  }
  return text("Unavailable value");
}

export function effectOperationLabel(operation) {
  return { set: "Set to", clear: "Clear", "add-to-set": "Add to set", "remove-from-set": "Remove from set" }[operation] || "Unavailable operation";
}

export function displayTime(time) {
  if (!time || typeof time !== "object") return "";
  return "Story beat";
}

export function entryTime(detail) {
  const time = detail && detail.frontmatter && detail.frontmatter.time;
  if ((!time || typeof time !== "object") && detail && detail.kind === "story-point") {
    const transition = detail.frontmatter && detail.frontmatter.lifecycle && detail.frontmatter.lifecycle.transitions && detail.frontmatter.lifecycle.transitions[0];
    return transition && transition.time && Number.isInteger(transition.time.tick) ? transition.time : null;
  }
  if (!time || typeof time !== "object") return null;
  const point = time.start || time.current || time.end || time;
  return Number.isInteger(point.tick) ? point : null;
}

// WEDL intervals include both endpoints.  Keep the browser's scene reading
// rules aligned with conversation.interval_contains: ordinal ticks and their
// within-tick order are both significant, and a different timeline is never
// treated as an overlapping interval.
export function intervalContains(at, from = null, until = null) {
  if (!at) return true;
  // StoryTime.from_value defaults an omitted timeline to the world's active
  // chronology.  The article already has that chronology in its horizon.
  const start = from && { ...from, timeline: from.timeline || at.timeline };
  const end = until && { ...until, timeline: until.timeline || at.timeline };
  if (start && (start.timeline !== at.timeline || compareStoryTime(start, at) > 0)) return false;
  if (end && (end.timeline !== at.timeline || compareStoryTime(at, end) > 0)) return false;
  return true;
}

export function observationVisibleAt(observation, horizon) {
  if (!horizon || !observation || typeof observation !== "object") return true;
  return intervalContains(horizon, observation.at || null, observation.until || null);
}

export function referenceVisibleAt(reference, horizon, sceneInterval = {}) {
  if (!horizon || !reference || typeof reference !== "object") return true;
  // Scene participants use `to` in the published source schema.  `until` is
  // accepted as the generic reference interval spelling used by observations.
  return intervalContains(horizon, reference.from || sceneInterval.from || null,
    reference.until || reference.to || sceneInterval.until || null);
}

export function buildLoreArticle(detail, registry, options = {}) {
  const frontmatter = detail.frontmatter && typeof detail.frontmatter === "object" ? detail.frontmatter : {};
  const kind = detail.kind || frontmatter.kind || "";
  const sections = [];
  const includeReference = typeof options.includeReference === "function" ? options.includeReference : () => true;
  const includeTransition = typeof options.includeTransition === "function" ? options.includeTransition : () => true;
  const horizon = options.horizon || null;
  const sceneTime = frontmatter.time && typeof frontmatter.time === "object" ? frontmatter.time : {};
  const sceneInterval = { from: sceneTime.start || null, until: sceneTime.end || null };
  const addReferences = (title, values, roleKey = "role") => {
    const items = (Array.isArray(values) ? values : [])
      .filter((value) => referenceVisibleAt(value, horizon, kind === "scene" ? sceneInterval : {}))
      .map((value) => ({ id: referenceValue(value), role: value && typeof value === "object" ? value[roleKey] : "" }))
      .filter((item) => item.id && includeReference(item.id));
    if (items.length) sections.push({ type: "references", title, items });
  };

  if (kind === "scene") {
    if (frontmatter.location && includeReference(frontmatter.location)) sections.push({ type: "references", title: "Place", items: [{ id: frontmatter.location }] });
    addReferences("Cast", frontmatter.participants);
    addReferences("Objects in the scene", frontmatter.objects);
    addReferences("Plot threads", frontmatter.story_points, "");
    addReferences("Conversations", frontmatter.conversations, "");
    const observations = Array.isArray(frontmatter.observations) ? frontmatter.observations.filter((item) => item && observationVisibleAt(item, horizon)) : [];
    if (observations.length) sections.push({ type: "list", title: "What happens here", items: observations.map((item) => item.text).filter(Boolean) });
    if (Array.isArray(frontmatter.author_constraints) && frontmatter.author_constraints.length) sections.push({ type: "list", title: "Author constraints", items: frontmatter.author_constraints });
  } else if (kind === "event") {
    if (frontmatter.location && includeReference(frontmatter.location)) sections.push({ type: "references", title: "Place", items: [{ id: frontmatter.location }] });
    addReferences("People involved", frontmatter.participants);
    addReferences("Caused by", frontmatter.causes, "");
    addReferences("Related plot threads", frontmatter.related_story_points, "");
    if (Array.isArray(frontmatter.effects) && frontmatter.effects.length) sections.push({ type: "effects", title: "What changes", typesAvailable: Boolean(options.stateKeys), items: frontmatter.effects.map((item) => {
      const definition = options.stateKeys?.[registry.get(item?.target)?.kind]?.[item?.key];
      const valueDefinition = ["add-to-set", "remove-from-set"].includes(item?.operation) && definition?.type === "array" ? definition.items : definition;
      return { target: item && item.target, key: effectText(item && item.key || "state"), operation: item && item.operation, valueParts: effectValueParts(item && item.value, registry, valueDefinition) };
    }) });
  } else if (kind === "story-point") {
    addReferences("Depends on", frontmatter.dependencies && frontmatter.dependencies.all, "");
    addReferences("Outcome events", frontmatter.outcome_events, "");
    const transitions = frontmatter.lifecycle && frontmatter.lifecycle.transitions;
    const visibleTransitions = Array.isArray(transitions) ? transitions.filter(includeTransition) : [];
    const initialState = frontmatter.lifecycle && frontmatter.lifecycle.initial_state;
    const currentState = visibleTransitions.length ? visibleTransitions[visibleTransitions.length - 1].state : initialState;
    sections.push({ type: "plot-status", title: "Plot thread status", recordStatus: detail.status || frontmatter.status, initialState, currentState, stateContext: horizon ? "horizon" : "full-story" });
    if (visibleTransitions.length) sections.push({ type: "story-trail", title: "Plot thread trail", items: visibleTransitions });
  } else if (kind === "conversation") {
    if (frontmatter.scene && includeReference(frontmatter.scene)) sections.push({ type: "references", title: "Scene", items: [{ id: frontmatter.scene }] });
    addReferences("Participants", frontmatter.participants);
  } else if (kind === "relationship") {
    const endpoints = [frontmatter.from, frontmatter.to].filter(Boolean);
    addReferences("Between", endpoints, "");
    const visibleTransitions = Array.isArray(frontmatter.transitions) ? frontmatter.transitions.filter(includeTransition) : [];
    if (visibleTransitions.length) sections.push({ type: "relationship-trail", title: "Relationship trail", items: visibleTransitions });
  } else if (kind === "knowledge") {
    if (frontmatter.character && includeReference(frontmatter.character)) sections.push({ type: "references", title: "Known by", items: [{ id: frontmatter.character }] });
  }
  if (kind === "character" && frontmatter.initial_state && typeof frontmatter.initial_state === "object") {
    const location = referenceValue(frontmatter.initial_state.location);
    if (location && includeReference(location)) sections.push({ type: "references", title: "Starting place", items: [{ id: location }] });
    const condition = frontmatter.initial_state.condition;
    if (condition) sections.push({ type: "list", title: "Starting state", items: [humanizeToken(condition)] });
  }
  if (kind === "location") {
    const context = options.locationContext;
    if (context && typeof context === "object") {
      if (context.parent && includeReference(context.parent.id)) sections.push({ type: "references", title: "Within", items: [{ id: context.parent.id }] });
      const addPlaces = (title, values) => {
        const items = (Array.isArray(values) ? values : [])
          // Keep authored route prose as structured data.  The DOM renderer
          // applies authorText to it; it must never pass through token
          // humanization intended only for the directional relationship.
          .map((item) => item && item.place ? {
            id: item.place.id,
            description: item.description,
            summary: item.summary,
            role: item.reciprocal ? "Two-way authored connection" : "One-way authored connection",
          } : { id: item && item.id })
          .filter((item) => item.id && includeReference(item.id));
        if (items.length) sections.push({ type: "references", title, items });
      };
      const children = (Array.isArray(context.children) ? context.children : []).map((item) => ({ id: item && item.id })).filter((item) => item.id && includeReference(item.id));
      if (children.length) sections.push({ type: "references", title: "Places within", items: children });
      addPlaces("Connections from here", context.outgoing);
      addPlaces("Connections to here", context.incoming);
    } else {
      if (frontmatter.parent && includeReference(frontmatter.parent)) sections.push({ type: "references", title: "Within", items: [{ id: frontmatter.parent }] });
      addReferences("Connected places", frontmatter.links, "");
    }
  }

  // These are reverse links compiled from explicit authored references.  They
  // intentionally do not derive relationships from shared time or proximity.
  const inbound = (Array.isArray(options.inboundReferences) ? options.inboundReferences : [])
    .filter((item) => item && item.id && item.id !== detail.id && includeReference(item.id))
    .map((item) => ({ id: item.id }));
  if (inbound.length) sections.push({ type: "references", title: "Related lore", items: inbound });

  const additional = Object.entries(frontmatter).filter(([key, value]) => !HIDDEN_KEYS.has(key) && value != null && value !== "" && !Array.isArray(value) && typeof value !== "object").map(([key, value]) => ({ label: humanizeToken(key), value: authorText(value, registry) }));
  if (additional.length) sections.push({ type: "facts", title: "At a glance", items: additional });
  if (Array.isArray(frontmatter.tags) && frontmatter.tags.length) sections.unshift({ type: "tags", items: frontmatter.tags.map(humanizeToken) });
  return { title: safeDisplayName(detail, registry, kind), kindLabel: kindLabel(kind), body: detail.bodyMarkdown || "", sections };
}
import { compareStoryTime } from "./timeline.mjs";
