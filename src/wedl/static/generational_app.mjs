import { createGenerationalApi, exactPoint, GenerationalReadError, PAGE_SIZE, READ_ITEMS } from "./generational_api.mjs";

const node = (document, tag, value = "") => { const item = document.createElement(tag); item.textContent = String(value); return item; };
const neutral = "Unavailable at this horizon";
const stateText = {
  available: "Available authored evidence", unknown: "Unknown: no admitted affirmative evidence",
  invalid: "Invalid request or horizon", unavailable: "Evidence unavailable", limit: "Limit reached; no partial evidence shown",
};
const asArray = (value) => Array.isArray(value) ? value : [];
const isId = (value) => typeof value === "string" && value.length > 0 && value.length <= 256;

function pointText(point) {
  return point && typeof point.tick === "string" && typeof point.order === "string"
    ? `${point.timeline}, tick ${point.tick}, order ${point.order}` : "unspecified applicability";
}
function applicabilityText(value) {
  if (!value || typeof value !== "object") return "unspecified applicability";
  return value.applicability_kind === "inclusive-interval"
    ? `${pointText(value.first)} through ${pointText(value.last)}` : pointText(value.point);
}
function namesFrom(results) {
  const ids = new Set();
  function visit(value, key = "") {
    if (Array.isArray(value)) { value.forEach((part) => visit(part, key)); return; }
    if (!value || typeof value !== "object") {
      if (isId(value) && (key.endsWith("_id") || ["recordId", "targetId", "eventId", "causeEventId", "participant_ids", "from", "to"].includes(key))) ids.add(value);
      return;
    }
    for (const [field, part] of Object.entries(value)) {
      if (["record_id", "transitionId"].includes(field)) continue;
      visit(part, field);
    }
  }
  results.forEach((result) => visit(result));
  return [...ids].slice(0, 1000);
}

export function mountGenerationalExplorer(document, api = createGenerationalApi()) {
  const get = (id) => document.getElementById(id);
  const el = Object.fromEntries(`gen-status horizon-form gen-mode gen-viewpoint gen-timeline gen-tick gen-order gen-refresh horizon-label discover-form gen-kind gen-query discover-status discover-results discover-more detail-title detail-status detail-content`.split(" ").map((id) => [id, get(id)]));
  const state = { point: null, kind: "character", query: "", cursor: null, selected: null, epoch: 0, labels: new Map() };
  const controllers = new Map();

  function status(element, value, error = false) {
    element.textContent = value;
    element.classList.toggle("error", error);
  }
  function cancel() { state.epoch += 1; for (const controller of controllers.values()) controller.abort(); controllers.clear(); }
  async function action(key, work) {
    controllers.get(key)?.abort();
    const controller = new AbortController(), epoch = state.epoch;
    controllers.set(key, controller);
    try {
      const result = await work(controller.signal);
      return !controller.signal.aborted && state.epoch === epoch && controllers.get(key) === controller ? result : null;
    } catch (error) {
      if (controller.signal.aborted || state.epoch !== epoch) return null;
      if (error?.state === "stale_revision") {
        cancel(); clearHorizon();
        fillTimelines();
        status(el["gen-status"], "The world changed. Choose the exact horizon again.", true);
      } else status(el["gen-status"], error instanceof GenerationalReadError ? error.message : "The read failed. Retry.", true);
      return null;
    } finally { if (controllers.get(key) === controller) controllers.delete(key); }
  }
  function clearDetail() {
    state.selected = null; state.labels.clear();
    el["detail-title"].textContent = "Evidence at horizon";
    el["detail-content"].replaceChildren();
    status(el["detail-status"], "Choose a search result.");
  }
  function clearHorizon() {
    state.point = null; state.cursor = null; state.query = "";
    el["horizon-label"].textContent = "No horizon selected.";
    el["discover-form"].querySelector("button").disabled = true;
    el["discover-results"].replaceChildren();
    el["discover-more"].hidden = true;
    status(el["discover-status"], "Apply a horizon to search.");
    clearDetail();
  }
  function fillTimelines() {
    const previous = el["gen-timeline"].value;
    el["gen-timeline"].replaceChildren(...api.timelines.map((timeline) => {
      const option = node(document, "option", timeline); option.value = timeline; return option;
    }));
    if (api.timelines.includes(previous)) el["gen-timeline"].value = previous;
  }
  async function boot() {
    cancel(); clearHorizon(); api.invalidate();
    const ready = await action("boot", (signal) => api.boot(signal));
    if (!ready) return;
    fillTimelines();
    status(el["gen-status"], ready.timelines.length ? "Session ready. Enter an exact author horizon." : "No declared timeline is available.", !ready.timelines.length);
  }
  async function fresh(signal) { await api.refresh(signal); }

  function label(id, learned = null) { return learned?.[id] || state.labels.get(id) || neutral; }
  function list(container, rows, render, empty) {
    const holder = node(document, "ol"); holder.className = "cards";
    if (!rows.length) holder.append(node(document, "li", empty));
    else holder.append(...rows.map(render));
    container.append(holder);
  }
  function section(container, title) { const box = node(document, "section"); box.className = "evidence"; box.append(node(document, "h3", title)); container.append(box); return box; }
  function field(box, name, value) {
    const line = node(document, "p"); line.append(node(document, "strong", `${name}: `), node(document, "span", value)); box.append(line);
  }
  function citations(box, items) {
    if (!asArray(items).length) { box.append(node(document, "p", "No admitted citation in this result.")); return; }
    const details = node(document, "details"), summary = node(document, "summary", `Citations (${items.length})`);
    details.append(summary);
    list(details, items, (item) => node(document, "li", api.mode === "character"
      ? `Knowledge ${item.knowledgeId || "locator unavailable"} · transition ${item.transitionId || "locator unavailable"} · ${pointText(item.time)} · learned ${pointText(item.learnedAt)}`
      : `${item.path || "Source unavailable"} · ${applicabilityText(item.applicability)}`), "No citations.");
    if (api.mode === "character") for (const item of items) {
      for (const evidence of asArray(item.evidence)) details.append(node(document, "p", `${evidence.kind} ${evidence.entityId} · ${evidence.itemId} · ${pointText(evidence.time)}`));
    }
    box.append(details);
  }
  function causes(box, items) {
    if (!asArray(items).length) return;
    const details = node(document, "details"); details.append(node(document, "summary", `Authored causes (${items.length})`));
    list(details, items, (item) => node(document, "li", `${label(item.eventId || item.causeEventId)} · ${item.citation?.path || "Source unavailable"} · ${applicabilityText(item.citation?.applicability)}`), "No causes.");
    box.append(details);
  }
  function referenceFields(box, value) {
    for (const [key, title] of [["character_id", "Character"], ["parent_id", "Parent organization"], ["organization_id", "Organization"],
      ["claimant_id", "Claimant"], ["holder_id", "Holder"], ["legacy_id", "Legacy"]]) {
      if (Object.hasOwn(value, key)) field(box, title, value[key] === null ? "Vacant" : label(value[key]));
    }
  }
  function history(box, entries) {
    if (!asArray(entries).length) return;
    const details = node(document, "details"); details.append(node(document, "summary", `Authored transitions (${entries.length})`));
    list(details, entries, (entry) => {
      const item = node(document, "li"); item.append(node(document, "strong", String(entry.kind || "Transition")));
      const payload = entry.payload || {};
      for (const [key, value] of Object.entries(payload)) {
        if (key === "title" && typeof value === "string") field(item, "Title", value);
        else if (key === "role") field(item, "Role", value == null ? "Role unspecified" : String(value));
        else if (["holder_id", "parent_id"].includes(key)) field(item, key === "holder_id" ? "Holder" : "Parent organization", value === null ? "Vacant" : label(value));
        else if (key === "basis") field(item, "Basis", String(value));
      }
      if (entry.causeEventId) field(item, "Caused by", label(entry.causeEventId));
      citations(item, [entry.citation]); return item;
    }, "No transitions."); box.append(details);
  }
  function rowCard(row, title, extra = []) {
    const card = node(document, "li"); card.append(node(document, "strong", title));
    field(card, "Authored state", row.state || "Unknown");
    const value = row.value || {};
    for (const [name, content] of extra) field(card, name, content);
    if (value.organization_kind) field(card, "Organization kind", value.organization_kind);
    if (value.legacy_kind) field(card, "Legacy kind", value.legacy_kind);
    if (Object.hasOwn(value, "role")) field(card, "Role", value.role == null ? "Role unspecified" : String(value.role));
    if (value.basis) field(card, "Basis", value.basis);
    referenceFields(card, value);
    citations(card, row.citations); causes(card, row.causes); history(card, row.history);
    return card;
  }
  function closedSection(container, heading, result, render) {
    const box = section(container, heading);
    if (result?.state === "available") render(box, result);
    else box.append(node(document, "p", `${stateText[result?.state] || "No result"}${result?.code ? ` (${result.code})` : ""}.`));
  }
  function relationCard(relation) {
    const card = node(document, "li"); card.append(node(document, "strong", label(relation.targetId, relation.labels)));
    field(card, "Relation", relation.label || "Authored relation");
    if (api.mode === "character") {
      field(card, "Belief state", relation.beliefState || "See each cited step");
      field(card, "Uncertainty", relation.uncertain ? "Uncertain asserted path" : "Accepted assertion; no comparison with canon");
    }
    if (relation.generationDistance != null) field(card, "Generations", String(relation.generationDistance));
    if (relation.citations) citations(card, relation.citations);
    for (const edge of asArray(relation.edges)) {
      field(card, "Cited step", `${label(edge.from, edge.labels)} → ${label(edge.to, edge.labels)}`);
      if (api.mode === "character") field(card, "Step belief", `${edge.beliefState || "Unknown"}${edge.uncertain ? " · uncertain" : ""}`);
      citations(card, edge.citations);
    }
    history(card, relation.history); return card;
  }
  function renderCharacter(container, results) {
    for (const [operation, heading] of [["parents", "Parents"], ["ancestors", "Ancestors"], ["descendants", "Descendants"]]) {
      closedSection(container, heading, results[operation], (box, result) => list(box, asArray(result.relations), relationCard, "No authored relations in this result."));
    }
    closedSection(container, "Approved vital state", results.vital, (box, result) => {
      if (api.mode === "character") { renderAssertions(box, result.assertions); return; }
      field(box, "Vital", result.vital || "Unknown"); citations(box, result.citations); history(box, result.history);
    });
    if (api.mode === "character") { restricted(container, "Current authored unions"); return; }
    closedSection(container, "Current authored unions", results["character-unions"], (box, result) => {
      list(box, asArray(result.unions), (row) => rowCard(row, "Union", [["Participants", asArray(row.value?.participant_ids).map(label).join(", ") || "None"]]), "No current authored unions.");
    });
  }
  function restricted(container, heading) {
    section(container, heading).append(node(document, "p", "Author-only endpoint. Unavailable in character view."));
  }
  function renderAssertions(container, assertions) {
    list(container, asArray(assertions), (row) => {
      const card = node(document, "li", row.kind || "Authored belief");
      field(card, "Belief state", row.beliefState || "Unknown");
      field(card, "Uncertainty", row.uncertain ? "Uncertain belief" : "Accepted belief; no comparison with canon");
      const payload = row.value || {};
      for (const [key, value] of Object.entries(payload)) {
        if (key.endsWith("_id")) field(card, key.replaceAll("_", " "), value === null ? "None asserted / vacancy" : label(value, row.labels));
        else if (key === "participant_ids") field(card, "Participants", asArray(value).map((id) => label(id, row.labels)).join(", "));
        else field(card, key.replaceAll("_", " "), String(value));
      }
      field(card, "Learned at", pointText(row.learnedAt));
      if (row.valid) field(card, "Asserted applicability", `${pointText(row.valid.from)}${row.valid.until ? ` through ${pointText(row.valid.until)}` : " onward"}`);
      if (row.applicable === false) field(card, "Historical knowledge", "Still held; not an affirmative relationship at this horizon");
      citations(card, row.citations); return card;
    }, "No held assertions in this result.");
  }
  function renderOrganization(container, results) {
    if (api.mode === "character") {
      closedSection(container, "Organization beliefs", results.organization, (box, data) => {
        renderAssertions(box, data.assertions); renderAssertions(box, data.roles);
        list(box, asArray(data.parentPath), relationCard, "No asserted parent path.");
      });
      restricted(container, "Reverse organization legacies"); return;
    }
    closedSection(container, "Organization or house", results.organization, (box, result) => {
      const organization = result.organization;
      field(box, "Authored state", organization.state || "Unknown");
      field(box, "Kind", organization.value?.organization_kind || "Unspecified");
      if (organization.value?.parent_id) field(box, "Parent", label(organization.value.parent_id));
      citations(box, organization.citations); causes(box, organization.causes);
      const parent = section(box, "Authored parent path");
      list(parent, asArray(result.parentPath), (path) => {
        const card = node(document, "li", label(path.targetId));
        for (const edge of asArray(path.edges)) citations(card, edge.citations);
        return card;
      }, "No authored parent path.");
      for (const [key, heading] of [["roles", "Active affiliations"], ["formerRoles", "Former affiliations"]]) {
        const roles = section(box, heading);
        list(roles, asArray(result[key]), (row) => rowCard(row, label(row.value?.character_id)), `No ${heading.toLowerCase()}.`);
      }
    });
    closedSection(container, "Explicitly linked legacies", results["organization-legacies"], (box, result) => {
      list(box, asArray(result.legacies), (row) => rowCard(row, label(row.recordId)), "No explicitly linked legacies.");
    });
  }
  function renderLegacy(container, result, allTime = false) {
    if (api.mode === "character") {
      closedSection(container, "Tenure and claim beliefs", result, (box, data) => {
        renderAssertions(box, data.tenures); renderAssertions(box, data.claims);
      });
      restricted(container, "All-time author history"); return;
    }
    const prefix = allTime ? "All-time authored history" : "As-of legacy evidence";
    closedSection(container, prefix, result, (box, data) => {
      field(box, "Authored state", data.legacy?.state || "Unknown");
      field(box, "Kind", data.legacy?.value?.legacy_kind || "Unspecified");
      if (data.legacy?.value?.organization_id) field(box, "Organization", label(data.legacy.value.organization_id));
      citations(box, data.legacy?.citations); causes(box, data.legacy?.causes); history(box, data.legacy?.history);
      const tenures = asArray(data.tenures), ordinal = new Map(tenures.map((row, index) => [row.recordId, `Tenure ${index + 1}`]));
      const tenureBox = section(box, allTime ? "Authored tenure histories" : "Tenures and vacancies");
      list(tenureBox, tenures, (row, index) => rowCard(row, `Tenure ${index + 1}`, [
        ["Predecessor", ordinal.get(row.value?.predecessor_tenure_id) || (row.value?.predecessor_tenure_id ? neutral : "None authored")],
        ["Successor", ordinal.get(row.value?.successor_tenure_id) || (row.value?.successor_tenure_id ? neutral : "None authored")],
      ]), "No authored tenure or vacancy.");
      if (!allTime) field(box, "Current holding tenures", String(asArray(data.holders).length));
      const claims = section(box, allTime ? "Authored claim histories" : "Claims, distinct from tenures");
      list(claims, asArray(data.claims), (row) => rowCard(row, "Claim"), "No authored claims.");
      const succession = section(box, "Explicit predecessor and successor links");
      list(succession, asArray(data.succession), (edge) => {
        const card = node(document, "li", `${ordinal.get(edge.from) || neutral} → ${ordinal.get(edge.to) || neutral}`);
        citations(card, edge.citations); causes(card, edge.causes); return card;
      }, "No explicit succession links.");
    });
  }
  async function resolveLabels(point, results, signal) {
    const ids = namesFrom(Object.values(results)).filter((id) => api.mode !== "character" || /^(?:char|character|organization|legacy|event)[_:]/.test(id));
    const labels = new Map(state.selected ? [[state.selected.id, state.selected.title]] : []);
    const alternatives = new Map();
    for (let offset = 0; offset < ids.length; offset += 100) {
      const response = await api.labels(point, ids.slice(offset, offset + 100), signal);
      if (response.state !== "available") throw new GenerationalReadError("Display labels could not be resolved.", response.state, response.code);
      for (const item of asArray(response.labels)) if (isId(item.id) && typeof item.title === "string") {
        const names = alternatives.get(item.id) || new Set(); names.add(item.title); alternatives.set(item.id, names);
      }
    }
    for (const [id, names] of alternatives) labels.set(id, [...names].sort().join(" / "));
    return labels;
  }
  async function select(item) {
    if (!state.point) return;
    controllers.get("selection")?.abort();
    controllers.get("all-time")?.abort();
    state.selected = item; state.labels.clear();
    el["detail-content"].replaceChildren();
    el["detail-title"].textContent = item.title;
    status(el["detail-status"], "Loading cited author evidence…");
    const point = state.point;
    const result = await action("selection", async (signal) => {
      await fresh(signal);
      let operations = item.kind === "character" ? ["parents", "ancestors", "descendants", "vital", "character-unions"]
        : item.kind === "organization" ? ["organization", "organization-legacies"] : ["legacy"];
      if (api.mode === "character") operations = [...operations.filter((op) => !["character-unions", "organization-legacies"].includes(op)), ...(item.kind === "character" ? ["context"] : [])];
      const values = await Promise.all(operations.map(async (operation) => [operation, await api.read(operation, point,
        { subject: item.id, items: READ_ITEMS, ...(operation === "ancestors" || operation === "descendants" || operation === "organization" ? { depth: 8 } : {}),
          ...(operation === "organization" && api.mode !== "character" ? { includeFormerRoles: true } : {}),
          ...(operation === "context" ? { maxCharacters: 20000 } : {}) }, signal)]));
      const results = Object.fromEntries(values);
      const labels = await resolveLabels(point, results, signal);
      return { results, labels };
    });
    if (!result || state.selected !== item || state.point !== point) return;
    state.labels = result.labels;
    el["detail-content"].replaceChildren();
    field(el["detail-content"], api.mode === "character" ? "Character learning horizon" : "Local-author horizon", pointText(point));
    if (item.kind === "character") renderCharacter(el["detail-content"], result.results);
    else if (item.kind === "organization") renderOrganization(el["detail-content"], result.results);
    else {
      renderLegacy(el["detail-content"], result.results.legacy);
      if (api.mode !== "character") {
      const button = node(document, "button", "Show separate all-time authored history"); button.type = "button";
      button.addEventListener("click", () => void loadAllTime(item, point)); el["detail-content"].append(button);
      }
    }
    if (api.mode === "character" && item.kind === "character") closedSection(el["detail-content"], "Held knowledge, including historical applicability", result.results.context, (box, data) => {
      renderAssertions(box, asArray(data.items).filter((entry) => entry.kind === "learned-history").flatMap((entry) => asArray(entry.result?.assertions)));
    });
    status(el["detail-status"], `${item.kind} evidence at ${pointText(point)}. Unknown and limit states stay separate.`);
  }
  async function loadAllTime(item, point) {
    const result = await action("all-time", async (signal) => {
      await fresh(signal);
      const response = await api.read("legacy", point, { subject: item.id, items: READ_ITEMS }, signal, "author-all-time");
      const labels = await resolveLabels(point, { allTime: response }, signal);
      return { response, labels };
    });
    if (!result || state.selected !== item || state.point !== point) return;
    for (const [id, title] of result.labels) state.labels.set(id, title);
    el["detail-content"].lastElementChild?.remove();
    renderLegacy(el["detail-content"], result.response, true);
    status(el["detail-status"], "All-time authored history is separate from the selected as-of holder and claims.");
  }
  function resultCard(item) {
    const card = node(document, "li");
    const button = node(document, "button", item.title); button.type = "button";
    button.addEventListener("click", () => void select(item)); card.append(button);
    if (item.matchedName && item.matchedName !== item.title) card.append(node(document, "p", `Matched alias: ${item.matchedName}`));
    return card;
  }
  async function discover(cursor = null) {
    if (!state.point) return;
    if (cursor === null) {
      controllers.get("selection")?.abort(); controllers.get("all-time")?.abort(); clearDetail();
      state.query = el["gen-query"].value.trim(); state.kind = el["gen-kind"].value;
    }
    if (!state.query || state.query.length > 64) { status(el["discover-status"], "Enter a title or alias prefix of at most 64 characters.", true); return; }
    const point = state.point, query = state.query, kind = state.kind;
    const response = await action("discover", async (signal) => { await fresh(signal); return api.discover(point, kind, query, cursor, signal); });
    if (!response || state.point !== point || state.query !== query || state.kind !== kind) return;
    el["discover-results"].replaceChildren();
    state.cursor = null; el["discover-more"].hidden = true;
    if (response.state !== "available") { status(el["discover-status"], `${stateText[response.state] || "No result"}${response.code ? ` (${response.code})` : ""}.`, true); return; }
    const items = asArray(response.results).slice(0, PAGE_SIZE);
    el["discover-results"].replaceChildren(...items.map(resultCard));
    state.cursor = response.cursor || null;
    el["discover-more"].hidden = !state.cursor;
    status(el["discover-status"], items.length ? `${items.length} admitted matches on this page at ${pointText(point)}.` : "No admitted matches on this page.");
  }

  el["horizon-form"].addEventListener("submit", (event) => {
    event.preventDefault();
    try {
      const point = exactPoint(el["gen-timeline"].value, el["gen-tick"].value, el["gen-order"].value);
      if (!api.timelines.includes(point.timeline)) throw new GenerationalReadError("Choose a declared timeline.");
      api.setScope(el["gen-mode"].value || "author-as-of", el["gen-viewpoint"].value);
      cancel(); clearHorizon(); state.point = point;
      el["discover-form"].querySelector("button").disabled = false;
      el["horizon-label"].textContent = `${api.mode === "character" ? "Character beliefs" : "Local-author evidence"} at ${pointText(point)}.`;
      status(el["gen-status"], api.mode === "character" ? "Learning horizon applied. Search explicitly learned labels." : "Exact author horizon applied. Search admitted titles and aliases.");
    } catch (error) { status(el["gen-status"], error.message, true); }
  });
  function scopeChanged() {
    cancel(); clearHorizon();
    el["gen-viewpoint"].disabled = el["gen-mode"].value !== "character";
    el["gen-viewpoint"].required = !el["gen-viewpoint"].disabled;
  }
  el["gen-mode"].addEventListener("change", scopeChanged);
  el["gen-viewpoint"].addEventListener("input", scopeChanged);
  for (const id of ["gen-timeline", "gen-tick", "gen-order"]) el[id].addEventListener("input", () => { cancel(); clearHorizon(); });
  el["gen-refresh"].addEventListener("click", () => void boot());
  el["discover-form"].addEventListener("submit", (event) => { event.preventDefault(); void discover(); });
  el["discover-more"].addEventListener("click", () => { if (state.cursor) void discover(state.cursor); });
  void boot();
  return { boot, discover, select };
}
