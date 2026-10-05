import { ApiError, createApiClient } from "./api.js";
import { CHRONOLOGY_PROTOCOL, SEARCH_PREDICATES, ERA_MODES, buildChronologyReplaceIntent, buildConvertRequest, buildFormatRequest, buildSearchRequest, buildStoryTimesRequest, capabilityIsEnabled, chronologyValueAriaLabel, chronologyValueLabel, cloneChronologyAnnotations, cloneChronologyValue } from "./chronology.mjs";
import { eventConsequenceDetails, eventConsequenceKey, eventConsequenceMatches, eventConsequenceRequest, characterKnowledgeRequestPath, conversationRequestPath, entityStateRequestPath, authorSearchRequestPath, threadMembershipRequestPaths, whereaboutsRequestPath } from "./query.mjs";
import { authorText, buildLoreArticle, conversationTranscriptBeats, createEntityRegistry, effectOperationLabel, effectValueParts, eventConsequenceModel, humanizeToken, kindLabel, safeDisplayName } from "./lore.mjs";
import { filterSearchResultsByKind, presentSearchResults, refreshAuthorSearch } from "./search.mjs";
import { compareCharacters, indexSortConfiguration, NAME_ASC, prominenceBand, PROMINENCE_SORT_HIGH, possibilitiesSortConfiguration, sortIndex, sortPossibilities, sortWhereabouts, validSort, whereaboutsSortConfiguration } from "./sorting.mjs";
import { clearSupersededNavigationLoading, createNavigationGeneration, historyAction, navigationSnapshot, restoreNavigation } from "./navigation.mjs";
import { chronologyEntries, classifyStoryMoment, compareStoryTime, horizonForTimeline, isAtOrBeforeHorizon, isEntityAvailable, originLabel, referencePresentAt, selectedTimelineId, timelineDisplayLabel, timelinePresentationGroups, trailPositionLabel } from "./timeline.mjs";

const el = {
  appStatus: document.querySelector("#app-status"), article: document.querySelector("#entity-detail-content"), articleStatus: document.querySelector("#article-status"), back: document.querySelector("#back-to-timeline"),
  entities: document.querySelector("#entities"), entitiesForm: document.querySelector("#entities-form"), entitiesStatus: document.querySelector("#entities-status"), entityText: document.querySelector("#entity-text"), searchKind: document.querySelector("#search-kind"), indexSort: document.querySelector("#index-sort"), horizon: document.querySelector("#author-horizon"), retrySearch: document.querySelector("#retry-search"), searchRecovery: document.querySelector("#search-recovery"), threadFilter: document.querySelector("#thread-filter"), threadFilterOptions: document.querySelector("#thread-filter-options"), threadFilterStatus: document.querySelector("#thread-filter-status"),
  indexHeading: document.querySelector("#entries-heading"), mobileBack: document.querySelector("#back-to-navigation"), nav: document.querySelector(".compendium-nav"), shell: document.querySelector(".compendium-shell"), worldName: document.querySelector("#world-name"),
};
const api = createApiClient();
const state = { activeTimelineId: "", chronologyCatalog: null, chronologyEditor: null, chronologyFormatCache: new Map(), chronologyRequestId: 0, detailRequestId: 0, eventConsequenceCache: new Map(), consequenceController: null, readerLens: "author", details: new Map(), entities: [], horizon: null, horizonEntries: new Map(), importanceByCharacter: new Map(), importanceError: "", importanceKey: "", importanceLoaded: false, indexSort: "", kind: "", membershipCache: new Map(), membershipError: "", membershipMap: new Map(), membershipRequestId: 0, mobilePane: "nav", possibilitiesSort: "", query: "", registry: new Map(), revision: "", restoreRequestId: 0, returnView: "index", searchError: "", searchKind: "", searchMatch: null, searchResults: null, selectedEntityId: "", selectedThreadIds: [], threadCatalog: { groupingAvailable: false, threads: [] }, threadCatalogError: "", threadSelectionRevisionChanged: false, threadSearchNeedsRefresh: false, timelineCache: new Map(), timelineDeclarations: [], timelineDefault: "", timelineRequestId: 0, view: "index", whereaboutsCache: new Map(), whereaboutsRequestId: 0, possibilitiesRequestId: 0, whereaboutsSort: "", worldStateKeys: null, listScroll: 0 };
const navigation = createNavigationGeneration();

async function revisionRead(path) {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    const requestRevision = state.revision; const payload = await api.get(typeof path === "function" ? path() : path);
    let responseRevision = payload && payload.revision;
    if (requestRevision && !responseRevision) responseRevision = (await api.get("/api/status")).revision || requestRevision;
    if (!requestRevision || (responseRevision === requestRevision && state.revision === requestRevision)) return payload;
    // A read that discovers a new revision must reconcile the visible view
    // before its old request can consume any retried data.
    if (responseRevision) await handleRevision(responseRevision);
    break;
  }
  throw new Error("The world changed while this reading was loading.");
}

async function chronologyPost(path, request) {
  const requestRevision = state.revision; const payload = await api.post(path, request); const responseRevision = payload && payload.revision;
  if (requestRevision && responseRevision && responseRevision !== requestRevision) {
    await handleRevision(responseRevision);
    throw new Error("The world changed while this chronology request was loading.");
  }
  return payload;
}

const text = (node, value) => { node.textContent = value; };
const node = (tag, value, className = "") => { const valueNode = document.createElement(tag); valueNode.textContent = value; if (className) valueNode.className = className; return valueNode; };
const normalizedThreadIds = (values) => [...new Set((Array.isArray(values) ? values : []).filter((value) => typeof value === "string" && value.trim()))].sort();
const catalogThreadIds = () => new Set((state.threadCatalog && state.threadCatalog.threads || []).map((entry) => entry && entry.id).filter((id) => typeof id === "string"));
const selectedThreadIds = () => normalizedThreadIds(state.selectedThreadIds);
function syncThreadFilterAvailability() {
  const catalog = state.threadCatalog || {}; const available = Boolean(catalog.groupingAvailable); const declared = Array.isArray(catalog.threads) && catalog.threads.length > 0;
  el.threadFilter.hidden = !available;
  el.threadFilter.disabled = !available || !declared || Boolean(state.threadCatalogError);
  el.threadFilterStatus.hidden = !state.threadCatalogError;
  text(el.threadFilterStatus, state.threadCatalogError || "");
}
function renderThreadFilter() {
  const catalog = state.threadCatalog || {}; const entries = Array.isArray(catalog.threads) ? catalog.threads : [];
  el.threadFilterOptions.replaceChildren();
  if (catalog.groupingAvailable && !entries.length) el.threadFilterOptions.append(node("p", "No narrative groups have been declared.", "field-help"));
  for (const entry of entries) {
    if (!entry || typeof entry.id !== "string" || typeof entry.label !== "string") continue;
    const label = document.createElement("label"); label.className = "thread-filter-option-label";
    const input = document.createElement("input"); input.type = "checkbox"; input.className = "thread-filter-option"; input.dataset.threadId = entry.id; input.checked = selectedThreadIds().includes(entry.id);
    input.addEventListener("change", () => { state.selectedThreadIds = normalizedThreadIds(Array.from(el.threadFilterOptions.querySelectorAll(".thread-filter-option")).filter((item) => item.checked).map((item) => item.dataset.threadId)); if (state.view === "timeline") { state.threadSearchNeedsRefresh = Boolean(state.query); void refreshMembershipView(); } else if (state.query) void search(); else void refreshMembershipView(); });
    label.append(input, node("span", entry.label)); el.threadFilterOptions.append(label);
  }
  syncThreadFilterAvailability();
}
const articleHeading = (value) => { const heading = node("h2", value); heading.id = "article-heading"; heading.tabIndex = -1; return heading; };
const name = (entity) => safeDisplayName(entity, state.registry, entity && entity.kind);
const cachedCharacterRole = (entity) => {
  if (!entity || entity.kind !== "character") return "";
  const detail = state.details.get(entity.id); const role = detail && detail.frontmatter && detail.frontmatter.role;
  return typeof role === "string" && role.trim() ? humanizeToken(authorText(role, state.registry)) : "";
};
const busy = (target, value) => target.setAttribute("aria-busy", String(value));
const currentTimeline = () => state.timelineCache.get(state.activeTimelineId) || [];
const activeHorizon = () => state.horizon || null;
const latestRecordedMoment = () => currentTimeline().reduce((latest, entry) => !latest || compareStoryTime(entry.at, latest) > 0 ? entry.at : latest, null);
const available = (entity) => !entity || isEntityAvailable(state.timelineCache.get(activeHorizon() && activeHorizon().timeline) || [], entity.id, activeHorizon());
const visible = (entities) => entities.filter(available);
const currentHorizonOption = () => [...state.horizonEntries.values()].find((entry) => state.horizon && compareStoryTime(entry.at, state.horizon) === 0 && (!state.horizon.anchorId || entry.at.anchorId === state.horizon.anchorId));
const horizonLabel = () => { const entry = currentHorizonOption(); return entry ? `Through ${entry.title}` : "Full story"; };

function writeHistory(mode = "replace") { const snapshot = navigationSnapshot(state, { listScroll: state.listScroll }); if (mode === "push") history.pushState(snapshot, "", location.pathname); else history.replaceState(snapshot, "", location.pathname); }
function invalidateDetail() { state.detailRequestId += 1; }
function invalidateTimeline() { state.timelineRequestId += 1; }
function beginNavigation() { state.consequenceController?.abort(); invalidateDetail(); invalidateTimeline(); clearSupersededNavigationLoading(el); return navigation.begin(); }
function navigationIsCurrent(generation) { return navigation.isCurrent(generation); }
function syncNavigation() { for (const item of el.nav.querySelectorAll(".nav-item")) item.removeAttribute("aria-current"); const active = [...el.nav.querySelectorAll(".nav-item")].find((item) => item.dataset.view ? item.dataset.view === state.view : (state.view === "index" && item.dataset.kind === state.kind)); if (active) active.setAttribute("aria-current", "page"); }
function syncHorizonControl() {
  const applicable = state.view !== "possibilities" && state.horizonEntries.size > 0;
  const selected = [...state.horizonEntries.entries()].find(([, entry]) => state.horizon && compareStoryTime(entry.at, state.horizon) === 0);
  el.horizon.value = applicable && selected ? selected[0] : "";
  el.horizon.disabled = !applicable;
}
function setView(view, mobilePane = view === "article" || view === "timeline" || view === "whereabouts" || view === "possibilities" || view === "chronology" ? "article" : "index") { state.view = view; state.mobilePane = mobilePane; el.shell.classList.toggle("detail-open", mobilePane === "article"); el.shell.classList.toggle("mobile-nav", mobilePane === "nav"); el.shell.classList.toggle("mobile-index", mobilePane === "index"); el.back.hidden = view === "index"; text(el.back, view === "article" && state.returnView === "timeline" ? "Back to timeline" : (view === "article" && state.returnView === "whereabouts" ? "Back to whereabouts" : "Back to list")); syncHorizonControl(); syncNavigation(); }
function focusArticle() { const target = el.article.querySelector("#article-heading") || el.article; target.focus(); }
function focusLoadedFeature(feature) { try { focusArticle(); } catch { text(el.articleStatus, `${feature} loaded, but the reading heading could not receive focus. You can continue reading below.`); } }
function saveLoadedFeatureHistory(feature, mode) { if (mode === "none") return; try { writeHistory(mode); } catch { text(el.articleStatus, `${feature} loaded, but the browser could not save this navigation step. You can continue reading or use the navigation links.`); } }
function renderLoadedFeatureFailure(feature, collection) { try { renderArticleMessage(`${feature} display problem`, `The ${collection} were loaded, but this page could not display them. Return to the list and try again.`); text(el.articleStatus, ""); focusLoadedFeature(feature); } catch { text(el.articleStatus, `${feature} loaded, but this page could not display the result. Return to the list and try again.`); } }
function isMobileLayout() { return window.matchMedia("(max-width: 719px)").matches; }
function renderArticleMessage(title, message) { el.article.replaceChildren(articleHeading(title), node("p", message)); }
function selectButton(id) { for (const button of el.entities.querySelectorAll(".entity-select")) button.setAttribute("aria-pressed", String(button.dataset.entityId === id)); }
function selectedMembership(entity) { return selectedThreadIds().length ? state.membershipMap.get(entity && entity.id) : null; }
function membershipLabels(entity) { const ids = selectedMembership(entity) || []; const labels = new Map((state.threadCatalog.threads || []).map((entry) => [entry.id, entry.label])); return ids.map((id) => labels.get(id) || id); }
function isSelectedMember(entity) { return !selectedThreadIds().length || state.membershipMap.has(entity && entity.id); }
function updateIndexStatus(source) { const count = source.filter((item) => available(item.entity || item) && isSelectedMember(item.entity || item)).length; const failed = Boolean(state.searchError || state.membershipError); const message = state.searchError || state.membershipError; text(el.entitiesStatus, failed ? message : (count ? `${count} matching entries` : "No matching entries in this reading context.")); el.entitiesStatus.classList.toggle("section-error", failed); el.searchRecovery.hidden = !failed; }
function updateIndexHeading() { const button = [...el.nav.querySelectorAll(".nav-item")].find((item) => item.dataset.kind === state.kind); text(el.indexHeading, state.kind ? (button ? button.textContent : "All lore") : "All lore"); }
function setSortOptions(control, configuration, requested) { const value = validSort(requested, configuration); control.replaceChildren(...configuration.options.map((option) => { const item = node("option", option.label); item.value = option.value; return item; })); control.value = value; return value; }
function syncIndexSort() { state.indexSort = setSortOptions(el.indexSort, indexSortConfiguration({ query: state.query, kind: state.searchKind || state.kind }), state.indexSort); }
function isCharacter(entity) { return entity && entity.kind === "character"; }
function prominenceScore(entity) { const score = state.importanceByCharacter.get(entity && entity.id); return Number.isFinite(score) ? score : null; }
function decorateCharacterName(element, entity) { if (!isCharacter(entity)) return element; const band = prominenceBand(prominenceScore(entity)); element.classList.add("character-prominence", `character-prominence--${band}`); const suffix = node("span", `, ${band === "unavailable" ? "calculated prominence unavailable" : `${band} calculated prominence`}`, "sr-only"); element.append(suffix); return element; }
function characterNameNode(entity, className = "") { return decorateCharacterName(node("span", name(entity), className), entity); }
function deriveImportance(payload, key = "") { state.importanceByCharacter = new Map((Array.isArray(payload && payload.characters) ? payload.characters : []).filter((entry) => entry && entry.character && Number.isFinite(Number(entry.importance && entry.importance.score))).map((entry) => [entry.character.id, Number(entry.importance.score)])); state.importanceError = ""; state.importanceLoaded = true; state.importanceKey = key; }
function referenceId(reference) { return typeof reference === "string" ? reference : (reference && typeof reference === "object" ? (reference.character || reference.entity || reference.id || "") : ""); }
function characterReference(reference) { const resolved = state.registry.get(referenceId(reference)); return isCharacter(resolved || reference); }
function timelineNeedsImportance(id) { return (state.timelineCache.get(id) || []).some((entry) => isCharacter(entry.entity) || ((entry.chronologyKind === "scene" || entry.chronologyKind === "event") && (entry.participants || []).some((participant) => referencePresentAt(participant, entry.at) && characterReference(participant)))); }
function needsImportance(timelineId = "") { return indexSource().some((item) => isCharacter(item.entity || item)) || Boolean(timelineId && timelineNeedsImportance(timelineId)); }
async function ensureImportance() { try { await loadWhereabouts(); return true; } catch { state.importanceByCharacter = new Map(); state.importanceError = "Calculated prominence is unavailable. Characters are shown by name until you retry."; state.importanceLoaded = true; return false; } }

function renderEntities(entities) {
  const fragment = document.createDocumentFragment();
  for (const candidate of entities) {
    const entity = candidate && candidate.entity ? candidate.entity : candidate;
    if (!entity || !available(entity) || !isSelectedMember(entity)) continue;
    const match = candidate && candidate.match;
    const item = document.createElement("article"); const button = document.createElement("button"); item.className = "entity"; button.type = "button"; button.className = "entity-select"; button.dataset.entityId = entity.id;
    button.setAttribute("aria-controls", "entity-detail-content"); button.setAttribute("aria-pressed", String(entity.id === state.selectedEntityId)); button.append(characterNameNode(entity, "entity-title"), node("span", kindLabel(entity.kind), "entity-meta")); const role = cachedCharacterRole(entity); if (role) button.append(node("span", role, "entity-role"));
    if (match && match.heading) button.append(node("span", `Found in ${match.heading}`, "entity-match"));
    if (match && match.snippet) button.append(node("span", match.snippet, "entity-snippet"));
    const labels = membershipLabels(entity); if (labels.length) button.append(node("span", `Narrative groups: ${labels.join(", ")}`, "thread-membership-badges"));
    button.addEventListener("click", () => { void showEntity(entity, button, { searchMatch: match || null }); }); item.append(button); fragment.append(item);
  }
  el.entities.replaceChildren(fragment);
}

function markdown(markdownText, title = "") {
  const fragment = document.createDocumentFragment(); let list; let sawContent = false;
  const flush = () => { if (list) fragment.append(list); list = null; };
  for (const raw of String(markdownText || "").replace(/\r/g, "").split("\n")) {
    const line = authorText(raw.trim(), state.registry); if (!line) { flush(); continue; }
    if (/^#{1,6}\s+/.test(line)) { const heading = line.replace(/^#+\s+/, ""); if (!sawContent && heading.toLocaleLowerCase() === title.toLocaleLowerCase()) { sawContent = true; continue; } flush(); fragment.append(node(`h${Math.min(4, line.match(/^#+/)[0].length + 1)}`, heading)); sawContent = true; continue; }
    if (/^[-*]\s+/.test(line)) { if (!list) list = document.createElement("ul"); list.append(node("li", line.replace(/^[-*]\s+/, ""))); continue; }
    if (/^>\s?/.test(line)) { flush(); fragment.append(node("blockquote", line.replace(/^>\s?/, ""))); continue; }
    flush(); fragment.append(node("p", line)); sawContent = true;
  }
  flush(); return fragment;
}

function reference(model) {
  const { id, role = "", description = "", summary = "" } = typeof model === "object" ? model : { id: model };
  const entity = state.registry.get(id); const item = document.createElement("li");
  if (entity && available(entity)) { const button = node("button", "", "lore-link"); button.append(characterNameNode(entity)); button.type = "button"; button.addEventListener("click", () => { void showEntity(entity, button); }); item.append(button); } else item.append(node("span", "Unavailable reference", "unavailable-reference"));
  for (const prose of [description, summary]) if (prose) item.append(node("span", authorText(prose, state.registry), "reference-detail"));
  if (role) item.append(node("span", humanizeToken(role), "reference-role")); return item;
}
function section(model) {
  const container = document.createElement("section"); container.className = "lore-section"; if (model.title) container.append(node("h3", model.title));
  if (model.type === "tags") { const list = document.createElement("ul"); list.className = "tag-list"; for (const item of model.items) list.append(node("li", item)); container.append(list); }
  if (model.type === "plot-status") { const list = document.createElement("ul"); const currentLabel = model.stateContext === "horizon" ? "Status at this reading" : "Recorded narrative outcome"; const values = [["Authored record", model.recordStatus], ["Starting story status", model.initialState], [currentLabel, model.currentState]]; for (const [label, value] of values) if (value) list.append(node("li", `${label}: ${humanizeToken(value)}`)); container.append(list); }
  if (model.type === "list" || model.type === "facts" || model.type === "story-trail" || model.type === "relationship-trail") { const list = document.createElement("ul"); list.className = model.type.endsWith("trail") ? "story-trail" : ""; for (const item of model.items) { const value = model.type === "facts" ? `${item.label}: ${item.value}` : (model.type === "story-trail" ? `Story status: ${humanizeToken(item.state || "recorded change")}${item.note ? ` — ${authorText(item.note, state.registry)}` : ""}` : (model.type === "relationship-trail" ? `Relationship status: ${humanizeToken(item.relationship_status || "recorded change")}${item.note ? ` — ${authorText(item.note, state.registry)}` : ""}` : item)); list.append(node("li", value)); } container.append(list); }
  if (model.type === "references") { const list = document.createElement("ul"); list.className = "reference-list"; for (const item of model.items) list.append(reference(item)); container.append(list); }
  if (model.type === "effects") {
    container.append(node("p", "Authored operations in this event; later events may change the resulting state.", "field-help"));
    if (!model.typesAvailable) container.append(node("p", "State value types are unavailable. Values remain visible; entity destinations cannot be resolved.", "field-help"));
    const list = document.createElement("ul");
    const appendReference = (line, id) => {
      const target = state.registry.get(id);
      if (target && available(target)) { const button = node("button", "", "lore-link"); button.append(characterNameNode(target)); button.type = "button"; button.addEventListener("click", () => { void showEntity(target, button); }); line.append(button); }
      else line.append(node("span", "Unavailable reference", "unavailable-reference"));
    };
    for (const item of model.items) {
      const line = document.createElement("li"); appendReference(line, item.target);
      line.append(node("span", `: ${humanizeToken(item.key)} — ${effectOperationLabel(item.operation)}: `));
      if (item.operation === "clear") line.append(node("span", "no value remains for this key."));
      else if (["set", "add-to-set", "remove-from-set"].includes(item.operation)) for (const part of item.valueParts) {
        if (part.type === "reference") appendReference(line, part.id);
        else line.append(node("span", part.text));
      }
      list.append(line);
    }
    container.append(list);
  }
  return container;
}

function renderArticle(detail) {
  const model = buildLoreArticle(detail, state.registry, { includeReference: (id) => available(state.registry.get(id)), includeTransition: (transition) => !transition || !transition.time || isAtOrBeforeHorizon(transition.time, activeHorizon()), horizon: activeHorizon(), stateKeys: state.worldStateKeys, inboundReferences: detail.inboundReferences, locationContext: detail.locationContext });
  const fragment = document.createDocumentFragment(); const header = document.createElement("header"); header.className = "article-header"; header.append(node("p", model.kindLabel, "article-kicker"), articleHeading(model.title));
  if (activeHorizon()) header.append(node("p", `Reading context: ${horizonLabel()}`, "field-help")); const summary = document.createElement("div"); summary.className = "article-summary"; summary.append(markdown(model.body, model.title)); header.append(summary); fragment.append(header);
  if (state.searchMatch && (state.searchMatch.heading || state.searchMatch.snippet)) { const match = document.createElement("section"); match.className = "search-grounding"; match.append(node("h3", state.searchMatch.heading ? `Found in ${state.searchMatch.heading}` : "Found in this entry")); if (state.searchMatch.snippet) match.append(node("p", state.searchMatch.snippet)); fragment.append(match); }
  for (const item of model.sections) fragment.append(section(item));
  const annotations = chronologyAnnotationSection(detail);
  if (annotations) fragment.append(annotations);
  el.article.replaceChildren(fragment);
}

function chronologyAnnotationSection(detail) {
  const annotations = Array.isArray(detail && detail.chronologyAnnotations) ? detail.chronologyAnnotations : [];
  const section = document.createElement("section"); section.className = "lore-section chronology-annotations";
  section.append(node("h3", "Chronology annotations"), node("p", "These are authored chronology values. Formatting and story-time mappings are requested from the server; no calendar or tick calculation happens in this browser.", "field-help"));
  const list = document.createElement("ul");
  for (const annotation of annotations) {
    const line = document.createElement("li"); const label = document.createElement("span");
    try { label.textContent = annotation.display || chronologyPresentation(annotation.value); label.setAttribute("aria-label", chronologyPresentation(annotation.value)); }
    catch { label.textContent = annotation && annotation.display || "Recorded chronology annotation"; }
    line.append(label);
    const format = node("button", "Format", "chronology-format-button"); format.type = "button";
    format.setAttribute("aria-label", `Format ${annotation.display || label.textContent}`);
    format.addEventListener("click", () => { void formatAnnotation(annotation, line); }); line.append(node("span", " "), format);
    list.append(line);
  }
  if (annotations.length) section.append(list); else section.append(node("p", "No chronology annotations have been authored for this record yet.", "field-help"));
  const edit = node("button", annotations.length ? "Replace this record’s chronology" : "Add chronology entry", "chronology-edit-button"); edit.type = "button";
  edit.addEventListener("click", () => { void openChronologyEditor(detail); }); section.append(edit);
  return section;
}

async function formatAnnotation(annotation, line) {
  try {
    const request = buildFormatRequest(annotation.value); const key = `${state.revision}\u0000${JSON.stringify(request.value)}`;
    let payload = state.chronologyFormatCache.get(key);
    if (!payload) { payload = await chronologyPost("/api/chronology/format", request); state.chronologyFormatCache.set(key, payload); }
    if (payload.outcome === "ok" && payload.result && typeof payload.result.formatted === "string") line.append(node("span", ` — ${payload.result.formatted}`, "chronology-formatted"));
    else line.append(node("span", ` — ${chronologyOutcomeMessage(payload)}`, "section-error"));
  } catch (error) { line.append(node("span", ` — ${chronologyErrorMessage(error)}`, "section-error")); }
}

function chronologyOutcomeMessage(payload) {
  if (!payload || typeof payload !== "object") return "Chronology service returned no result.";
  if (payload.outcome === "unavailable") return payload.detail || "Chronology is unavailable for this source schema.";
  return payload.detail || payload.reason || "The chronology value is invalid for this request.";
}
function chronologyErrorMessage(error) { return error instanceof ApiError && error.code ? `${error.code}: ${error.message}` : (error && error.message) || "Chronology request failed."; }
function chronologyCatalogEntries(catalog, key) { return Array.isArray(catalog && catalog[key]) ? catalog[key] : []; }
function chronologySelectedValue(control) { return control && typeof control.value === "string" ? control.value : ""; }
function chronologyOption(control, value, label) { const option = node("option", label); option.value = value; control.append(option); return option; }
function chronologyCatalogName(id, key) { const entry = chronologyCatalogEntries(state.chronologyCatalog, key).find((item) => item && item.id === id); return entry && (entry.label || entry.displayName) || (key === "eras" ? "an unnamed era" : "an unnamed calendar"); }
async function ensureChronologyCatalogForDetail(detail, requestId, generation) { if (state.chronologyCatalog || !Array.isArray(detail && detail.chronologyAnnotations) || !detail.chronologyAnnotations.length) return; try { const catalog = await revisionRead("/api/chronology"); if (navigationIsCurrent(generation) && requestId === state.detailRequestId) state.chronologyCatalog = catalog; } catch { /* Detail prose remains available if the optional public catalogue cannot be read. */ } }
function chronologyDateText(value) { return `${value.year}${value.month ? `-${value.month}` : ""}${value.day ? `-${value.day}` : ""}`; }
function chronologyPresentation(value) { const item = cloneChronologyValue(value); if (item.kind === "civil") return `${chronologyCatalogName(item.calendarId, "calendars")}: ${chronologyDateText(item)}`; if (item.kind === "era") return `${chronologyCatalogName(item.eraId, "eras")}: ${chronologyDateText(item)}`; if (item.kind === "range") return `${chronologyCatalogName(item.calendarId, "calendars")}: ${item.lower ? chronologyDateText(item.lower) : "open"} to ${item.upper ? chronologyDateText(item.upper) : "open"}`; if (item.kind === "conflict") return `Conflicting dates (${item.claims.map(chronologyPresentation).join("; ")})`; return chronologyValueLabel(item); }
function chronologyHitPresentation(hit, predicate) { const record = state.registry.get(hit.recordId); const recordName = record ? name(record) : "An unavailable record"; const value = hit.display || chronologyPresentation(hit.value); const precision = hit.precision ? `, ${humanizeToken(hit.precision)} precision` : ""; const relation = hit.relation ? ` ${humanizeToken(hit.relation)}` : ""; const provenance = Array.isArray(hit.provenance) && hit.provenance.length ? ` Provenance: ${hit.provenance.join(", ")}.` : ""; return `${recordName}: ${value}${relation} for ${humanizeToken(predicate)}${precision}.${provenance}`; }

function chronologyDateControl(catalog, { includeSearch = true } = {}) {
  const form = document.createElement("form"); form.className = "chronology-request"; form.setAttribute("aria-label", "Chronology query");
  const kind = document.createElement("select"); kind.className = "chronology-kind"; kind.setAttribute("aria-label", "Chronology value kind");
  for (const value of ["civil", "era", "range", "approximate", "conflict", "relative", "duration"]) chronologyOption(kind, value, humanizeToken(value));
  const calendar = document.createElement("select"); calendar.className = "chronology-calendar"; calendar.setAttribute("aria-label", "Calendar");
  for (const item of chronologyCatalogEntries(catalog, "calendars")) if (item && typeof item.id === "string") chronologyOption(calendar, item.id, item.label || item.displayName || item.id);
  const era = document.createElement("select"); era.className = "chronology-era"; era.setAttribute("aria-label", "Era");
  const syncEras = () => { const selectedCalendar = chronologySelectedValue(calendar); const selected = chronologySelectedValue(era); era.replaceChildren(); for (const item of chronologyCatalogEntries(catalog, "eras")) if (item && typeof item.id === "string" && (!selectedCalendar || !item.basisId || chronologyCatalogEntries(catalog, "calendars").some((calendarItem) => calendarItem && calendarItem.id === selectedCalendar && calendarItem.basisId === item.basisId))) chronologyOption(era, item.id, item.label || item.displayName || item.id); era.value = selected; };
  calendar.addEventListener("change", syncEras); syncEras();
  // Conversion targets deliberately have their own controls.  Changing a
  // target must never rewrite the authored/query value on the left.
  const targetKind = document.createElement("select"); targetKind.className = "chronology-target-kind"; targetKind.setAttribute("aria-label", "Conversion target kind"); chronologyOption(targetKind, "calendar", "Calendar target"); chronologyOption(targetKind, "era", "Era target");
  const targetCalendar = document.createElement("select"); targetCalendar.className = "chronology-target-calendar"; targetCalendar.setAttribute("aria-label", "Target calendar");
  for (const item of chronologyCatalogEntries(catalog, "calendars")) if (item && typeof item.id === "string") chronologyOption(targetCalendar, item.id, item.label || item.displayName || item.id);
  const targetEra = document.createElement("select"); targetEra.className = "chronology-target-era"; targetEra.setAttribute("aria-label", "Target era");
  const syncTargetEras = () => { const selectedCalendar = chronologySelectedValue(targetCalendar); const selected = chronologySelectedValue(targetEra); targetEra.replaceChildren(); for (const item of chronologyCatalogEntries(catalog, "eras")) if (item && typeof item.id === "string" && (!selectedCalendar || !item.basisId || chronologyCatalogEntries(catalog, "calendars").some((calendarItem) => calendarItem && calendarItem.id === selectedCalendar && calendarItem.basisId === item.basisId))) chronologyOption(targetEra, item.id, item.label || item.displayName || item.id); targetEra.value = selected; };
  const targetCalendarWrap = document.createElement("label"); targetCalendarWrap.className = "chronology-target-calendar-wrap"; targetCalendarWrap.textContent = "Target calendar"; targetCalendarWrap.append(targetCalendar);
  const targetEraWrap = document.createElement("label"); targetEraWrap.className = "chronology-target-era-wrap"; targetEraWrap.textContent = "Target era"; targetEraWrap.append(targetEra);
  const syncTargetKind = () => { const eraTarget = chronologySelectedValue(targetKind) === "era"; targetCalendarWrap.hidden = eraTarget; targetEraWrap.hidden = !eraTarget; };
  targetCalendar.addEventListener("change", syncTargetEras); targetKind.addEventListener("change", syncTargetKind); syncTargetEras(); syncTargetKind();
  const year = document.createElement("input"); year.className = "chronology-year"; year.type = "text"; year.value = "0"; year.setAttribute("inputmode", "text"); year.setAttribute("aria-label", "Year as an exact decimal string");
  const month = document.createElement("input"); month.className = "chronology-month"; month.type = "text"; month.placeholder = "month"; month.setAttribute("aria-label", "Optional month as an exact decimal string");
  const day = document.createElement("input"); day.className = "chronology-day"; day.type = "text"; day.placeholder = "day"; day.setAttribute("aria-label", "Optional day as an exact decimal string");
  const upperYear = document.createElement("input"); upperYear.className = "chronology-upper-year"; upperYear.type = "text"; upperYear.placeholder = "upper year"; upperYear.setAttribute("aria-label", "Upper year for between or range");
  const display = document.createElement("input"); display.className = "chronology-display"; display.type = "text"; display.placeholder = "approximate display"; display.setAttribute("aria-label", "Approximate display value");
  const relation = document.createElement("input"); relation.className = "chronology-relation"; relation.type = "text"; relation.placeholder = "relation"; relation.value = "before"; relation.setAttribute("aria-label", "Relative relation");
  const predicate = document.createElement("select"); predicate.className = "chronology-predicate"; predicate.setAttribute("aria-label", "Search predicate");
  for (const value of SEARCH_PREDICATES) chronologyOption(predicate, value, humanizeToken(value));
  const eraFilter = document.createElement("select"); eraFilter.className = "chronology-era-filter"; eraFilter.setAttribute("aria-label", "Era filter"); chronologyOption(eraFilter, "", "No era filter"); for (const item of chronologyCatalogEntries(catalog, "eras")) if (item && typeof item.id === "string") chronologyOption(eraFilter, item.id, item.label || item.id);
  const eraMode = document.createElement("select"); eraMode.className = "chronology-era-mode"; eraMode.setAttribute("aria-label", "Era filter mode"); for (const value of ERA_MODES) chronologyOption(eraMode, value, value === "authored" ? "Authored era" : "Overlapping bounds");
  const limit = document.createElement("input"); limit.className = "chronology-limit"; limit.type = "number"; limit.value = "100"; limit.min = "1"; limit.max = "10000"; limit.setAttribute("aria-label", "Result limit");
  const controls = { form, kind, calendar, era, targetKind, targetCalendar, targetEra, year, month, day, upperYear, display, relation, predicate, eraFilter, eraMode, limit };
  form.append(node("label", "Value kind"), kind, node("label", "Calendar"), calendar, node("label", "Era"), era, node("label", "Conversion target"), targetKind, targetCalendarWrap, targetEraWrap, node("label", "Year"), year, month, day, upperYear, display, relation);
  if (includeSearch) form.append(node("label", "Predicate"), predicate, node("label", "Era filter"), eraFilter, eraMode, node("label", "Limit"), limit);
  return controls;
}

function chronologyValueFromControl(controls, { upper = false } = {}) {
  const kind = chronologySelectedValue(controls.kind); const calendarId = chronologySelectedValue(controls.calendar); const eraId = chronologySelectedValue(controls.era); const upperYear = controls.upperYear.value.trim(); const year = upper ? upperYear : controls.year.value.trim(); const month = controls.month.value.trim(); const day = controls.day.value.trim();
  const date = { year }; if (month) date.month = month; if (day) date.day = day;
  if (kind === "civil") return cloneChronologyValue({ kind, calendarId, ...date });
  if (kind === "era") return cloneChronologyValue({ kind, eraId, ...date });
  if (kind === "range") return cloneChronologyValue({ kind, calendarId, lower: { calendarId, ...date }, upper: upperYear ? { calendarId, year: upperYear } : null });
  if (kind === "approximate") return cloneChronologyValue({ kind, displayValue: controls.display.value, bounds: { calendarId, lower: { calendarId, ...date }, upper: null } });
  if (kind === "conflict") return cloneChronologyValue({ kind, claims: [{ kind: "civil", calendarId, ...date }, { kind: "civil", calendarId, year: upper ? year : upperYear || year }] });
  if (kind === "relative") return cloneChronologyValue({ kind, relation: controls.relation.value, beforeId: "$chronology.reference" });
  return cloneChronologyValue({ kind: "duration", unit: "day", value: year });
}

function chronologyAdvisories(payload) {
  const advisories = Array.isArray(payload && payload.advisories) ? payload.advisories : [];
  return advisories.map((item) => `${item.code}${item.count != null ? ` (${item.count})` : ""}: ${item.message || ""}`).join(" ");
}

function chronologyLimit(value) {
  const text = value.trim();
  if (!text) return undefined;
  if (!/^(0|[1-9][0-9]*)$/.test(text)) throw new TypeError("search limit must be an integer from 1 to 10000");
  return [...text].reduce((total, digit) => total * 10 + digit.charCodeAt(0) - 48, 0);
}

function renderChronologyPanel(catalog) {
  const fragment = document.createDocumentFragment(); fragment.append(articleHeading("Chronology"));
  const capability = catalog && catalog.capability;
  if (!capabilityIsEnabled(catalog)) { fragment.append(node("p", "Chronology is unavailable for this ordinal-only source. Story-time ticks remain ordering labels; this page will not infer a calendar or duration from them.", "chronology-unavailable")); return fragment; }
  fragment.append(node("p", "Query the published chronology catalogue. Calendar and era lists retain server order, and every coordinate is sent as its exact decimal string.", "field-help"));
  const controls = chronologyDateControl(catalog); const actions = document.createElement("div"); actions.className = "chronology-actions";
  const output = node("p", "Choose a chronology value and a server operation.", "section-status"); output.setAttribute("role", "status"); output.setAttribute("aria-live", "polite");
  let requestGeneration = 0; const invalidateRequests = () => { requestGeneration += 1; }; controls.form.addEventListener("input", invalidateRequests); controls.form.addEventListener("change", invalidateRequests);
  const invoke = async (operation) => {
    const requestId = ++requestGeneration; let value;
    try { value = chronologyValueFromControl(controls); const predicate = chronologySelectedValue(controls.predicate); const eraId = chronologySelectedValue(controls.eraFilter); const options = { predicate, value, limit: chronologyLimit(controls.limit.value) };
      if (predicate === "between") options.upper = chronologyValueFromControl(controls, { upper: true }); if (eraId) options.eraFilter = { eraId, mode: chronologySelectedValue(controls.eraMode) };
      const request = operation === "format" ? buildFormatRequest(value) : operation === "convert" ? buildConvertRequest(value, conversionTarget(controls)) : operation === "search" ? buildSearchRequest(options) : buildStoryTimesRequest(value);
      const paths = { format: "/api/chronology/format", convert: "/api/chronology/convert", search: "/api/chronology/search", "story-times": "/api/chronology/story-times" };
      const payload = await chronologyPost(paths[operation], request); if (requestId !== requestGeneration) return;
      if (payload.outcome !== "ok") { output.textContent = chronologyOutcomeMessage(payload); output.className = "section-status section-error"; return; }
      output.className = "section-status";
      if (operation === "search") { const matches = payload.result && Array.isArray(payload.result.matches) ? payload.result.matches : []; output.textContent = `${matches.length} server-ordered matching annotations${matches.length ? `: ${matches.map((item) => chronologyHitPresentation(item, predicate)).join("; ")}` : "."}${chronologyAdvisories(payload) ? ` ${chronologyAdvisories(payload)}` : ""}`; }
      else if (operation === "story-times") { const mapping = payload.result || { mapping: "none", storyTimes: [] }; output.textContent = mapping.mapping === "none" ? "No explicit StoryTime anchor maps this chronology value." : mapping.mapping === "unique" ? `One explicit StoryTime: ${mapping.storyTimes[0].timeline} ${mapping.storyTimes[0].tick}/${mapping.storyTimes[0].order}.` : `Ambiguous explicit StoryTimes: ${mapping.storyTimes.map((item) => `${item.timeline} ${item.tick}/${item.order}`).join("; ")}. Choose a reading horizon explicitly.`; }
      else output.textContent = payload.result && (payload.result.formatted || payload.result.axisDay) ? (payload.result.formatted || `Axis day ${payload.result.axisDay}`) : "Chronology request succeeded.";
    } catch (error) { if (requestId !== requestGeneration) return; output.className = "section-status section-error"; output.textContent = chronologyErrorMessage(error); }
  };
  for (const [operation, label] of [["format", "Format"], ["convert", "Convert"], ["search", "Search"], ["story-times", "Map to StoryTime"]]) { const button = node("button", label); button.type = "button"; button.addEventListener("click", () => { void invoke(operation); }); actions.append(button); }
  fragment.append(controls.form, actions, output);
  return fragment;
}

function conversionTarget(controls) { return chronologySelectedValue(controls.targetKind) === "era" ? { eraId: chronologySelectedValue(controls.targetEra) } : { calendarId: chronologySelectedValue(controls.targetCalendar) }; }
async function openChronology({ historyMode = "push", navigationGeneration } = {}) {
  const generation = navigationGeneration ?? beginNavigation(); state.chronologyRequestId += 1; const requestId = state.chronologyRequestId; setView("chronology"); renderArticleMessage("Chronology", "Loading the public chronology catalogue…"); text(el.articleStatus, "Loading chronology…");
  try { const catalog = await revisionRead("/api/chronology"); if (!navigationIsCurrent(generation) || requestId !== state.chronologyRequestId) return; state.chronologyCatalog = catalog; el.article.replaceChildren(renderChronologyPanel(catalog)); text(el.articleStatus, ""); if (historyMode !== "none") writeHistory(historyMode); focusLoadedFeature("Chronology"); }
  catch (error) { if (navigationIsCurrent(generation) && requestId === state.chronologyRequestId) { renderArticleMessage("Chronology unavailable", chronologyErrorMessage(error)); text(el.articleStatus, ""); focusLoadedFeature("Chronology"); } }
}

async function openChronologyEditor(detail) {
  const annotations = Array.isArray(detail && detail.chronologyAnnotations) ? detail.chronologyAnnotations : [];
  try { const currentAnnotations = cloneChronologyAnnotations(annotations); state.chronologyEditor = { record: detail.id, title: detail.title, currentAnnotations, draftText: JSON.stringify(currentAnnotations, null, 2), idempotencyKey: "", preview: null, previewRequestId: 0, reconciliation: null }; setView("chronology"); renderChronologyEditor(); writeHistory("push"); focusArticle(); }
  catch (error) { text(el.articleStatus, chronologyErrorMessage(error)); }
}

function editorDraftKey(annotations, idempotency) { return `${annotations}\u0000${idempotency}`; }
function editorPreviewIsCurrent(editor, key) { return Boolean(editor.preview && editor.preview.draftKey === key && editor.preview.revision === state.revision); }

function renderChronologyEditor() {
  const editor = state.chronologyEditor; if (!editor) return;
  const fragment = document.createDocumentFragment(); fragment.append(articleHeading(`Replace chronology for ${editor.title}`), node("p", "This is a complete replacement for this record only. IDs, temporary IDs, provenance, relative and duration values, tag extensions, and every x-* value remain in the submitted JSON unchanged.", "field-help"));
  const outline = document.createElement("section"); outline.className = "chronology-editor-outline"; outline.append(node("h3", "Structured annotation overview"));
  const outlineList = document.createElement("ol");
  for (const annotation of editor.currentAnnotations) { const item = document.createElement("li"); const details = document.createElement("details"); const summary = node("summary", annotation.display || chronologyValueLabel(annotation.value)); const fields = document.createElement("dl"); fields.className = "chronology-editor-fields"; const append = (label, value) => { const row = document.createElement("div"); row.append(node("dt", label), node("dd", typeof value === "string" ? value : JSON.stringify(value))); fields.append(row); }; append("Identifier", annotation.id || annotation.temporaryId); append("Role", annotation.role || "(none)"); append("Provenance", annotation.provenance); append("Kind", annotation.value.kind); append("Value", chronologyValueLabel(annotation.value)); if (annotation.value.tagExtensions) append("Tag extensions", annotation.value.tagExtensions); for (const key of Object.keys(annotation).filter((key) => key.startsWith("x-"))) append(key, annotation[key]); details.append(summary, fields); item.append(details); outlineList.append(item); }
  outline.append(outlineList); fragment.append(outline);
  if (editor.reconciliation) { const reconciliation = document.createElement("section"); reconciliation.className = "chronology-reconciliation"; reconciliation.setAttribute("aria-live", "polite"); reconciliation.append(node("h3", "Reconcile the current record and your draft"), node("p", "Another change reached this record. Your draft is retained below; compare it with the reloaded current annotations, then preview again before applying.")); const current = document.createElement("pre"); current.className = "chronology-current-annotations"; current.setAttribute("aria-label", "Current reloaded chronology annotations"); current.textContent = JSON.stringify(editor.currentAnnotations, null, 2); const draft = document.createElement("pre"); draft.className = "chronology-draft-annotations"; draft.setAttribute("aria-label", "Retained chronology draft"); draft.textContent = editor.draftText; reconciliation.append(node("h4", "Current annotations"), current, node("h4", "Your retained draft"), draft); fragment.append(reconciliation); }
  const form = document.createElement("form"); form.className = "chronology-editor"; const annotations = document.createElement("textarea"); annotations.className = "chronology-annotations-json"; annotations.rows = 16; annotations.value = editor.draftText; annotations.setAttribute("aria-label", "Complete chronology annotation replacement"); const idempotency = document.createElement("input"); idempotency.type = "text"; idempotency.value = editor.idempotencyKey || ""; idempotency.setAttribute("aria-label", "Optional idempotency key for safe replay");
  const preview = node("button", "Preview replacement"); preview.type = "submit"; const status = node("p", "Edit the complete annotation array, then preview the server diff.", "section-status"); status.setAttribute("role", "status"); status.setAttribute("aria-live", "polite"); const diff = document.createElement("pre"); diff.className = "chronology-preview-diff"; diff.setAttribute("aria-label", "Server preview source diff"); diff.hidden = true; const apply = node("button", "Confirm and apply replacement"); apply.type = "button"; apply.disabled = !editorPreviewIsCurrent(editor, editorDraftKey(annotations.value, idempotency.value));
  const invalidatePreview = () => { editor.draftText = annotations.value; editor.idempotencyKey = idempotency.value; editor.preview = null; editor.previewRequestId += 1; apply.disabled = true; diff.hidden = true; diff.textContent = ""; status.className = "section-status"; status.textContent = "Draft changed. Preview the current draft before applying."; };
  annotations.addEventListener("input", invalidatePreview); annotations.addEventListener("change", invalidatePreview); idempotency.addEventListener("input", invalidatePreview); idempotency.addEventListener("change", invalidatePreview);
  const parseIntent = () => { const value = JSON.parse(annotations.value); const next = cloneChronologyAnnotations(value); editor.draftText = annotations.value; editor.idempotencyKey = idempotency.value; return { next, intent: buildChronologyReplaceIntent({ expectedHead: state.revision, record: editor.record, annotations: next, summary: `Replace chronology for ${editor.title}`, ...(editor.idempotencyKey ? { idempotencyKey: editor.idempotencyKey } : {}) }) }; };
  form.addEventListener("submit", (event) => { event.preventDefault(); void (async () => { const key = editorDraftKey(annotations.value, idempotency.value); const requestId = ++editor.previewRequestId; editor.preview = null; apply.disabled = true; diff.hidden = true; diff.textContent = ""; try { const { intent } = parseIntent(); const response = await api.post("/api/authoring/preview", intent); if (state.chronologyEditor !== editor || requestId !== editor.previewRequestId || key !== editorDraftKey(annotations.value, idempotency.value)) return; const plan = response && response.preview || {}; editor.preview = { intent, response, confirmationToken: plan.confirmationToken || "", draftKey: key, revision: state.revision }; editor.reconciliation = null; status.className = "section-status"; if (typeof plan.diff === "string" && plan.diff) { diff.hidden = false; diff.textContent = plan.diff; status.textContent = "Preview ready. Review the exact source diff, then confirm the replacement."; } else if (typeof plan.diff === "string") status.textContent = "No source changes: this replacement matches the current record."; else status.textContent = response.authorImpact && response.authorImpact.summary || "Preview ready. Review the server result, then confirm the replacement."; apply.disabled = !editor.preview.confirmationToken || plan.valid !== true; } catch (error) { if (state.chronologyEditor !== editor || requestId !== editor.previewRequestId) return; editor.preview = null; apply.disabled = true; status.className = "section-status section-error"; status.textContent = chronologyErrorMessage(error); } })(); });
  apply.addEventListener("click", () => { void (async () => { const key = editorDraftKey(annotations.value, idempotency.value); try { if (!editorPreviewIsCurrent(editor, key) || !editor.preview.confirmationToken) { invalidatePreview(); throw new Error("The draft or replay key changed. Preview the current draft before applying."); } const response = await api.post("/api/authoring/apply", editor.preview.intent, { confirmationToken: editor.preview.confirmationToken }); status.className = "section-status"; status.textContent = response.idempotentReplay ? "Replacement replayed from its existing receipt." : "Chronology replacement applied and compiled."; editor.preview = null; await handleRevision(response.newHead || response.revision || response.head || state.revision); } catch (error) { status.className = "section-status section-error"; status.textContent = chronologyErrorMessage(error); if (error instanceof ApiError && ["stale_revision", "conflict"].includes(error.code)) { const draftText = annotations.value; const replayKey = idempotency.value; invalidatePreview(); try { await refreshRevision(); const current = await api.get(`/api/entities/${encodeURIComponent(editor.record)}`); if (state.chronologyEditor !== editor || !current || !Array.isArray(current.chronologyAnnotations)) return; editor.currentAnnotations = cloneChronologyAnnotations(current.chronologyAnnotations); editor.title = current.title || editor.title; editor.draftText = draftText; editor.idempotencyKey = replayKey; editor.reconciliation = true; editor.preview = null; editor.previewRequestId += 1; renderChronologyEditor(); } catch { status.textContent += " The draft is retained, but current annotations could not be reloaded."; } } } })(); });
  form.append(node("label", "Replay key (optional)"), idempotency, annotations, preview, apply, status, diff); fragment.append(form); el.article.replaceChildren(fragment);
}

function momentSection(title, help = "") {
  const container = document.createElement("section"); container.className = "lore-section moment-section"; container.append(node("h3", title));
  if (help) container.append(node("p", help, "field-help"));
  return container;
}
function appendAuthorValue(line, value) {
  if (typeof value === "string" && state.registry.has(value) && available(state.registry.get(value))) {
    const entity = state.registry.get(value); const button = node("button", "", "lore-link"); button.append(characterNameNode(entity)); button.type = "button"; button.addEventListener("click", () => { void showEntity(entity, button); }); line.append(button); return;
  }
  if (Array.isArray(value)) { line.append(node("span", value.map((item) => typeof item === "string" ? authorText(item, state.registry) : humanizeToken(item)).join(", "))); return; }
  if (value && typeof value === "object") { const referenceId = ["entity", "character", "location", "target"].map((key) => value[key]).find((item) => typeof item === "string" && state.registry.has(item)); if (referenceId) { appendAuthorValue(line, referenceId); return; } line.append(node("span", Object.entries(value).filter(([key]) => key !== "id").map(([key, item]) => `${humanizeToken(key)}: ${authorText(String(item), state.registry)}`).join("; ") || "Recorded")); return; }
  line.append(node("span", authorText(String(value), state.registry)));
}
function populateStateSection(container, payload) {
  const fields = Object.entries(payload && payload.state && typeof payload.state === "object" ? payload.state : {}).filter(([key, value]) => key !== "id" && key !== "location" && value != null && value !== "");
  if (!fields.length) { container.append(node("p", "No changing state is recorded for this person at this story moment.")); return; }
  const list = document.createElement("ul");
  for (const [key, value] of fields) { const line = document.createElement("li"); line.append(node("strong", `${humanizeToken(key)}: `)); appendAuthorValue(line, value); list.append(line); }
  container.append(list);
}
function locationReferenceId(value) {
  if (typeof value === "string") return value;
  if (value && typeof value === "object") return ["id", "entity", "location", "target"].map((key) => value[key]).find((item) => typeof item === "string") || "";
  return "";
}
function appendLocationReference(line, value) {
  const id = locationReferenceId(value);
  const entity = state.registry.get(id);
  if (entity && available(entity)) { const button = node("button", "", "lore-link"); button.append(characterNameNode(entity)); button.type = "button"; button.addEventListener("click", () => { void showEntity(entity, button); }); line.append(button); return; }
  line.append(node("span", value && value.title ? value.title : "No place recorded", "unavailable-reference"));
}
function populateLocationSections(locationSlot, journeySlot, payload, fullStory) {
  const location = payload && payload.state && payload.state.location;
  if (location) { const line = document.createElement("p"); line.append(node("span", fullStory ? "Last recorded place: " : "Recorded place: ")); appendLocationReference(line, location); locationSlot.append(line); }
  else locationSlot.append(node("p", "No place is recorded for this reading context."));
  const history = Array.isArray(payload && payload.locationHistory) ? payload.locationHistory : [];
  if (!history.length) { journeySlot.append(node("p", "No authored location changes are recorded.")); return; }
  const list = document.createElement("ul"); list.className = "story-trail";
  for (const item of history) {
    const line = document.createElement("li");
    const operation = item.operation === "initial" ? "Starts in " : (item.operation === "clear" ? "Leaves the recorded place" : "Moves to ");
    line.append(node("span", operation));
    if (item.operation !== "clear") appendLocationReference(line, item.location);
    list.append(line);
  }
  journeySlot.append(list);
}
function populateKnowledgeSection(container, payload) {
  const knowledge = Array.isArray(payload && payload.knowledge) ? payload.knowledge : [];
  if (!knowledge.length) { container.append(node("p", "No authored knowledge is recorded for this person at this story moment.")); return; }
  const list = document.createElement("ul");
  for (const item of knowledge) {
    const line = document.createElement("li"); const statement = authorText(item.statement || item.claimKey || "Recorded knowledge", state.registry);
    line.append(node("strong", `${humanizeToken(item.state || "known")}: `), node("span", statement));
    if (item.note) line.append(node("span", ` — ${authorText(item.note, state.registry)}`));
    const source = state.registry.get(item.sourceEntityId || item.causingEventId);
    if (source && available(source)) { line.append(node("span", " · Learned through ")); const button = node("button", name(source), "lore-link"); button.type = "button"; button.addEventListener("click", () => { void showEntity(source, button); }); line.append(button); }
    list.append(line);
  }
  container.append(list);
}
async function renderCharacterMoment(detail, requestId, generation) {
  if (detail.kind !== "character" || !navigationIsCurrent(generation) || requestId !== state.detailRequestId) return;
  await appendCharacterProminence(detail, requestId, generation);
  if (!navigationIsCurrent(generation) || requestId !== state.detailRequestId) return;
  const horizon = activeHorizon(); const readingMoment = horizon || latestRecordedMoment();
  if (!readingMoment) { const caveat = momentSection("Full-story reading", "No dated story moment is available to resolve a recorded location. The authored prose and starting state above remain unchanged."); el.article.append(caveat); return; }
  const fullStory = !horizon;
  const stateSlot = momentSection(fullStory ? "Last recorded state" : "State at this story moment", fullStory ? "Full story is not one story moment; this is the last authored beat in the selected chronology." : "This is the recorded state through the selected author horizon."); stateSlot.append(node("p", "Loading recorded state…", "field-help"));
  const locationSlot = momentSection(fullStory ? "Recorded location" : "Location at this story moment", fullStory ? "The journey below is ordered through the last authored beat in this chronology." : "This place is recorded through the selected author horizon."); locationSlot.append(node("p", "Loading recorded location…", "field-help"));
  const journeySlot = momentSection("Journey trail", "Only canonical location set and clear transitions are shown, in exact story order."); journeySlot.append(node("p", "Loading location history…", "field-help"));
  const knowledgeSlot = horizon ? momentSection("What this person knows", "Knowledge is limited to what has been authored through the selected author horizon.") : null;
  if (knowledgeSlot) knowledgeSlot.append(node("p", "Loading authored knowledge…", "field-help"));
  el.article.append(stateSlot, locationSlot, journeySlot); if (knowledgeSlot) el.article.append(knowledgeSlot);
  const current = () => navigationIsCurrent(generation) && requestId === state.detailRequestId && stateSlot.isConnected && locationSlot.isConnected && journeySlot.isConnected && (!knowledgeSlot || knowledgeSlot.isConnected);
  const stateRead = revisionRead(() => entityStateRequestPath(detail.id, readingMoment)).then((payload) => { if (!current()) return; stateSlot.replaceChildren(node("h3", fullStory ? "Last recorded state" : "State at this story moment"), node("p", fullStory ? "Full story is not one story moment; this is the last authored beat in the selected chronology." : "This is the recorded state through the selected author horizon.", "field-help")); locationSlot.replaceChildren(node("h3", fullStory ? "Recorded location" : "Location at this story moment"), node("p", fullStory ? "The journey below is ordered through the last authored beat in this chronology." : "This place is recorded through the selected author horizon.", "field-help")); journeySlot.replaceChildren(node("h3", "Journey trail"), node("p", "Only canonical location set and clear transitions are shown, in exact story order.", "field-help")); populateStateSection(stateSlot, payload); populateLocationSections(locationSlot, journeySlot, payload, fullStory); }).catch(() => { if (current()) { stateSlot.replaceChildren(node("h3", fullStory ? "Last recorded state" : "State at this story moment"), node("p", "The recorded state could not be loaded. Other lore on this page is still available.", "section-error")); locationSlot.replaceChildren(node("h3", fullStory ? "Recorded location" : "Location at this story moment"), node("p", "The recorded location could not be loaded. Other lore on this page is still available.", "section-error")); journeySlot.replaceChildren(node("h3", "Journey trail"), node("p", "The location history could not be loaded. Other lore on this page is still available.", "section-error")); } });
  const knowledgeRead = knowledgeSlot ? revisionRead(() => characterKnowledgeRequestPath(detail.id, horizon)).then((payload) => { if (!current()) return; knowledgeSlot.replaceChildren(node("h3", "What this person knows"), node("p", "Knowledge is limited to what has been authored through the selected author horizon.", "field-help")); populateKnowledgeSection(knowledgeSlot, payload); }).catch(() => { if (current()) knowledgeSlot.replaceChildren(node("h3", "What this person knows"), node("p", "The knowledge record could not be loaded. Other lore on this page is still available.", "section-error")); }) : Promise.resolve();
  await Promise.all([stateRead, knowledgeRead]);
}
function conversationBeatCard(beat) {
  const line = document.createElement("article"); line.className = `conversation-beat conversation-beat--${beat.kind}`;
  if (beat.kind === "action") {
    const heading = node("h4", "Action — ");
    if (beat.actors.length) beat.actors.forEach((actor, index) => { if (index) heading.append(node("span", ", ")); const entity = state.registry.get(beat.actorIds && beat.actorIds[index]); heading.append(entity ? characterNameNode(entity) : node("span", actor)); }); else heading.append(node("span", "Someone present"));
    line.append(heading, node("p", beat.text));
    return line;
  }
  const heading = document.createElement("h4"); const speaker = state.registry.get(beat.speakerId); heading.append(speaker ? characterNameNode(speaker) : node("span", beat.speaker));
  if (beat.addressee) { const addressee = state.registry.get(beat.addresseeId); heading.append(node("span", " to ", "conversation-addressee"), addressee ? characterNameNode(addressee) : node("span", beat.addressee, "conversation-addressee")); }
  line.append(heading);
  if (beat.interruption) { line.classList.add("conversation-beat--interrupting"); line.append(node("p", `Cuts in on ${beat.interruption.speaker}'s earlier line: “${beat.interruption.text}”`, "conversation-interruption-label")); }
  line.append(node("p", beat.text));
  if (beat.delivery) line.append(node("p", beat.delivery, "article-kicker"));
  return line;
}
function renderConversation(data) {
  const fragment = document.createDocumentFragment(); const transcript = document.createElement("section"); transcript.className = "lore-section"; transcript.append(node("h3", "Canonical transcript")); if (data.timeScope && data.timeScope.mode === "all-time") transcript.append(node("p", "Full story collects the authored transcript across its history; it is not a single story moment.", "field-help")); const beats = conversationTranscriptBeats(data, state.registry);
  if (!beats.length) transcript.append(node("p", "No spoken lines or recorded action are available in this reading context."));
  const beatList = document.createElement("div"); beatList.className = "conversation-transcript";
  for (const beat of beats) beatList.append(conversationBeatCard(beat));
  if (beats.length) transcript.append(beatList); fragment.append(transcript);
  const recollections = Array.isArray(data.recollections) ? data.recollections : []; if (recollections.length) { const memories = document.createElement("section"); memories.className = "lore-section"; memories.append(node("h3", "Subjective recollections"), node("p", "These memories are authored impressions, separate from the canonical transcript.", "field-help")); for (const memory of recollections) { const record = document.createElement("article"); record.className = "timeline-entry"; const character = state.registry.get(memory.character); const heading = document.createElement("h4"); heading.append(character ? characterNameNode(character) : node("span", "Unavailable person reference")); record.append(heading, node("p", authorText(memory.summary || memory.interpretation || "No recollection summary was supplied.", state.registry))); memories.append(record); } fragment.append(memories); }
  el.article.append(fragment);
}
function renderConversationError() { const section = momentSection("Conversation record", "The transcript could not be loaded. The conversation overview and related lore remain available."); section.lastChild.className = "section-error"; el.article.append(section); }
function horizonBoundary() { const message = document.createElement("section"); message.className = "welcome"; message.append(node("p", "Reading context", "eyebrow"), articleHeading("This entry comes later"), node("p", `This lore has not entered the story ${horizonLabel().toLocaleLowerCase()}. Choose a later reading context or return to the list.`)); el.article.replaceChildren(message); }

async function loadEventStateKeys(requestId, generation) {
  if (state.worldStateKeys !== null) return;
  const world = state.entities.find((entity) => entity.kind === "world");
  if (!world) return;
  try {
    const revision = state.revision;
    const detail = state.details.get(world.id) || await revisionRead(() => `/api/entities/${encodeURIComponent(world.id)}`);
    if (!navigationIsCurrent(generation) || requestId !== state.detailRequestId || revision !== state.revision) return;
    state.details.set(world.id, detail);
    const definitions = detail?.frontmatter?.state_keys;
    if (definitions && typeof definitions === "object" && !Array.isArray(definitions)) state.worldStateKeys = definitions;
  } catch { /* Direct effects still render without guessing their value types. */ }
}


function consequenceNameParts(container, parts, names) {
  for (const part of parts) {
    if (part.type !== "reference") { container.append(node("span", part.text)); continue; }
    const entry = names.get(part.id); const button = node("button", "", "lore-link"); button.type = "button";
    const label = entry ? safeDisplayName(entry, names, entry.kind) : "Unavailable reference";
    button.textContent = effectValueParts(label, new Map()).map((item) => item.text || "Unavailable reference").join("");
    const destination = state.registry.get(part.id);
    if (destination && available(destination)) button.addEventListener("click", () => void showEntity(destination, button, { returnView: state.returnView }));
    else button.disabled = true;
    container.append(button);
  }
}
function renderConsequenceReport(slot, report, linkedDetails, horizonDescription, capturedHorizon) {
  const model = eventConsequenceModel(report, state.registry, { stateKeys: state.worldStateKeys, linkedDetails });
  slot.replaceChildren(node("p", horizonDescription), node("p", "Recorded links and authored state; this report does not establish narrative completeness.", "section-help"));
  for (const sectionModel of model.sections) {
    const sectionNode = momentSection(sectionModel.title, sectionModel.help);
    if (!sectionModel.rows.length) sectionNode.append(node("p", "No recorded links in this section.", "section-help"));
    else {
      const list = document.createElement("ul");
      for (const row of sectionModel.rows) {
        const item = document.createElement("li"); consequenceNameParts(item, row.parts, model.names);
        if (row.label || row.time) item.append(node("p", [row.label, row.time ? trailPositionLabel(row.time, capturedHorizon) : ""].filter(Boolean).join(" · "), "section-help"));
        if (row.citations?.length) {
          const citations = document.createElement("ul"); citations.className = "consequence-citations";
          for (const citation of row.citations) { const entry = document.createElement("li"); consequenceNameParts(entry, citation.parts, model.names); citations.append(entry); }
          item.append(citations);
        }
        list.append(item);
      }
      sectionNode.append(list);
    }
    slot.append(sectionNode);
  }
}
async function renderEventConsequences(detail, requestId, generation) {
  const request = eventConsequenceRequest({ revision: state.revision, event: detail.id, timeline: state.activeTimelineId, horizon: activeHorizon(), lens: state.readerLens });
  const slot = node("section", "", "event-consequences"); const reportStatus = node("p", "", "section-help consequence-status"); reportStatus.setAttribute("role", "status"); reportStatus.setAttribute("aria-live", "polite");
  const content = node("div", "", "consequence-content"); slot.append(node("h2", "Explicit event consequences"), reportStatus, content); el.article.append(slot);
  if (!request) { reportStatus.textContent = "Choose an author chronology to read recorded consequences."; return; }
  const key = eventConsequenceKey(request); const capturedHorizon = activeHorizon() ? { ...activeHorizon() } : null;
  const chronology = state.timelineDeclarations.find((item) => item.id === request.at.timeline);
  const horizonDescription = capturedHorizon ? horizonLabel() : `Full story — through the terminal story boundary in ${chronology ? timelineDisplayLabel(chronology) : "the selected chronology"}.`;
  const current = () => navigationIsCurrent(generation) && requestId === state.detailRequestId && state.view === "article"
    && state.selectedEntityId === request.event && state.revision === request.revision
    && eventConsequenceKey(eventConsequenceRequest({ revision: state.revision, event: detail.id, timeline: state.activeTimelineId, horizon: activeHorizon(), lens: state.readerLens }) || { revision: "", event: "", at: {}, limit: 0 }) === key;
  state.consequenceController?.abort(); const controller = new AbortController(); state.consequenceController = controller;
  reportStatus.textContent = "Loading recorded consequences…"; slot.setAttribute("aria-busy", "true");
  const failure = (outcome) => {
    content.replaceChildren(); slot.setAttribute("aria-busy", "false"); reportStatus.textContent = ({
      unavailable: "Recorded consequences are unavailable at this reading horizon.",
      limit: "The recorded consequences exceed the report limit.",
      invalid: "The consequence request could not be read.",
    })[outcome] || "Recorded consequences could not be loaded. Open this entry again to retry.";
  };
  try {
    const cached = state.eventConsequenceCache.get(key);
    let report;
    try { report = cached?.report || await api.eventConsequences(request, { signal: controller.signal }); }
    catch (error) { report = error instanceof ApiError ? error.body : null; }
    if (!current() || controller.signal.aborted) return;
    if (!eventConsequenceMatches(report, request)) { failure("error"); return; }
    if (report.outcome !== "ok") { failure(report.outcome); return; }
    renderConsequenceReport(content, report, cached?.linkedDetails || new Map(), horizonDescription, capturedHorizon);
    reportStatus.textContent = "Recorded consequences loaded."; slot.setAttribute("aria-busy", "false");
    const linkedDetails = cached?.linkedDetails || await eventConsequenceDetails(report, request, (path, options) => api.get(path, options), current, controller.signal);
    if (!current() || controller.signal.aborted) return;
    renderConsequenceReport(content, report, linkedDetails, horizonDescription, capturedHorizon);
    state.eventConsequenceCache.set(key, { report, linkedDetails });
    while (state.eventConsequenceCache.size > 64) state.eventConsequenceCache.delete(state.eventConsequenceCache.keys().next().value);
  } catch { if (current() && !controller.signal.aborted) failure("error"); }
}

async function showEntity(entity, trigger, { historyMode = "push", navigationGeneration, returnToTimeline = false, returnView = "", focus = true, searchMatch = null } = {}) {
  const generation = navigationGeneration ?? beginNavigation(); if (!navigationIsCurrent(generation)) return;
  if (state.view === "index" && historyMode === "push") { state.listScroll = el.entities.scrollTop; state.selectedEntityId = entity.id; writeHistory("replace"); }
  state.returnView = returnView || (returnToTimeline ? "timeline" : (state.returnView || "index"));
  if (!available(entity)) { state.selectedEntityId = ""; setView("article"); horizonBoundary(); if (historyMode !== "none") writeHistory(historyMode); if (focus) focusArticle(); return; }
  const requestId = ++state.detailRequestId; state.searchMatch = searchMatch; state.selectedEntityId = entity.id; setView("article"); selectButton(entity.id); renderArticleMessage(`Opening ${name(entity)}`, "Loading this lore entry…"); if (focus) focusArticle(); text(el.articleStatus, `Opening ${name(entity)}…`); busy(el.article, true);
  try { const requestRevision = state.revision; const detail = state.details.get(entity.id) || await revisionRead(() => `/api/entities/${encodeURIComponent(entity.id)}`); if (!navigationIsCurrent(generation) || requestId !== state.detailRequestId || requestRevision !== state.revision) return; state.details.set(entity.id, detail); await ensureChronologyCatalogForDetail(detail, requestId, generation); if (!navigationIsCurrent(generation) || requestId !== state.detailRequestId) return; if (detail.kind === "event") await loadEventStateKeys(requestId, generation); if (!navigationIsCurrent(generation) || requestId !== state.detailRequestId) return; renderArticle(detail); text(el.articleStatus, ""); if (detail.kind === "event") void renderEventConsequences(detail, requestId, generation); if (detail.kind === "conversation") { const horizon = activeHorizon(); try { const record = await revisionRead(() => conversationRequestPath(detail.id, horizon ? "as-of" : "all-time", horizon)); if (navigationIsCurrent(generation) && requestId === state.detailRequestId) renderConversation(record); } catch { if (navigationIsCurrent(generation) && requestId === state.detailRequestId) renderConversationError(); } } if (detail.kind === "character") await renderCharacterMoment(detail, requestId, generation); if (!navigationIsCurrent(generation) || requestId !== state.detailRequestId) return; if (historyMode !== "none") writeHistory(historyMode); if (focus) focusArticle(); }
  catch { if (navigationIsCurrent(generation) && requestId === state.detailRequestId) { renderArticleMessage("Lore unavailable", "The requested lore could not be loaded. Please try again."); text(el.articleStatus, ""); if (focus) focusArticle(); } }
  finally { if (navigationIsCurrent(generation) && requestId === state.detailRequestId) busy(el.article, false); }
}

function timelineEntry(entry) { const { at, boundary, entity, kind, lifecycleState, status, summary, location, participants } = entry; const resolved = state.registry.get(entity.id) || entity; const base = kind === "plot-transition" ? "Plot thread" : kindLabel(kind); const boundaryLabel = boundary === "point" ? (kind === "plot-transition" ? "changes" : "") : ({ start: "begins", current: "context", end: "closes" }[boundary] || ""); return { at, boundary, chronologyKind: kind, entity: resolved, lifecycleState, status, summary, location, participants, title: name(resolved), kind: `${base}${boundaryLabel ? ` ${boundaryLabel}` : ""}` }; }
function entriesFromTimeline(payload) { return chronologyEntries(payload).map(timelineEntry); }
async function loadTimeline(id) { if (!id) return []; if (state.timelineCache.has(id)) return state.timelineCache.get(id); const payload = await revisionRead(() => `/api/timeline?timeline=${encodeURIComponent(id)}`); const resolvedId = payload.timeline && payload.timeline.id ? payload.timeline.id : id; const entries = entriesFromTimeline(payload); state.timelineCache.set(resolvedId, entries); return entries; }
function horizonCoordinateKey(at) { return `${at.timeline}\u0000${at.tick}\u0000${at.order ?? 0}`; }
function populateHorizons() {
  const previous = horizonForTimeline(activeHorizon(), state.activeTimelineId);
  state.horizonEntries.clear();
  const fragment = document.createDocumentFragment(); const full = node("option", "Full story"); full.value = ""; fragment.append(full);
  // A horizon is a story coordinate, not an entity.  Several events may be
  // authored at that same coordinate, so offer it once and retain the first
  // named card as a friendly anchor.  Older history snapshots can still name
  // a now-absent anchor; matching their coordinate is the safe fallback.
  for (const entry of currentTimeline()) {
    if (!entry.at) continue;
    const key = horizonCoordinateKey(entry.at);
    if (state.horizonEntries.has(key)) continue;
    const value = { timeline: entry.at.timeline, tick: entry.at.tick, order: entry.at.order ?? 0 };
    state.horizonEntries.set(key, { at: value, title: entry.title || "this story moment" });
    const option = node("option", `Through ${entry.title || "this story moment"}`); option.value = key; fragment.append(option);
  }
  el.horizon.replaceChildren(fragment);
  const selected = [...state.horizonEntries.entries()].find(([, item]) => previous && compareStoryTime(item.at, previous) === 0);
  state.horizon = selected ? selected[1].at : null; el.horizon.value = selected ? selected[0] : ""; syncHorizonControl();
}
function loreReferenceButton(reference, fallbackKind = "entry") {
  const resolved = reference && state.registry.get(reference.id) || reference;
  const button = node("button", "", "lore-link"); button.append(characterNameNode(resolved || { kind: fallbackKind })); button.type = "button";
  if (resolved && resolved.id && state.registry.has(resolved.id)) button.addEventListener("click", () => { const returnView = state.view === "timeline" ? "timeline" : (state.view === "whereabouts" ? "whereabouts" : state.returnView); void showEntity(resolved, button, { returnView }); });
  else button.disabled = true;
  return button;
}
function namedReferences(values) {
  return (Array.isArray(values) ? values : []).map((value) => {
    const id = typeof value === "string" ? value : value && (value.character || value.entity || value.id);
    return id ? (state.registry.get(id) || (typeof value === "object" ? { ...value, id } : { id })) : null;
  }).filter(Boolean);
}
function appendTimelineSceneContext(card, entry) {
  if (entry.chronologyKind !== "scene" && entry.chronologyKind !== "event") return;
  const place = entry.location && (state.registry.get(entry.location.id) || entry.location);
  if (place) { const line = document.createElement("p"); line.className = "timeline-context"; line.append(node("span", "At "), loreReferenceButton(place, "place")); card.append(line); }
  const cast = namedReferences((entry.participants || []).filter((participant) => referencePresentAt(participant, entry.at)));
  if (cast.length) { const line = document.createElement("p"); line.className = "timeline-context"; line.append(node("span", entry.chronologyKind === "event" ? "Involving " : "With ")); cast.forEach((person, index) => { if (index) line.append(node("span", index === cast.length - 1 ? " and " : ", ")); line.append(loreReferenceButton(person, "person")); }); card.append(line); }
}
function renderTimelineCard(entry) {
  const card = document.createElement("article"); card.className = "timeline-entry"; const classification = classifyStoryMoment(entry.at, activeHorizon()); const moment = trailPositionLabel(entry.at, activeHorizon()); if (classification === "past" || classification === "current") card.classList.add("is-revealed"); if (classification === "later") card.classList.add("is-future"); if (classification === "current") card.classList.add("is-current"); const open = loreReferenceButton(entry.entity); const heading = document.createElement("h3"); heading.append(open); card.append(node("p", moment, "trail-position"), node("p", entry.kind, "article-kicker"), heading); const labels = membershipLabels(entry.entity); if (labels.length) card.append(node("p", `Narrative groups: ${labels.join(", ")}`, "thread-membership-badges")); const status = entry.lifecycleState ? `Story status: ${humanizeToken(entry.lifecycleState)}` : (entry.status && entry.status !== "canonical" ? `Record status: ${humanizeToken(entry.status)}` : "Recorded story moment"); card.append(node("p", status, "trail-status")); appendTimelineSceneContext(card, entry); if (entry.summary) card.append(node("p", authorText(String(entry.summary).replace(/^#.*\n?/, "").trim().split("\n")[0], state.registry))); return card;
}
function appendNamedPeople(line, values) { values.forEach((person, index) => { if (index) line.append(node("span", index === values.length - 1 ? " and " : ", ")); line.append(loreReferenceButton(person, "person")); }); }
function renderWhereaboutsLink() {
  const section = document.createElement("section"); section.className = "whereabouts lore-section";
  section.append(node("h3", "Where everyone is"), node("p", "Explore every person’s directly recorded place, active scene, and authored journey at this reading moment. No travel or route is inferred.", "field-help"));
  const open = node("button", "Open whereabouts"); open.type = "button"; open.addEventListener("click", () => { void openWhereabouts(); }); section.append(open);
  return section;
}
function locationButton(reference, fallback = "No place recorded") { return reference ? loreReferenceButton(state.registry.get(reference.id) || reference, "place") : node("span", fallback, "unavailable-reference"); }
function renderJourney(item, readingMoment, selectedHorizon) {
  const line = document.createElement("li");
  const from = item.from && (state.registry.get(item.from.id) || item.from); const to = item.to && (state.registry.get(item.to.id) || item.to);
  if (item.kind === "initial") { line.append(node("span", "Starting place: "), locationButton(to)); }
  else if (item.kind === "clear") { line.append(node("span", "Leaves the recorded place")); if (from) line.append(node("span", " after "), locationButton(from)); }
  else if (item.kind === "reaffirmation") { line.append(node("span", "Confirms being at "), locationButton(to)); }
  else if (to) { line.append(node("span", "Moves to "), locationButton(to)); }
  else line.append(node("span", "A location change is recorded."));
  if (item.event) { const event = state.registry.get(item.event.id) || item.event; line.append(node("span", " — "), loreReferenceButton(event, "event")); }
  const momentLabel = item.at ? (classifyStoryMoment(item.at, readingMoment) === "current" ? (selectedHorizon ? "At selected moment" : "At current authored moment") : "Earlier in the story") : "At the story’s beginning"; line.append(node("span", ` · ${momentLabel}`, "field-help"));
  return line;
}
function renderCalculatedProminence(entry, payload) {
  const section = momentSection("Calculated prominence");
  const importance = entry && entry.importance;
  if (!importance) { section.append(node("p", "Calculated prominence is unavailable for this character.", "field-help")); return section; }
  const context = activeHorizon() ? `Context: selected horizon (${horizonLabel()}).` : "Context: current authored moment from the Whereabouts projection.";
  section.append(node("p", `Score: ${Number(importance.score).toFixed(2)}`), node("p", context, "field-help"));
  const details = document.createElement("details"); details.append(node("summary", "How this calculated prominence was derived"));
  const raw = importance.raw || {}; const contributions = importance.contributions || {}; const list = document.createElement("ul");
  [["Scene appearances", "scenes"], ["Point-of-view scenes", "pointOfViewScenes"], ["Canonical event involvement", "events"], ["Relationship neighbors", "relationshipNeighbors"]].forEach(([label, key]) => list.append(node("li", `${label}: ${raw[key] || 0} (weighted contribution ${Number(contributions[key] || 0).toFixed(2)})`)));
  details.append(list, node("p", importance.explanation || "Calculated from authored evidence through this reading moment.", "field-help"), node("p", "Calculated, relative, non-canonical navigation aid; it does not change the canonical story.", "field-help")); section.append(details); return section;
}
async function appendCharacterProminence(detail, requestId, generation) {
  if (detail.status !== "canonical" && detail.status !== "retired") { el.article.append(renderCalculatedProminence(null)); return; }
  try { const payload = await loadWhereabouts(); if (!navigationIsCurrent(generation) || requestId !== state.detailRequestId) return; const entry = (payload.characters || []).find((item) => item.character && item.character.id === detail.id); el.article.append(renderCalculatedProminence(entry, payload)); }
  catch { if (navigationIsCurrent(generation) && requestId === state.detailRequestId) { const section = momentSection("Calculated prominence"); section.append(node("p", "Calculated prominence could not be loaded. Other lore on this page is still available.", "section-error")); el.article.append(section); } }
}
function renderWhereaboutsProjection(payload) {
  const fragment = document.createDocumentFragment(); fragment.append(articleHeading("Whereabouts"), node("p", "Every character is shown independently at this story moment. Places and journeys reflect only authored location state and canonical location changes; the compendium does not invent routes, travel, or collective membership.", "whereabouts-intro"));
  fragment.append(node("p", activeHorizon() ? `Reading context: ${horizonLabel()}` : "Reading context: the world’s current authored moment.", "field-help"));
  const fronts = Array.isArray(payload && payload.activeScenes) ? payload.activeScenes : [];
  const frontSection = momentSection("Active story fronts", "Each scene remains separate, even when several scenes share a place."); const frontList = document.createElement("div"); frontList.className = "whereabouts-fronts";
  for (const front of fronts) { const card = document.createElement("section"); card.className = "whereabouts-front"; const heading = document.createElement("h4"); heading.append(loreReferenceButton(state.registry.get(front.scene && front.scene.id) || front.scene, "scene")); card.append(heading); const place = document.createElement("p"); place.append(node("span", "At "), locationButton(front.location)); card.append(place); const people = namedReferences(front.characters); if (people.length) { const line = document.createElement("p"); line.append(node("span", "With ")); appendNamedPeople(line, people); card.append(line); } else card.append(node("p", "No included person is recorded in this scene.", "field-help")); frontList.append(card); }
  if (!fronts.length) frontSection.append(node("p", "No active scene is recorded at this reading moment.")); else frontSection.append(frontList); fragment.append(frontSection);
  const peopleSection = momentSection("All characters", "Open an authored journey only when you need the location changes behind a character’s current record."); const orderLabel = node("label", "Order characters by", "sort-control"); const order = document.createElement("select"); order.id = "whereabouts-sort"; state.whereaboutsSort = setSortOptions(order, whereaboutsSortConfiguration(), state.whereaboutsSort); order.addEventListener("change", () => { state.whereaboutsSort = validSort(order.value, whereaboutsSortConfiguration()); writeHistory(); el.article.replaceChildren(renderWhereaboutsProjection(payload)); el.article.querySelector("#whereabouts-sort")?.focus(); }); orderLabel.append(order); peopleSection.append(orderLabel); const peopleList = document.createElement("div"); peopleList.className = "whereabouts-people-list";
  const people = sortWhereabouts(Array.isArray(payload && payload.characters) ? payload.characters : [], state.whereaboutsSort, state.importanceByCharacter);
  for (const entry of people) {
    const card = document.createElement("section"); card.className = "whereabouts-person"; const character = state.registry.get(entry.character && entry.character.id) || entry.character; const heading = document.createElement("h4"); heading.append(loreReferenceButton(character, "character")); card.append(heading); const role = entry.role ? humanizeToken(entry.role) : ""; if (role) card.append(node("p", role, "entity-role"));
    const presence = { "active-scene": "In an active scene", offstage: "Offstage", unlocated: entry.lastKnownLocation ? "No current place recorded" : "No place has been recorded" }[entry.presence] || "Recorded presence"; card.append(node("p", presence, "whereabouts-presence"));
    const location = document.createElement("p"); if (entry.location) location.append(node("span", "At "), locationButton(entry.location)); else if (entry.lastKnownLocation) location.append(node("span", "Last recorded at "), locationButton(entry.lastKnownLocation)); else location.append(node("span", "No place has been recorded.")); card.append(location);
    if (entry.activeScene) { const scene = document.createElement("p"); scene.append(node("span", "Scene: "), loreReferenceButton(state.registry.get(entry.activeScene.id) || entry.activeScene, "scene")); card.append(scene); }
    const journey = Array.isArray(entry.journey) ? entry.journey : []; if (journey.length) { const details = document.createElement("details"); details.append(node("summary", "Authored journey")); const list = document.createElement("ul"); list.className = "whereabouts-journey"; const readingMoment = activeHorizon() || (payload && payload.effectiveTime) || null; journey.forEach((item) => list.append(renderJourney(item, readingMoment, Boolean(activeHorizon())))); details.append(list); card.append(details); }
    peopleList.append(card);
  }
  if (!peopleList.childElementCount) peopleSection.append(node("p", "No canonical or retired characters are available in this reading context.")); else peopleSection.append(peopleList); fragment.append(peopleSection); return fragment;
}
async function loadWhereabouts() { const at = activeHorizon() && { ...activeHorizon() }; const requestRevision = state.revision; const key = `${requestRevision}\u0000${at ? horizonCoordinateKey(at) : "current"}`; if (state.whereaboutsCache.has(key)) { const cached = state.whereaboutsCache.get(key); deriveImportance(cached, key); return cached; } const payload = await revisionRead(() => whereaboutsRequestPath(at)); if (state.revision !== requestRevision || `${state.revision}\u0000${activeHorizon() ? horizonCoordinateKey(activeHorizon()) : "current"}` !== key) throw new Error("Whereabouts context changed."); state.whereaboutsCache.set(key, payload); deriveImportance(payload, key); return payload; }
async function openWhereabouts({ historyMode = "push", navigationGeneration } = {}) { const generation = navigationGeneration ?? beginNavigation(); if (!navigationIsCurrent(generation)) return; const requestId = ++state.whereaboutsRequestId; setView("whereabouts"); renderArticleMessage("Opening whereabouts", "Gathering the recorded locations and journeys…"); focusLoadedFeature("Whereabouts"); text(el.articleStatus, "Gathering whereabouts…"); busy(el.article, true); let payload; try { payload = await loadWhereabouts(); } catch { if (navigationIsCurrent(generation) && requestId === state.whereaboutsRequestId) { renderArticleMessage("Whereabouts unavailable", "The recorded locations could not be loaded. Please try again."); text(el.articleStatus, ""); focusLoadedFeature("Whereabouts"); busy(el.article, false); } return; } try { if (!navigationIsCurrent(generation) || requestId !== state.whereaboutsRequestId) return; el.article.replaceChildren(renderWhereaboutsProjection(payload)); } catch { if (navigationIsCurrent(generation) && requestId === state.whereaboutsRequestId) renderLoadedFeatureFailure("Whereabouts", "recorded locations"); return; } finally { if (navigationIsCurrent(generation) && requestId === state.whereaboutsRequestId) busy(el.article, false); } if (!navigationIsCurrent(generation) || requestId !== state.whereaboutsRequestId) return; text(el.articleStatus, ""); saveLoadedFeatureHistory("Whereabouts", historyMode); focusLoadedFeature("Whereabouts"); }
function renderPossibilities(payload) { const fragment = document.createDocumentFragment(); fragment.append(articleHeading("Possibilities"), node("p", "These are author hypotheses: non-canonical notes that do not enter the timeline, author horizon, state, causality, whereabouts, or character context.", "whereabouts-intro")); const orderLabel = node("label", "Order possibilities by", "sort-control"); const order = document.createElement("select"); order.id = "possibilities-sort"; state.possibilitiesSort = setSortOptions(order, possibilitiesSortConfiguration(), state.possibilitiesSort); order.addEventListener("change", () => { state.possibilitiesSort = validSort(order.value, possibilitiesSortConfiguration()); writeHistory(); el.article.replaceChildren(renderPossibilities(payload)); el.article.querySelector("#possibilities-sort")?.focus(); }); orderLabel.append(order); fragment.append(orderLabel); const items = sortPossibilities(Array.isArray(payload && payload.hypotheses) ? payload.hypotheses : [], state.possibilitiesSort); const names = (values) => (Array.isArray(values) ? values : []).map((value) => authorText(value && value.title ? value.title : "Unavailable reference", state.registry)).filter(Boolean); if (!items.length) fragment.append(node("p", "No possibilities have been recorded.")); for (const item of items) { const card = document.createElement("section"); card.className = "lore-section"; card.append(node("h3", authorText(item.title || "Untitled possibility", state.registry)), node("p", `Status: ${humanizeToken(item.status || "open")} · Non-canonical`, "article-kicker"), node("p", authorText(item.statement || "", state.registry))); const subjects = names(item.subjects); if (subjects.length) card.append(node("p", `About: ${subjects.join(", ")}`, "field-help")); if (Array.isArray(item.alternatives) && item.alternatives.length) { const list = document.createElement("ul"); for (const alternative of item.alternatives) list.append(node("li", authorText(alternative, state.registry))); card.append(node("h4", "Possible readings"), list); } if (item.context) card.append(node("p", authorText(item.context, state.registry), "field-help")); const placement = item.placement || {}; const placed = [placement.scene, placement.event, placement.location].filter(Boolean).map((value) => authorText(value.title || "Unavailable reference", state.registry)); if (placed.length || placement.context) card.append(node("p", `${placed.length ? `Context: ${placed.join(", ")}` : "Context"}${placement.context ? ` — ${authorText(placement.context, state.registry)}` : ""}`, "field-help")); const resolution = item.resolution || {}; const adopted = names(resolution.canonicalRecords); if (adopted.length) card.append(node("p", `Already addressed in canon: ${adopted.join(", ")}.`, "field-help")); if (item.status === "rejected" && resolution.note) card.append(node("p", `Retained rejection note: ${authorText(resolution.note, state.registry)}`, "field-help")); fragment.append(card); } return fragment; }
async function refreshSearchForFullStory(navigationGeneration) {
  if (!state.query) { const source = indexSource(); renderEntities(source); updateIndexStatus(source); return true; }
  busy(el.entities, true); text(el.entitiesStatus, "Refreshing search for the full story…");
  try {
    const refreshed = await refreshAuthorSearch(() => revisionRead(() => authorSearchRequestPath(state.query, activeHorizon(), selectedThreadIds())), (payload) => presentSearchResults(payload, state.registry), state.searchResults);
    if (!navigationIsCurrent(navigationGeneration)) return false;
    state.searchResults = refreshed.results;
    state.searchError = refreshed.error ? "Search results from the earlier reading context are still shown. The search could not refresh for the full story." : "";
    const source = indexSource(); renderEntities(source); updateIndexStatus(source);
    return true;
  } finally { if (navigationIsCurrent(navigationGeneration)) busy(el.entities, false); }
}
async function openPossibilities({ historyMode = "push", navigationGeneration } = {}) { const generation = navigationGeneration ?? beginNavigation(); if (!navigationIsCurrent(generation)) return; const requestId = ++state.possibilitiesRequestId; const hadHorizon = Boolean(activeHorizon()); state.horizon = null; state.importanceByCharacter = new Map(); state.importanceLoaded = false; state.importanceError = ""; setView("possibilities"); renderArticleMessage("Opening possibilities", "Gathering non-canonical author notes…"); focusLoadedFeature("Possibilities"); busy(el.article, true); if (hadHorizon && !await refreshSearchForFullStory(generation)) { if (navigationIsCurrent(generation) && requestId === state.possibilitiesRequestId) busy(el.article, false); return; } let payload; try { payload = await revisionRead("/api/hypotheses"); } catch { if (navigationIsCurrent(generation) && requestId === state.possibilitiesRequestId) { renderArticleMessage("Possibilities unavailable", "The non-canonical author notes could not be loaded. Please try again."); text(el.articleStatus, ""); focusLoadedFeature("Possibilities"); } if (navigationIsCurrent(generation) && requestId === state.possibilitiesRequestId) busy(el.article, false); return; } try { if (!navigationIsCurrent(generation) || requestId !== state.possibilitiesRequestId) return; el.article.replaceChildren(renderPossibilities(payload)); } catch { if (navigationIsCurrent(generation) && requestId === state.possibilitiesRequestId) renderLoadedFeatureFailure("Possibilities", "non-canonical author notes"); return; } finally { if (navigationIsCurrent(generation) && requestId === state.possibilitiesRequestId) busy(el.article, false); } if (!navigationIsCurrent(generation) || requestId !== state.possibilitiesRequestId) return; text(el.articleStatus, ""); saveLoadedFeatureHistory("Possibilities", historyMode); focusLoadedFeature("Possibilities"); }
function renderTimeline(id) {
  state.activeTimelineId = id; setView("timeline"); const fragment = document.createDocumentFragment(); fragment.append(articleHeading("Story trail"), node("p", "Story beats are shown in chronological order. Every stop is equally spaced; the gaps do not indicate elapsed duration.", "timeline-intro")); const origin = originLabel(state.timelineDeclarations, id); if (origin) fragment.append(node("p", `The trail begins at ${origin}.`, "field-help"));
  const tabs = document.createElement("div"); tabs.className = "timeline-tabs"; for (const timeline of state.timelineDeclarations) { const tab = node("button", timelineDisplayLabel(timeline)); tab.type = "button"; tab.setAttribute("aria-pressed", String(timeline.id === id)); tab.addEventListener("click", () => { void openTimeline(timeline.id); }); tabs.append(tab); } fragment.append(tabs);
  if (state.membershipError) { const recovery = node("section", "", "thread-membership-recovery"); recovery.setAttribute("role", "status"); recovery.setAttribute("aria-live", "polite"); recovery.append(node("p", state.membershipError, "section-error")); const retry = node("button", "Retry narrative groups", "thread-membership-retry"); retry.type = "button"; retry.addEventListener("click", () => { void refreshMembershipView(); }); recovery.append(retry); fragment.append(recovery); }
  fragment.append(renderWhereaboutsLink());
  const selectedEntries = currentTimeline().filter((entry) => isSelectedMember(entry.entity)); const rail = document.createElement("div"); rail.className = "timeline-rail"; for (const group of timelinePresentationGroups(selectedEntries, activeHorizon())) { if (group.type !== "entry") { const container = document.createElement("section"); const isFronts = group.type === "concurrent-scenes"; container.className = `${isFronts ? "timeline-concurrent-scenes" : "meanwhile-group"} is-${group.classification}`; if (group.classification !== "later") container.classList.add("is-revealed"); if (group.classification === "current") container.classList.add("is-current"); container.setAttribute("aria-label", isFronts ? "Parallel story fronts" : "Meanwhile, at the same story moment"); if (!isFronts) container.append(node("p", "Meanwhile", "meanwhile-label")); const cards = document.createElement("div"); cards.className = "meanwhile-cards"; cards.append(...group.entries.map(renderTimelineCard)); container.append(cards); rail.append(container); } else rail.append(renderTimelineCard(group.entries[0])); } if (!selectedEntries.length) rail.append(node("p", state.membershipError || (selectedThreadIds().length ? "No story beats match the selected narrative groups." : "No dated story beats have been authored on this timeline yet."), state.membershipError ? "section-error" : "")); fragment.append(rail); el.article.replaceChildren(fragment); text(el.articleStatus, state.membershipError || ""); focusArticle();
}
async function openTimeline(id = "", { historyMode = "push", navigationGeneration } = {}) { const generation = navigationGeneration ?? beginNavigation(); if (!navigationIsCurrent(generation)) return; const requestId = ++state.timelineRequestId; const selected = selectedTimelineId(id, state.timelineDefault, state.timelineDeclarations); setView("timeline"); renderArticleMessage("Opening story trail", "Gathering the story timeline…"); focusArticle(); text(el.articleStatus, "Gathering the story timeline…"); busy(el.article, true); try { await loadTimeline(selected); if (!navigationIsCurrent(generation) || requestId !== state.timelineRequestId) return; state.activeTimelineId = selected; await loadMembershipProjection(currentTimeline().map((entry) => entry.entity && entry.entity.id), generation); if (!navigationIsCurrent(generation) || requestId !== state.timelineRequestId) return; state.importanceLoaded = false; if (needsImportance(selected)) await ensureImportance(); if (!navigationIsCurrent(generation) || requestId !== state.timelineRequestId) return; renderTimeline(selected); if (historyMode !== "none") writeHistory(historyMode); } catch { if (navigationIsCurrent(generation) && requestId === state.timelineRequestId) { renderArticleMessage("Timeline unavailable", "The requested lore could not be loaded. Please try again."); text(el.articleStatus, ""); focusArticle(); } } finally { if (navigationIsCurrent(generation) && requestId === state.timelineRequestId) busy(el.article, false); } }

function indexSource() { const source = state.query ? filterSearchResultsByKind(state.searchResults || [], state.searchKind) : state.entities.filter((entity) => !state.kind || entity.kind === state.kind); return sortIndex(source, state.indexSort, state.importanceByCharacter); }
async function loadMembershipProjection(candidateIds, generation = null) {
  const selected = selectedThreadIds(); const candidates = normalizedThreadIds(candidateIds); const revision = state.revision;
  const key = [revision, selected.join("\u0000"), candidates.join("\u0000")].join("\u0001"); const requestId = ++state.membershipRequestId;
  const current = () => requestId === state.membershipRequestId && revision === state.revision && key === [state.revision, selectedThreadIds().join("\u0000"), normalizedThreadIds(candidateIds).join("\u0000")].join("\u0001") && (generation === null || navigationIsCurrent(generation));
  if (!selected.length || !candidates.length) { if (current()) { state.membershipMap = new Map(); state.membershipError = ""; } return current(); }
  if (state.membershipCache.has(key)) { if (current()) { state.membershipMap = state.membershipCache.get(key); state.membershipError = ""; } return current(); }
  try {
    const payloads = await Promise.all(threadMembershipRequestPaths(candidates, selected).map((path) => revisionRead(path)));
    if (payloads.some((payload) => !payload || payload.revision !== revision)) throw new Error("revision mismatch");
    const projected = new Map();
    for (const payload of payloads) for (const record of Array.isArray(payload.records) ? payload.records : []) if (record && typeof record.recordId === "string" && Array.isArray(record.threadIds)) projected.set(record.recordId, record.threadIds.filter((id) => selected.includes(id)).sort());
    if (!current()) return false;
    state.membershipCache.set(key, projected); state.membershipMap = projected; state.membershipError = ""; return true;
  } catch { if (!current()) return false; state.membershipMap = new Map(); state.membershipError = "Narrative group projection is unavailable; the selected view is closed. Retry when the connection is restored."; return false; }
}
async function refreshMembershipView() { const generation = beginNavigation(); const candidates = state.view === "timeline" ? currentTimeline().map((entry) => entry.entity && entry.entity.id) : indexSource().map((item) => (item.entity || item).id); await loadMembershipProjection(candidates, generation); if (!navigationIsCurrent(generation)) return; if (state.view === "timeline") renderTimeline(state.activeTimelineId); else renderIndex({ navigationGeneration: generation }); writeHistory(); }
function renderIndex({ focus = false, navigationGeneration } = {}) { const generation = navigationGeneration ?? beginNavigation(); if (!navigationIsCurrent(generation)) return; setView("index"); el.entityText.value = state.query; el.searchKind.value = state.searchKind; syncThreadFilterAvailability(); syncIndexSort(); updateIndexHeading(); const source = indexSource(); renderEntities(source); updateIndexStatus(source); if (needsImportance() && !state.importanceLoaded) void ensureImportance().then(() => { if (navigationIsCurrent(generation) && state.view === "index") renderIndex({ navigationGeneration: generation }); }); if (state.importanceError && needsImportance()) text(el.entitiesStatus, state.importanceError); requestAnimationFrame(() => { if (!navigationIsCurrent(generation)) return; el.entities.scrollTop = state.listScroll; if (focus) { const selected = el.entities.querySelector('[aria-pressed="true"]'); (selected || el.entityText).focus(); } }); }
function browse(kind, label) { const generation = beginNavigation(); Object.assign(state, historyAction(state, { type: "browse", kind })); state.searchKind = kind; state.indexSort = ""; state.returnView = "index"; state.searchError = ""; state.searchResults = null; state.listScroll = 0; renderThreadFilter(); text(el.indexHeading, label); void loadMembershipProjection(indexSource().map((item) => (item.entity || item).id), generation).then(() => { if (navigationIsCurrent(generation)) renderIndex({ focus: isMobileLayout(), navigationGeneration: generation }); }); writeHistory("push"); }
async function returnToIndex() { const generation = beginNavigation(); Object.assign(state, historyAction(state, { type: "back-to-list" })); if (state.query && state.threadSearchNeedsRefresh) { busy(el.entities, true); text(el.entitiesStatus, "Refreshing search for selected narrative groups…"); const refreshed = await refreshAuthorSearch(() => revisionRead(() => authorSearchRequestPath(state.query, activeHorizon(), selectedThreadIds())), (payload) => presentSearchResults(payload, state.registry), null); if (!navigationIsCurrent(generation)) return; state.searchResults = refreshed.results; state.searchError = refreshed.error ? "The search could not refresh for the selected narrative groups. Retry when the connection is restored." : ""; state.threadSearchNeedsRefresh = false; } await loadMembershipProjection(indexSource().map((item) => (item.entity || item).id), generation); if (!navigationIsCurrent(generation)) return; busy(el.entities, false); renderIndex({ focus: true, navigationGeneration: generation }); writeHistory("push"); }
async function search({ preserveResults = false } = {}) { const generation = beginNavigation(); const query = el.entityText.value.trim(); const kind = el.searchKind.value; state.indexSort = ""; const priorResults = preserveResults && query === state.query ? state.searchResults : null; state.query = query; renderThreadFilter(); state.kind = kind; state.searchKind = kind; state.searchError = ""; state.searchResults = priorResults; state.listScroll = 0; busy(el.entities, true); text(el.entitiesStatus, "Searching lore…"); try { const refreshed = state.query ? await refreshAuthorSearch(() => revisionRead(() => authorSearchRequestPath(state.query, activeHorizon(), selectedThreadIds())), (payload) => presentSearchResults(payload, state.registry), priorResults) : { error: null, results: null }; if (!navigationIsCurrent(generation)) return; state.searchResults = refreshed.results; state.searchError = refreshed.error ? "The search could not be loaded. Retry search when the connection is restored." : ""; syncIndexSort(); updateIndexHeading(); syncNavigation(); const source = indexSource(); await loadMembershipProjection(source.map((item) => (item.entity || item).id), generation); if (!navigationIsCurrent(generation)) return; renderEntities(source); updateIndexStatus(source); writeHistory(); } finally { if (navigationIsCurrent(generation)) busy(el.entities, false); } }
async function changeHorizon() { const generation = beginNavigation(); const selected = state.horizonEntries.get(el.horizon.value); state.horizon = selected ? selected.at : null; state.importanceByCharacter = new Map(); state.importanceError = ""; state.importanceLoaded = false; state.listScroll = el.entities.scrollTop; if (state.query) { busy(el.entities, true); text(el.entitiesStatus, "Refreshing search for this story moment…"); const refreshed = await refreshAuthorSearch(() => revisionRead(() => authorSearchRequestPath(state.query, activeHorizon(), selectedThreadIds())), (payload) => presentSearchResults(payload, state.registry), state.searchResults); if (!navigationIsCurrent(generation)) return; state.searchResults = refreshed.results; state.searchError = refreshed.error ? "Search results from the earlier reading context are still shown. The search could not refresh for this story moment." : ""; if (navigationIsCurrent(generation)) busy(el.entities, false); } if (!navigationIsCurrent(generation)) return; if (state.view === "possibilities") { writeHistory(); return; } await loadMembershipProjection((state.view === "timeline" ? currentTimeline() : indexSource()).map((item) => (item.entity || item).id), generation); if (!navigationIsCurrent(generation)) return; const source = indexSource(); renderEntities(source); updateIndexStatus(source); if (state.view === "whereabouts") { await openWhereabouts({ historyMode: "none", navigationGeneration: generation }); if (navigationIsCurrent(generation)) writeHistory(); return; } if (state.view === "timeline") { if (needsImportance(state.activeTimelineId)) await ensureImportance(); if (!navigationIsCurrent(generation)) return; renderTimeline(state.activeTimelineId); } else if (state.view === "article") { const entry = state.registry.get(state.selectedEntityId); if (entry) await showEntity(entry, null, { historyMode: "replace", navigationGeneration: generation, returnView: state.returnView, focus: false }); } else renderIndex({ navigationGeneration: generation }); if (navigationIsCurrent(generation)) writeHistory(); }
function activateNav(button) { for (const item of el.nav.querySelectorAll(".nav-item")) item.removeAttribute("aria-current"); button.setAttribute("aria-current", "page"); }
async function restore(snapshot) {
  const requestId = ++state.restoreRequestId; const generation = beginNavigation(); const currentRestore = () => navigationIsCurrent(generation) && requestId === state.restoreRequestId;
  const restored = restoreNavigation(snapshot, { timelineId: state.activeTimelineId }); const known = state.timelineDeclarations.some((item) => item.id === restored.timelineId);
  Object.assign(state, { kind: restored.kind, mobilePane: restored.mobilePane, query: restored.query, searchKind: restored.searchKind || "", selectedThreadIds: normalizedThreadIds(restored.threadIds).filter((id) => catalogThreadIds().has(id)), indexSort: validSort(restored.indexSort, indexSortConfiguration({ query: restored.query, kind: restored.searchKind || restored.kind })), whereaboutsSort: validSort(restored.whereaboutsSort, whereaboutsSortConfiguration()), possibilitiesSort: validSort(restored.possibilitiesSort, possibilitiesSortConfiguration()), horizon: restored.horizon, activeTimelineId: known ? restored.timelineId : state.timelineDefault, returnView: restored.returnView, selectedEntityId: restored.entryId, listScroll: restored.listScroll, importanceByCharacter: new Map(), importanceLoaded: false, importanceError: "", searchError: "", searchMatch: null, searchResults: null });
  renderThreadFilter();
  if (state.activeTimelineId) { await loadTimeline(state.activeTimelineId); if (!currentRestore()) return; populateHorizons(); }
  if (state.query) { const refreshed = await refreshAuthorSearch(() => revisionRead(() => authorSearchRequestPath(state.query, activeHorizon(), selectedThreadIds())), (payload) => presentSearchResults(payload, state.registry)); if (!currentRestore()) return; state.searchResults = refreshed.results; state.searchError = refreshed.error ? "The saved search could not be restored. Retry search when the connection is restored." : ""; if (!currentRestore()) return; }
  await loadMembershipProjection((restored.view === "timeline" ? currentTimeline() : indexSource()).map((item) => (item.entity || item).id), generation); if (!currentRestore()) return;
  if (restored.mobilePane === "nav") { setView("index", "nav"); requestAnimationFrame(() => { if (navigationIsCurrent(generation)) el.nav.querySelector(".nav-item")?.focus(); }); return; }
  if (restored.view === "possibilities") { await openPossibilities({ historyMode: "none", navigationGeneration: generation }); return; }
  if (restored.view === "chronology") { await openChronology({ historyMode: "none", navigationGeneration: generation }); return; }
  if (restored.view === "whereabouts") { await openWhereabouts({ historyMode: "none", navigationGeneration: generation }); return; }
  if (restored.view === "timeline") { if (needsImportance(state.activeTimelineId)) await ensureImportance(); if (!currentRestore()) return; renderTimeline(state.activeTimelineId); return; }
  if (restored.view === "article") { const entry = state.registry.get(restored.entryId); if (entry) { await showEntity(entry, null, { historyMode: "none", navigationGeneration: generation, returnView: restored.returnView }); return; } }
  renderIndex({ focus: true, navigationGeneration: generation });
}
function declaredTimelineId(requested, defaultTimeline, declarations) { return declarations.some((item) => item.id === requested) ? requested : (declarations.some((item) => item.id === defaultTimeline) ? defaultTimeline : (declarations[0] && declarations[0].id) || ""); }
async function revisionSnapshot(expectedRevision = "") { for (let attempt = 0; attempt < 3; attempt += 1) { const before = await api.get("/api/status"); const revision = before && before.revision || expectedRevision || ""; const entities = await api.get("/api/entities"); let threadCatalog; let threadCatalogError = ""; try { threadCatalog = await api.get("/api/threads"); if (revision && (!threadCatalog || threadCatalog.revision !== revision)) continue; } catch { threadCatalog = { revision, groupingAvailable: false, threads: [] }; threadCatalogError = "Narrative group filters are unavailable; author search remains unfiltered."; } const after = await api.get("/api/status"); if (revision && after && after.revision && after.revision !== revision) continue; const declarations = (after.timeModel && after.timeModel.timelineDeclarations) || []; const defaultTimeline = (after.timeModel && after.timeModel.defaultTimeline) || ""; const activeTimelineId = declaredTimelineId(state.activeTimelineId, defaultTimeline, declarations); let timelinePayload = null; if (activeTimelineId) { timelinePayload = await api.get(`/api/timeline?timeline=${encodeURIComponent(activeTimelineId)}`); if (revision && timelinePayload && timelinePayload.revision && timelinePayload.revision !== revision) continue; const confirmed = await api.get("/api/status"); if (revision && confirmed && confirmed.revision && confirmed.revision !== revision) continue; } return { revision, status: after, entities, declarations, defaultTimeline, activeTimelineId, timelinePayload, threadCatalog, threadCatalogError }; } return null; }
async function refreshRevision(revision = "", generation = null) { const snapshot = await revisionSnapshot(revision); if (!snapshot || (generation !== null && !navigationIsCurrent(generation))) return false; const previousThreadIds = selectedThreadIds(); state.membershipRequestId += 1; state.revision = snapshot.revision; state.details.clear(); state.consequenceController?.abort(); state.eventConsequenceCache.clear(); state.worldStateKeys = null; state.timelineCache.clear(); state.whereaboutsCache.clear(); state.membershipCache.clear(); state.membershipMap = new Map(); state.membershipError = ""; state.importanceByCharacter = new Map(); state.importanceError = ""; state.importanceLoaded = false; state.searchResults = null; state.entities = snapshot.entities; state.registry = createEntityRegistry(snapshot.entities); state.threadCatalog = snapshot.threadCatalog; state.threadCatalogError = snapshot.threadCatalogError; state.selectedThreadIds = previousThreadIds.filter((id) => catalogThreadIds().has(id)); state.threadSelectionRevisionChanged = previousThreadIds.join("\u0000") !== selectedThreadIds().join("\u0000"); state.threadSearchNeedsRefresh = previousThreadIds.length > 0; state.timelineDeclarations = snapshot.declarations; state.timelineDefault = snapshot.defaultTimeline; state.activeTimelineId = snapshot.activeTimelineId; state.horizon = horizonForTimeline(state.horizon, state.activeTimelineId); state.horizonEntries.clear(); renderThreadFilter(); if (snapshot.timelinePayload) { state.timelineCache.set(state.activeTimelineId, entriesFromTimeline(snapshot.timelinePayload)); populateHorizons(); } else { el.horizon.replaceChildren(node("option", "Full story")); syncHorizonControl(); } if (!state.registry.has(state.selectedEntityId)) { state.selectedEntityId = ""; state.returnView = "index"; } return true; }
async function handleRevision(revision) {
  const view = state.view; const selectedId = state.selectedEntityId; const generation = beginNavigation(); state.whereaboutsRequestId += 1; state.possibilitiesRequestId += 1;
  if (!await refreshRevision(revision, generation) || !navigationIsCurrent(generation)) return false;
  if (state.query && state.threadSearchNeedsRefresh) { const refreshed = await refreshAuthorSearch(() => revisionRead(() => authorSearchRequestPath(state.query, activeHorizon(), selectedThreadIds())), (payload) => presentSearchResults(payload, state.registry)); if (!navigationIsCurrent(generation)) return false; state.searchResults = refreshed.results; state.searchError = refreshed.error ? "The search could not refresh after narrative groups changed." : ""; }
  const candidates = view === "timeline" ? currentTimeline().map((entry) => entry.entity && entry.entity.id) : indexSource().map((item) => (item.entity || item).id);
  await loadMembershipProjection(candidates, generation); if (!navigationIsCurrent(generation)) return false;
  if (view === "chronology") { state.chronologyCatalog = null; state.chronologyFormatCache.clear(); await openChronology({ historyMode: "none", navigationGeneration: generation }); return navigationIsCurrent(generation); }
  if (view === "whereabouts") { await openWhereabouts({ historyMode: "none", navigationGeneration: generation }); return navigationIsCurrent(generation); }
  if (view === "timeline") { if (needsImportance(state.activeTimelineId)) await ensureImportance(); if (!navigationIsCurrent(generation)) return false; renderTimeline(state.activeTimelineId); return true; }
  if (view === "article") { const entity = state.registry.get(selectedId); if (entity) { await showEntity(entity, null, { historyMode: "none", navigationGeneration: generation, returnView: state.returnView, focus: false }); return navigationIsCurrent(generation); } renderArticleMessage("Lore changed", "The selected entry is no longer available in this revision."); }
  renderIndex({ navigationGeneration: generation }); return navigationIsCurrent(generation);
}
function connectRevisionChannel() { if (!globalThis.WebSocket) return; let socket; try { socket = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws?token=${encodeURIComponent(api.sessionToken())}`); socket.addEventListener("message", (event) => { try { const message = JSON.parse(event.data); const revision = message.revision || (message.compile && message.compile.revision) || (message.status && message.status.revision) || message.head; if (revision && revision !== state.revision) void handleRevision(revision); } catch {} }); } catch {} }
function consumeSpatialLoreHandoff() {
  try {
    const id = globalThis.sessionStorage?.getItem("wedl.spatial.lore.once");
    globalThis.sessionStorage?.removeItem("wedl.spatial.lore.once");
    return typeof id === "string" && id.length > 0 && id.length <= 256 && !/[\u0000-\u001f\u007f]/.test(id) ? id : "";
  } catch { return ""; }
}
async function boot() { const bootGeneration = navigation.begin(); const session = await api.get("/api/session"); api.setToken(session.token); await refreshRevision(); const handoffId = consumeSpatialLoreHandoff(); const world = state.entities.find((entity) => entity.kind === "world"); text(el.worldName, world ? name(world) : "Your world"); text(el.appStatus, `${state.entities.length} lore entries ready`); busy(el.entities, false); connectRevisionChannel(); if (!navigationIsCurrent(bootGeneration)) return; const handedOffEntity = state.registry.get(handoffId); if (handedOffEntity) { await showEntity(handedOffEntity, null, { navigationGeneration: bootGeneration }); return; } await restore(history.state); }

el.entities.addEventListener("scroll", () => { state.listScroll = el.entities.scrollTop; }); el.entitiesForm.addEventListener("submit", (event) => { event.preventDefault(); void search(); }); el.indexSort.addEventListener("change", () => { state.indexSort = validSort(el.indexSort.value, indexSortConfiguration({ query: state.query, kind: state.searchKind || state.kind })); state.listScroll = 0; renderIndex({ navigationGeneration: navigation.begin() }); writeHistory(); }); el.searchKind.addEventListener("change", () => { state.searchKind = el.searchKind.value; state.kind = state.searchKind; state.indexSort = ""; updateIndexHeading(); syncNavigation(); void refreshMembershipView(); writeHistory(); }); el.retrySearch.addEventListener("click", () => { if (state.membershipError) void refreshMembershipView(); else void search({ preserveResults: true }); }); el.horizon.addEventListener("change", () => { void changeHorizon(); });
el.mobileBack.addEventListener("click", () => { beginNavigation(); state.listScroll = el.entities.scrollTop; Object.assign(state, historyAction(state, { type: "open-navigation" })); setView("index", "nav"); writeHistory("push"); requestAnimationFrame(() => el.nav.querySelector(".nav-item")?.focus()); });
el.back.addEventListener("click", () => { if (state.view === "article" && state.returnView === "timeline") void openTimeline(state.activeTimelineId); else if (state.view === "article" && state.returnView === "whereabouts") void openWhereabouts(); else void returnToIndex(); });
for (const button of el.nav.querySelectorAll(".nav-item")) button.addEventListener("click", () => { activateNav(button); if (button.dataset.view === "timeline") void openTimeline(); else if (button.dataset.view === "whereabouts") void openWhereabouts(); else if (button.dataset.view === "possibilities") void openPossibilities(); else if (button.dataset.view === "chronology") void openChronology(); else browse(button.dataset.kind || "", button.textContent); }); window.addEventListener("popstate", (event) => { void restore(event.state); });
boot().catch(() => { text(el.appStatus, "The compendium could not be opened."); text(el.entitiesStatus, "Lore is unavailable until the connection is restored."); busy(el.entities, false); });
