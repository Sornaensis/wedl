import { createApiClient } from "./api.js";
import { characterKnowledgeRequestPath, conversationRequestPath, entityStateRequestPath, authorSearchRequestPath, whereaboutsRequestPath } from "./query.mjs";
import { authorText, buildLoreArticle, conversationTranscriptBeats, createEntityRegistry, humanizeToken, kindLabel, safeDisplayName } from "./lore.mjs";
import { filterSearchResultsByKind, presentSearchResults, refreshAuthorSearch } from "./search.mjs";
import { clearSupersededNavigationLoading, createNavigationGeneration, historyAction, navigationSnapshot, restoreNavigation } from "./navigation.mjs";
import { chronologyEntries, classifyStoryMoment, compareStoryTime, horizonForTimeline, isAtOrBeforeHorizon, isEntityAvailable, originLabel, referencePresentAt, selectedTimelineId, timelineDisplayLabel, timelinePresentationGroups, trailPositionLabel } from "./timeline.mjs";

const el = {
  appStatus: document.querySelector("#app-status"), article: document.querySelector("#entity-detail-content"), articleStatus: document.querySelector("#article-status"), back: document.querySelector("#back-to-timeline"),
  entities: document.querySelector("#entities"), entitiesForm: document.querySelector("#entities-form"), entitiesStatus: document.querySelector("#entities-status"), entityText: document.querySelector("#entity-text"), searchKind: document.querySelector("#search-kind"), horizon: document.querySelector("#author-horizon"), retrySearch: document.querySelector("#retry-search"), searchRecovery: document.querySelector("#search-recovery"),
  indexHeading: document.querySelector("#entries-heading"), mobileBack: document.querySelector("#back-to-navigation"), nav: document.querySelector(".compendium-nav"), shell: document.querySelector(".compendium-shell"), worldName: document.querySelector("#world-name"),
};
const api = createApiClient();
const state = { activeTimelineId: "", detailRequestId: 0, details: new Map(), entities: [], horizon: null, horizonEntries: new Map(), kind: "", mobilePane: "nav", query: "", registry: new Map(), restoreRequestId: 0, returnView: "index", searchError: "", searchKind: "", searchMatch: null, searchResults: null, selectedEntityId: "", timelineCache: new Map(), timelineDeclarations: [], timelineDefault: "", timelineRequestId: 0, view: "index", whereaboutsCache: new Map(), whereaboutsRequestId: 0, possibilitiesRequestId: 0, listScroll: 0 };
const navigation = createNavigationGeneration();

const text = (node, value) => { node.textContent = value; };
const node = (tag, value, className = "") => { const valueNode = document.createElement(tag); valueNode.textContent = value; if (className) valueNode.className = className; return valueNode; };
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
function beginNavigation() { invalidateDetail(); invalidateTimeline(); clearSupersededNavigationLoading(el); return navigation.begin(); }
function navigationIsCurrent(generation) { return navigation.isCurrent(generation); }
function syncNavigation() { for (const item of el.nav.querySelectorAll(".nav-item")) item.removeAttribute("aria-current"); const active = [...el.nav.querySelectorAll(".nav-item")].find((item) => item.dataset.view ? item.dataset.view === state.view : (state.view === "index" && item.dataset.kind === state.kind)); if (active) active.setAttribute("aria-current", "page"); }
function syncHorizonControl() {
  const applicable = state.view !== "possibilities" && state.horizonEntries.size > 0;
  const selected = [...state.horizonEntries.entries()].find(([, entry]) => state.horizon && compareStoryTime(entry.at, state.horizon) === 0);
  el.horizon.value = applicable && selected ? selected[0] : "";
  el.horizon.disabled = !applicable;
}
function setView(view, mobilePane = view === "article" || view === "timeline" || view === "whereabouts" || view === "possibilities" ? "article" : "index") { state.view = view; state.mobilePane = mobilePane; el.shell.classList.toggle("detail-open", mobilePane === "article"); el.shell.classList.toggle("mobile-nav", mobilePane === "nav"); el.shell.classList.toggle("mobile-index", mobilePane === "index"); el.back.hidden = view === "index"; text(el.back, view === "article" && state.returnView === "timeline" ? "Back to timeline" : (view === "article" && state.returnView === "whereabouts" ? "Back to whereabouts" : "Back to list")); syncHorizonControl(); syncNavigation(); }
function focusArticle() { const target = el.article.querySelector("#article-heading") || el.article; target.focus(); }
function focusLoadedFeature(feature) { try { focusArticle(); } catch { text(el.articleStatus, `${feature} loaded, but the reading heading could not receive focus. You can continue reading below.`); } }
function saveLoadedFeatureHistory(feature, mode) { if (mode === "none") return; try { writeHistory(mode); } catch { text(el.articleStatus, `${feature} loaded, but the browser could not save this navigation step. You can continue reading or use the navigation links.`); } }
function renderLoadedFeatureFailure(feature, collection) { try { renderArticleMessage(`${feature} display problem`, `The ${collection} were loaded, but this page could not display them. Return to the list and try again.`); text(el.articleStatus, ""); focusLoadedFeature(feature); } catch { text(el.articleStatus, `${feature} loaded, but this page could not display the result. Return to the list and try again.`); } }
function isMobileLayout() { return window.matchMedia("(max-width: 719px)").matches; }
function renderArticleMessage(title, message) { el.article.replaceChildren(articleHeading(title), node("p", message)); }
function selectButton(id) { for (const button of el.entities.querySelectorAll(".entity-select")) button.setAttribute("aria-pressed", String(button.dataset.entityId === id)); }
function updateIndexStatus(source) { const count = source.filter((item) => available(item.entity || item)).length; const failed = Boolean(state.searchError); text(el.entitiesStatus, failed ? state.searchError : (count ? `${count} matching entries` : "No matching entries in this reading context.")); el.entitiesStatus.classList.toggle("section-error", failed); el.searchRecovery.hidden = !failed; }
function updateIndexHeading() { const button = [...el.nav.querySelectorAll(".nav-item")].find((item) => item.dataset.kind === state.kind); text(el.indexHeading, state.kind ? (button ? button.textContent : "All lore") : "All lore"); }

function renderEntities(entities) {
  const fragment = document.createDocumentFragment();
  for (const candidate of entities) {
    const entity = candidate && candidate.entity ? candidate.entity : candidate;
    if (!entity || !available(entity)) continue;
    const match = candidate && candidate.match;
    const item = document.createElement("article"); const button = document.createElement("button"); item.className = "entity"; button.type = "button"; button.className = "entity-select"; button.dataset.entityId = entity.id;
    button.setAttribute("aria-controls", "entity-detail-content"); button.setAttribute("aria-pressed", String(entity.id === state.selectedEntityId)); button.append(node("span", name(entity), "entity-title"), node("span", kindLabel(entity.kind), "entity-meta")); const role = cachedCharacterRole(entity); if (role) button.append(node("span", role, "entity-role"));
    if (match && match.heading) button.append(node("span", `Found in ${match.heading}`, "entity-match"));
    if (match && match.snippet) button.append(node("span", match.snippet, "entity-snippet"));
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
  if (entity && available(entity)) { const button = node("button", name(entity), "lore-link"); button.type = "button"; button.addEventListener("click", () => { void showEntity(entity, button); }); item.append(button); } else item.append(node("span", "Unavailable reference", "unavailable-reference"));
  for (const prose of [description, summary]) if (prose) item.append(node("span", authorText(prose, state.registry), "reference-detail"));
  if (role) item.append(node("span", humanizeToken(role), "reference-role")); return item;
}
function section(model) {
  const container = document.createElement("section"); container.className = "lore-section"; if (model.title) container.append(node("h3", model.title));
  if (model.type === "tags") { const list = document.createElement("ul"); list.className = "tag-list"; for (const item of model.items) list.append(node("li", item)); container.append(list); }
  if (model.type === "plot-status") { const list = document.createElement("ul"); const currentLabel = model.stateContext === "horizon" ? "Status at this reading" : "Recorded narrative outcome"; const values = [["Authored record", model.recordStatus], ["Starting story status", model.initialState], [currentLabel, model.currentState]]; for (const [label, value] of values) if (value) list.append(node("li", `${label}: ${humanizeToken(value)}`)); container.append(list); }
  if (model.type === "list" || model.type === "facts" || model.type === "story-trail" || model.type === "relationship-trail") { const list = document.createElement("ul"); list.className = model.type.endsWith("trail") ? "story-trail" : ""; for (const item of model.items) { const value = model.type === "facts" ? `${item.label}: ${item.value}` : (model.type === "story-trail" ? `Story status: ${humanizeToken(item.state || "recorded change")}${item.note ? ` — ${authorText(item.note, state.registry)}` : ""}` : (model.type === "relationship-trail" ? `Relationship status: ${humanizeToken(item.relationship_status || "recorded change")}${item.note ? ` — ${authorText(item.note, state.registry)}` : ""}` : item)); list.append(node("li", value)); } container.append(list); }
  if (model.type === "references") { const list = document.createElement("ul"); list.className = "reference-list"; for (const item of model.items) list.append(reference(item)); container.append(list); }
  if (model.type === "effects") { const list = document.createElement("ul"); for (const item of model.items) { const target = state.registry.get(item.target); const line = document.createElement("li"); if (target && available(target)) { const button = node("button", name(target), "lore-link"); button.type = "button"; button.addEventListener("click", () => { void showEntity(target, button); }); line.append(button, node("span", `: ${humanizeToken(item.key || "state")} ${humanizeToken(item.operation || "changed")}`)); } else line.append(node("span", "A referenced entry changes.")); list.append(line); } container.append(list); }
  return container;
}

function renderArticle(detail) {
  const model = buildLoreArticle(detail, state.registry, { includeReference: (id) => available(state.registry.get(id)), includeTransition: (transition) => !transition || !transition.time || isAtOrBeforeHorizon(transition.time, activeHorizon()), horizon: activeHorizon(), inboundReferences: detail.inboundReferences, locationContext: detail.locationContext });
  const fragment = document.createDocumentFragment(); const header = document.createElement("header"); header.className = "article-header"; header.append(node("p", model.kindLabel, "article-kicker"), articleHeading(model.title));
  if (activeHorizon()) header.append(node("p", `Reading context: ${horizonLabel()}`, "field-help")); const summary = document.createElement("div"); summary.className = "article-summary"; summary.append(markdown(model.body, model.title)); header.append(summary); fragment.append(header);
  if (state.searchMatch && (state.searchMatch.heading || state.searchMatch.snippet)) { const match = document.createElement("section"); match.className = "search-grounding"; match.append(node("h3", state.searchMatch.heading ? `Found in ${state.searchMatch.heading}` : "Found in this entry")); if (state.searchMatch.snippet) match.append(node("p", state.searchMatch.snippet)); fragment.append(match); }
  for (const item of model.sections) fragment.append(section(item)); el.article.replaceChildren(fragment);
}

function momentSection(title, help = "") {
  const container = document.createElement("section"); container.className = "lore-section moment-section"; container.append(node("h3", title));
  if (help) container.append(node("p", help, "field-help"));
  return container;
}
function appendAuthorValue(line, value) {
  if (typeof value === "string" && state.registry.has(value) && available(state.registry.get(value))) {
    const entity = state.registry.get(value); const button = node("button", name(entity), "lore-link"); button.type = "button"; button.addEventListener("click", () => { void showEntity(entity, button); }); line.append(button); return;
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
  if (entity && available(entity)) { const button = node("button", name(entity), "lore-link"); button.type = "button"; button.addEventListener("click", () => { void showEntity(entity, button); }); line.append(button); return; }
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
  const stateRead = api.get(entityStateRequestPath(detail.id, readingMoment)).then((payload) => { if (!current()) return; stateSlot.replaceChildren(node("h3", fullStory ? "Last recorded state" : "State at this story moment"), node("p", fullStory ? "Full story is not one story moment; this is the last authored beat in the selected chronology." : "This is the recorded state through the selected author horizon.", "field-help")); locationSlot.replaceChildren(node("h3", fullStory ? "Recorded location" : "Location at this story moment"), node("p", fullStory ? "The journey below is ordered through the last authored beat in this chronology." : "This place is recorded through the selected author horizon.", "field-help")); journeySlot.replaceChildren(node("h3", "Journey trail"), node("p", "Only canonical location set and clear transitions are shown, in exact story order.", "field-help")); populateStateSection(stateSlot, payload); populateLocationSections(locationSlot, journeySlot, payload, fullStory); }).catch(() => { if (current()) { stateSlot.replaceChildren(node("h3", fullStory ? "Last recorded state" : "State at this story moment"), node("p", "The recorded state could not be loaded. Other lore on this page is still available.", "section-error")); locationSlot.replaceChildren(node("h3", fullStory ? "Recorded location" : "Location at this story moment"), node("p", "The recorded location could not be loaded. Other lore on this page is still available.", "section-error")); journeySlot.replaceChildren(node("h3", "Journey trail"), node("p", "The location history could not be loaded. Other lore on this page is still available.", "section-error")); } });
  const knowledgeRead = knowledgeSlot ? api.get(characterKnowledgeRequestPath(detail.id, horizon)).then((payload) => { if (!current()) return; knowledgeSlot.replaceChildren(node("h3", "What this person knows"), node("p", "Knowledge is limited to what has been authored through the selected author horizon.", "field-help")); populateKnowledgeSection(knowledgeSlot, payload); }).catch(() => { if (current()) knowledgeSlot.replaceChildren(node("h3", "What this person knows"), node("p", "The knowledge record could not be loaded. Other lore on this page is still available.", "section-error")); }) : Promise.resolve();
  await Promise.all([stateRead, knowledgeRead]);
}
function conversationBeatCard(beat) {
  const line = document.createElement("article"); line.className = `conversation-beat conversation-beat--${beat.kind}`;
  if (beat.kind === "action") {
    const actors = beat.actors.length ? beat.actors.join(", ") : "Someone present";
    line.append(node("h4", `Action — ${actors}`), node("p", beat.text));
    return line;
  }
  const heading = document.createElement("h4"); heading.append(node("span", beat.speaker));
  if (beat.addressee) heading.append(node("span", ` to ${beat.addressee}`, "conversation-addressee"));
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
  const recollections = Array.isArray(data.recollections) ? data.recollections : []; if (recollections.length) { const memories = document.createElement("section"); memories.className = "lore-section"; memories.append(node("h3", "Subjective recollections"), node("p", "These memories are authored impressions, separate from the canonical transcript.", "field-help")); for (const memory of recollections) { const record = document.createElement("article"); record.className = "timeline-entry"; const character = state.registry.get(memory.character); record.append(node("h4", character ? name(character) : "Unavailable person reference"), node("p", authorText(memory.summary || memory.interpretation || "No recollection summary was supplied.", state.registry))); memories.append(record); } fragment.append(memories); }
  el.article.append(fragment);
}
function renderConversationError() { const section = momentSection("Conversation record", "The transcript could not be loaded. The conversation overview and related lore remain available."); section.lastChild.className = "section-error"; el.article.append(section); }
function horizonBoundary() { const message = document.createElement("section"); message.className = "welcome"; message.append(node("p", "Reading context", "eyebrow"), articleHeading("This entry comes later"), node("p", `This lore has not entered the story ${horizonLabel().toLocaleLowerCase()}. Choose a later reading context or return to the list.`)); el.article.replaceChildren(message); }

async function showEntity(entity, trigger, { historyMode = "push", navigationGeneration, returnToTimeline = false, returnView = "", focus = true, searchMatch = null } = {}) {
  const generation = navigationGeneration ?? beginNavigation(); if (!navigationIsCurrent(generation)) return;
  if (state.view === "index" && historyMode === "push") { state.listScroll = el.entities.scrollTop; state.selectedEntityId = entity.id; writeHistory("replace"); }
  state.returnView = returnView || (returnToTimeline ? "timeline" : (state.returnView || "index"));
  if (!available(entity)) { state.selectedEntityId = ""; setView("article"); horizonBoundary(); if (historyMode !== "none") writeHistory(historyMode); if (focus) focusArticle(); return; }
  const requestId = ++state.detailRequestId; state.searchMatch = searchMatch; state.selectedEntityId = entity.id; setView("article"); selectButton(entity.id); renderArticleMessage(`Opening ${name(entity)}`, "Loading this lore entry…"); if (focus) focusArticle(); text(el.articleStatus, `Opening ${name(entity)}…`); busy(el.article, true);
  try { const detail = state.details.get(entity.id) || await api.get(`/api/entities/${encodeURIComponent(entity.id)}`); if (!navigationIsCurrent(generation) || requestId !== state.detailRequestId) return; state.details.set(entity.id, detail); renderArticle(detail); text(el.articleStatus, ""); if (detail.kind === "conversation") { const horizon = activeHorizon(); try { const record = await api.get(conversationRequestPath(detail.id, horizon ? "as-of" : "all-time", horizon)); if (navigationIsCurrent(generation) && requestId === state.detailRequestId) renderConversation(record); } catch { if (navigationIsCurrent(generation) && requestId === state.detailRequestId) renderConversationError(); } } if (detail.kind === "character") await renderCharacterMoment(detail, requestId, generation); if (!navigationIsCurrent(generation) || requestId !== state.detailRequestId) return; if (historyMode !== "none") writeHistory(historyMode); if (focus) focusArticle(); }
  catch { if (navigationIsCurrent(generation) && requestId === state.detailRequestId) { renderArticleMessage("Lore unavailable", "The requested lore could not be loaded. Please try again."); text(el.articleStatus, ""); if (focus) focusArticle(); } }
  finally { if (navigationIsCurrent(generation) && requestId === state.detailRequestId) busy(el.article, false); }
}

function timelineEntry(entry) { const { at, boundary, entity, kind, lifecycleState, status, summary, location, participants } = entry; const resolved = state.registry.get(entity.id) || entity; const base = kind === "plot-transition" ? "Plot thread" : kindLabel(kind); const boundaryLabel = boundary === "point" ? (kind === "plot-transition" ? "changes" : "") : ({ start: "begins", current: "context", end: "closes" }[boundary] || ""); return { at, boundary, chronologyKind: kind, entity: resolved, lifecycleState, status, summary, location, participants, title: name(resolved), kind: `${base}${boundaryLabel ? ` ${boundaryLabel}` : ""}` }; }
function entriesFromTimeline(payload) { return chronologyEntries(payload).map(timelineEntry); }
async function loadTimeline(id) { if (!id) return []; if (state.timelineCache.has(id)) return state.timelineCache.get(id); const payload = await api.get(`/api/timeline?timeline=${encodeURIComponent(id)}`); const resolvedId = payload.timeline && payload.timeline.id ? payload.timeline.id : id; const entries = entriesFromTimeline(payload); state.timelineCache.set(resolvedId, entries); return entries; }
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
  const button = node("button", name(resolved || { kind: fallbackKind }), "lore-link"); button.type = "button";
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
  const card = document.createElement("article"); card.className = "timeline-entry"; const classification = classifyStoryMoment(entry.at, activeHorizon()); const moment = trailPositionLabel(entry.at, activeHorizon()); if (classification === "past" || classification === "current") card.classList.add("is-revealed"); if (classification === "later") card.classList.add("is-future"); if (classification === "current") card.classList.add("is-current"); const open = loreReferenceButton(entry.entity); const heading = document.createElement("h3"); heading.append(open); card.append(node("p", moment, "trail-position"), node("p", entry.kind, "article-kicker"), heading); const status = entry.lifecycleState ? `Story status: ${humanizeToken(entry.lifecycleState)}` : (entry.status && entry.status !== "canonical" ? `Record status: ${humanizeToken(entry.status)}` : "Recorded story moment"); card.append(node("p", status, "trail-status")); appendTimelineSceneContext(card, entry); if (entry.summary) card.append(node("p", authorText(String(entry.summary).replace(/^#.*\n?/, "").trim().split("\n")[0], state.registry))); return card;
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
function renderWhereaboutsProjection(payload) {
  const fragment = document.createDocumentFragment(); fragment.append(articleHeading("Whereabouts"), node("p", "Every person is shown independently at this story moment. Places and journeys reflect only authored location state and canonical location changes; the compendium does not invent routes, travel, or collective membership.", "whereabouts-intro"));
  fragment.append(node("p", activeHorizon() ? `Reading context: ${horizonLabel()}` : "Reading context: the world’s current authored moment.", "field-help"));
  const fronts = Array.isArray(payload && payload.activeScenes) ? payload.activeScenes : [];
  const frontSection = momentSection("Active story fronts", "Each scene remains separate, even when several scenes share a place."); const frontList = document.createElement("div"); frontList.className = "whereabouts-fronts";
  for (const front of fronts) { const card = document.createElement("section"); card.className = "whereabouts-front"; const heading = document.createElement("h4"); heading.append(loreReferenceButton(state.registry.get(front.scene && front.scene.id) || front.scene, "scene")); card.append(heading); const place = document.createElement("p"); place.append(node("span", "At "), locationButton(front.location)); card.append(place); const people = namedReferences(front.characters); if (people.length) { const line = document.createElement("p"); line.append(node("span", "With ")); appendNamedPeople(line, people); card.append(line); } else card.append(node("p", "No included person is recorded in this scene.", "field-help")); frontList.append(card); }
  if (!fronts.length) frontSection.append(node("p", "No active scene is recorded at this reading moment.")); else frontSection.append(frontList); fragment.append(frontSection);
  const peopleSection = momentSection("All people", "Open an authored journey only when you need the location changes behind a person’s current record."); const peopleList = document.createElement("div"); peopleList.className = "whereabouts-people-list";
  for (const entry of Array.isArray(payload && payload.characters) ? payload.characters : []) {
    const card = document.createElement("section"); card.className = "whereabouts-person"; const character = state.registry.get(entry.character && entry.character.id) || entry.character; const heading = document.createElement("h4"); heading.append(loreReferenceButton(character, "person")); card.append(heading); const role = cachedCharacterRole(character); if (role) card.append(node("p", role, "entity-role"));
    const presence = { "active-scene": "In an active scene", offstage: "Offstage", unlocated: entry.lastKnownLocation ? "No current place recorded" : "No place has been recorded" }[entry.presence] || "Recorded presence"; card.append(node("p", presence, "whereabouts-presence"));
    const location = document.createElement("p"); if (entry.location) location.append(node("span", "At "), locationButton(entry.location)); else if (entry.lastKnownLocation) location.append(node("span", "Last recorded at "), locationButton(entry.lastKnownLocation)); else location.append(node("span", "No place has been recorded.")); card.append(location);
    if (entry.activeScene) { const scene = document.createElement("p"); scene.append(node("span", "Scene: "), loreReferenceButton(state.registry.get(entry.activeScene.id) || entry.activeScene, "scene")); card.append(scene); }
    const journey = Array.isArray(entry.journey) ? entry.journey : []; if (journey.length) { const details = document.createElement("details"); details.append(node("summary", "Authored journey")); const list = document.createElement("ul"); list.className = "whereabouts-journey"; const readingMoment = activeHorizon() || (payload && payload.effectiveTime) || null; journey.forEach((item) => list.append(renderJourney(item, readingMoment, Boolean(activeHorizon())))); details.append(list); card.append(details); }
    peopleList.append(card);
  }
  if (!peopleList.childElementCount) peopleSection.append(node("p", "No canonical or retired people are available in this reading context.")); else peopleSection.append(peopleList); fragment.append(peopleSection); return fragment;
}
async function loadWhereabouts() { const at = activeHorizon(); const key = at ? horizonCoordinateKey(at) : "current"; if (state.whereaboutsCache.has(key)) return state.whereaboutsCache.get(key); const payload = await api.get(whereaboutsRequestPath(at)); state.whereaboutsCache.set(key, payload); return payload; }
async function openWhereabouts({ historyMode = "push", navigationGeneration } = {}) { const generation = navigationGeneration ?? beginNavigation(); if (!navigationIsCurrent(generation)) return; const requestId = ++state.whereaboutsRequestId; setView("whereabouts"); renderArticleMessage("Opening whereabouts", "Gathering the recorded locations and journeys…"); focusLoadedFeature("Whereabouts"); text(el.articleStatus, "Gathering whereabouts…"); busy(el.article, true); let payload; try { payload = await loadWhereabouts(); } catch { if (navigationIsCurrent(generation) && requestId === state.whereaboutsRequestId) { renderArticleMessage("Whereabouts unavailable", "The recorded locations could not be loaded. Please try again."); text(el.articleStatus, ""); focusLoadedFeature("Whereabouts"); busy(el.article, false); } return; } try { if (!navigationIsCurrent(generation) || requestId !== state.whereaboutsRequestId) return; el.article.replaceChildren(renderWhereaboutsProjection(payload)); } catch { if (navigationIsCurrent(generation) && requestId === state.whereaboutsRequestId) renderLoadedFeatureFailure("Whereabouts", "recorded locations"); return; } finally { if (navigationIsCurrent(generation) && requestId === state.whereaboutsRequestId) busy(el.article, false); } if (!navigationIsCurrent(generation) || requestId !== state.whereaboutsRequestId) return; text(el.articleStatus, ""); saveLoadedFeatureHistory("Whereabouts", historyMode); focusLoadedFeature("Whereabouts"); }
function renderPossibilities(payload) { const fragment = document.createDocumentFragment(); fragment.append(articleHeading("Possibilities"), node("p", "These are author hypotheses: non-canonical notes that do not enter the timeline, author horizon, state, causality, whereabouts, or character context.", "whereabouts-intro")); const items = Array.isArray(payload && payload.hypotheses) ? payload.hypotheses : []; const names = (values) => (Array.isArray(values) ? values : []).map((value) => authorText(value && value.title ? value.title : "Unavailable reference", state.registry)).filter(Boolean); if (!items.length) fragment.append(node("p", "No possibilities have been recorded.")); for (const item of items) { const card = document.createElement("section"); card.className = "lore-section"; card.append(node("h3", authorText(item.title || "Untitled possibility", state.registry)), node("p", `Status: ${humanizeToken(item.status || "open")} · Non-canonical`, "article-kicker"), node("p", authorText(item.statement || "", state.registry))); const subjects = names(item.subjects); if (subjects.length) card.append(node("p", `About: ${subjects.join(", ")}`, "field-help")); if (Array.isArray(item.alternatives) && item.alternatives.length) { const list = document.createElement("ul"); for (const alternative of item.alternatives) list.append(node("li", authorText(alternative, state.registry))); card.append(node("h4", "Possible readings"), list); } if (item.context) card.append(node("p", authorText(item.context, state.registry), "field-help")); const placement = item.placement || {}; const placed = [placement.scene, placement.event, placement.location].filter(Boolean).map((value) => authorText(value.title || "Unavailable reference", state.registry)); if (placed.length || placement.context) card.append(node("p", `${placed.length ? `Context: ${placed.join(", ")}` : "Context"}${placement.context ? ` — ${authorText(placement.context, state.registry)}` : ""}`, "field-help")); const resolution = item.resolution || {}; const adopted = names(resolution.canonicalRecords); if (adopted.length) card.append(node("p", `Already addressed in canon: ${adopted.join(", ")}.`, "field-help")); if (item.status === "rejected" && resolution.note) card.append(node("p", `Retained rejection note: ${authorText(resolution.note, state.registry)}`, "field-help")); fragment.append(card); } return fragment; }
async function refreshSearchForFullStory(navigationGeneration) {
  if (!state.query) { const source = indexSource(); renderEntities(source); updateIndexStatus(source); return true; }
  busy(el.entities, true); text(el.entitiesStatus, "Refreshing search for the full story…");
  try {
    const refreshed = await refreshAuthorSearch(() => api.get(authorSearchRequestPath(state.query, activeHorizon())), (payload) => presentSearchResults(payload, state.registry), state.searchResults);
    if (!navigationIsCurrent(navigationGeneration)) return false;
    state.searchResults = refreshed.results;
    state.searchError = refreshed.error ? "Search results from the earlier reading context are still shown. The search could not refresh for the full story." : "";
    const source = indexSource(); renderEntities(source); updateIndexStatus(source);
    return true;
  } finally { if (navigationIsCurrent(navigationGeneration)) busy(el.entities, false); }
}
async function openPossibilities({ historyMode = "push", navigationGeneration } = {}) { const generation = navigationGeneration ?? beginNavigation(); if (!navigationIsCurrent(generation)) return; const requestId = ++state.possibilitiesRequestId; const hadHorizon = Boolean(activeHorizon()); state.horizon = null; setView("possibilities"); renderArticleMessage("Opening possibilities", "Gathering non-canonical author notes…"); focusLoadedFeature("Possibilities"); busy(el.article, true); if (hadHorizon && !await refreshSearchForFullStory(generation)) { if (navigationIsCurrent(generation) && requestId === state.possibilitiesRequestId) busy(el.article, false); return; } let payload; try { payload = await api.get("/api/hypotheses"); } catch { if (navigationIsCurrent(generation) && requestId === state.possibilitiesRequestId) { renderArticleMessage("Possibilities unavailable", "The non-canonical author notes could not be loaded. Please try again."); text(el.articleStatus, ""); focusLoadedFeature("Possibilities"); } if (navigationIsCurrent(generation) && requestId === state.possibilitiesRequestId) busy(el.article, false); return; } try { if (!navigationIsCurrent(generation) || requestId !== state.possibilitiesRequestId) return; el.article.replaceChildren(renderPossibilities(payload)); } catch { if (navigationIsCurrent(generation) && requestId === state.possibilitiesRequestId) renderLoadedFeatureFailure("Possibilities", "non-canonical author notes"); return; } finally { if (navigationIsCurrent(generation) && requestId === state.possibilitiesRequestId) busy(el.article, false); } if (!navigationIsCurrent(generation) || requestId !== state.possibilitiesRequestId) return; text(el.articleStatus, ""); saveLoadedFeatureHistory("Possibilities", historyMode); focusLoadedFeature("Possibilities"); }
function renderTimeline(id) {
  state.activeTimelineId = id; setView("timeline"); const fragment = document.createDocumentFragment(); fragment.append(articleHeading("Story trail"), node("p", "Story beats are shown in chronological order. Every stop is equally spaced; the gaps do not indicate elapsed duration.", "timeline-intro")); const origin = originLabel(state.timelineDeclarations, id); if (origin) fragment.append(node("p", `The trail begins at ${origin}.`, "field-help"));
  const tabs = document.createElement("div"); tabs.className = "timeline-tabs"; for (const timeline of state.timelineDeclarations) { const tab = node("button", timelineDisplayLabel(timeline)); tab.type = "button"; tab.setAttribute("aria-pressed", String(timeline.id === id)); tab.addEventListener("click", () => { void openTimeline(timeline.id); }); tabs.append(tab); } fragment.append(tabs);
  fragment.append(renderWhereaboutsLink());
  const rail = document.createElement("div"); rail.className = "timeline-rail"; for (const group of timelinePresentationGroups(currentTimeline(), activeHorizon())) { if (group.type !== "entry") { const container = document.createElement("section"); const isFronts = group.type === "concurrent-scenes"; container.className = `${isFronts ? "timeline-concurrent-scenes" : "meanwhile-group"} is-${group.classification}`; if (group.classification !== "later") container.classList.add("is-revealed"); if (group.classification === "current") container.classList.add("is-current"); container.setAttribute("aria-label", isFronts ? "Parallel story fronts" : "Meanwhile, at the same story moment"); if (!isFronts) container.append(node("p", "Meanwhile", "meanwhile-label")); const cards = document.createElement("div"); cards.className = "meanwhile-cards"; cards.append(...group.entries.map(renderTimelineCard)); container.append(cards); rail.append(container); } else rail.append(renderTimelineCard(group.entries[0])); } if (!currentTimeline().length) rail.append(node("p", "No dated story beats have been authored on this timeline yet.")); fragment.append(rail); el.article.replaceChildren(fragment); text(el.articleStatus, ""); focusArticle();
}
async function openTimeline(id = "", { historyMode = "push", navigationGeneration } = {}) { const generation = navigationGeneration ?? beginNavigation(); if (!navigationIsCurrent(generation)) return; const requestId = ++state.timelineRequestId; const selected = selectedTimelineId(id, state.timelineDefault, state.timelineDeclarations); setView("timeline"); renderArticleMessage("Opening story trail", "Gathering the story timeline…"); focusArticle(); text(el.articleStatus, "Gathering the story timeline…"); busy(el.article, true); try { await loadTimeline(selected); if (!navigationIsCurrent(generation) || requestId !== state.timelineRequestId) return; state.activeTimelineId = selected; populateHorizons(); renderTimeline(selected); if (historyMode !== "none") writeHistory(historyMode); } catch { if (navigationIsCurrent(generation) && requestId === state.timelineRequestId) { renderArticleMessage("Timeline unavailable", "The requested lore could not be loaded. Please try again."); text(el.articleStatus, ""); focusArticle(); } } finally { if (navigationIsCurrent(generation) && requestId === state.timelineRequestId) busy(el.article, false); } }

function indexSource() { return state.query ? filterSearchResultsByKind(state.searchResults || [], state.searchKind) : state.entities.filter((entity) => !state.kind || entity.kind === state.kind); }
function renderIndex({ focus = false, navigationGeneration } = {}) { const generation = navigationGeneration ?? beginNavigation(); if (!navigationIsCurrent(generation)) return; setView("index"); el.entityText.value = state.query; el.searchKind.value = state.searchKind; updateIndexHeading(); const source = indexSource(); renderEntities(source); updateIndexStatus(source); requestAnimationFrame(() => { if (!navigationIsCurrent(generation)) return; el.entities.scrollTop = state.listScroll; if (focus) { const selected = el.entities.querySelector('[aria-pressed="true"]'); (selected || el.entityText).focus(); } }); }
function browse(kind, label) { const generation = beginNavigation(); Object.assign(state, historyAction(state, { type: "browse", kind })); state.searchKind = kind; state.returnView = "index"; state.searchError = ""; state.searchResults = null; state.listScroll = 0; text(el.indexHeading, label); renderIndex({ focus: isMobileLayout(), navigationGeneration: generation }); writeHistory("push"); }
async function search({ preserveResults = false } = {}) { const generation = beginNavigation(); const query = el.entityText.value.trim(); const kind = el.searchKind.value; const priorResults = preserveResults && query === state.query ? state.searchResults : null; state.query = query; state.kind = kind; state.searchKind = kind; state.searchError = ""; state.searchResults = priorResults; state.listScroll = 0; busy(el.entities, true); text(el.entitiesStatus, "Searching lore…"); try { const refreshed = state.query ? await refreshAuthorSearch(() => api.get(authorSearchRequestPath(state.query, activeHorizon())), (payload) => presentSearchResults(payload, state.registry), priorResults) : { error: null, results: null }; if (!navigationIsCurrent(generation)) return; state.searchResults = refreshed.results; state.searchError = refreshed.error ? "The search could not be loaded. Retry search when the connection is restored." : ""; updateIndexHeading(); syncNavigation(); const source = indexSource(); renderEntities(source); updateIndexStatus(source); writeHistory(); } finally { if (navigationIsCurrent(generation)) busy(el.entities, false); } }
async function changeHorizon() { const generation = beginNavigation(); const selected = state.horizonEntries.get(el.horizon.value); state.horizon = selected ? selected.at : null; state.listScroll = el.entities.scrollTop; if (state.query) { busy(el.entities, true); text(el.entitiesStatus, "Refreshing search for this story moment…"); const refreshed = await refreshAuthorSearch(() => api.get(authorSearchRequestPath(state.query, activeHorizon())), (payload) => presentSearchResults(payload, state.registry), state.searchResults); if (!navigationIsCurrent(generation)) return; state.searchResults = refreshed.results; state.searchError = refreshed.error ? "Search results from the earlier reading context are still shown. The search could not refresh for this story moment." : ""; if (navigationIsCurrent(generation)) busy(el.entities, false); } if (!navigationIsCurrent(generation)) return; if (state.view === "possibilities") { writeHistory(); return; } const source = indexSource(); renderEntities(source); updateIndexStatus(source); if (state.view === "whereabouts") { await openWhereabouts({ historyMode: "none", navigationGeneration: generation }); if (navigationIsCurrent(generation)) writeHistory(); return; } if (state.view === "timeline") renderTimeline(state.activeTimelineId); else if (state.view === "article") { const entry = state.registry.get(state.selectedEntityId); if (entry) await showEntity(entry, null, { historyMode: "replace", navigationGeneration: generation, returnView: state.returnView, focus: false }); } else renderIndex({ navigationGeneration: generation }); if (navigationIsCurrent(generation)) writeHistory(); }
function activateNav(button) { for (const item of el.nav.querySelectorAll(".nav-item")) item.removeAttribute("aria-current"); button.setAttribute("aria-current", "page"); }
async function restore(snapshot) { const requestId = ++state.restoreRequestId; const generation = beginNavigation(); const currentRestore = () => navigationIsCurrent(generation) && requestId === state.restoreRequestId; const restored = restoreNavigation(snapshot, { timelineId: state.activeTimelineId }); const known = state.timelineDeclarations.some((item) => item.id === restored.timelineId); Object.assign(state, { kind: restored.kind, mobilePane: restored.mobilePane, query: restored.query, searchKind: restored.searchKind || "", horizon: restored.horizon, activeTimelineId: known ? restored.timelineId : state.timelineDefault, returnView: restored.returnView, selectedEntityId: restored.entryId, listScroll: restored.listScroll, searchError: "", searchMatch: null, searchResults: null }); if (state.activeTimelineId) { await loadTimeline(state.activeTimelineId); if (!currentRestore()) return; populateHorizons(); } if (state.query) { const refreshed = await refreshAuthorSearch(() => api.get(authorSearchRequestPath(state.query, activeHorizon())), (payload) => presentSearchResults(payload, state.registry)); if (!currentRestore()) return; state.searchResults = refreshed.results; state.searchError = refreshed.error ? "The saved search could not be restored. Retry search when the connection is restored." : ""; if (!currentRestore()) return; } if (restored.mobilePane === "nav") { setView("index", "nav"); requestAnimationFrame(() => { if (navigationIsCurrent(generation)) el.nav.querySelector(".nav-item")?.focus(); }); return; } if (restored.view === "possibilities") { await openPossibilities({ historyMode: "none", navigationGeneration: generation }); return; } if (restored.view === "whereabouts") { await openWhereabouts({ historyMode: "none", navigationGeneration: generation }); return; } if (restored.view === "timeline") { renderTimeline(state.activeTimelineId); return; } if (restored.view === "article") { const entry = state.registry.get(restored.entryId); if (entry) { await showEntity(entry, null, { historyMode: "none", navigationGeneration: generation, returnView: restored.returnView }); return; } } renderIndex({ focus: true, navigationGeneration: generation }); }
async function boot() { const session = await api.get("/api/session"); api.setToken(session.token); const [status, entities] = await Promise.all([api.get("/api/status"), api.get("/api/entities")]); state.entities = entities; state.registry = createEntityRegistry(entities); state.timelineDeclarations = (status.timeModel && status.timeModel.timelineDeclarations) || []; state.timelineDefault = (status.timeModel && status.timeModel.defaultTimeline) || ""; state.activeTimelineId = selectedTimelineId("", state.timelineDefault, state.timelineDeclarations); const world = entities.find((entity) => entity.kind === "world"); text(el.worldName, world ? name(world) : "Your world"); text(el.appStatus, `${status.recordCount} lore entries ready`); if (state.activeTimelineId) { await loadTimeline(state.activeTimelineId); populateHorizons(); } busy(el.entities, false); await restore(history.state); writeHistory(); }

el.entities.addEventListener("scroll", () => { state.listScroll = el.entities.scrollTop; }); el.entitiesForm.addEventListener("submit", (event) => { event.preventDefault(); void search(); }); el.searchKind.addEventListener("change", () => { state.searchKind = el.searchKind.value; state.kind = state.searchKind; updateIndexHeading(); syncNavigation(); const source = indexSource(); renderEntities(source); updateIndexStatus(source); writeHistory(); }); el.retrySearch.addEventListener("click", () => { void search({ preserveResults: true }); }); el.horizon.addEventListener("change", () => { void changeHorizon(); });
el.mobileBack.addEventListener("click", () => { beginNavigation(); state.listScroll = el.entities.scrollTop; Object.assign(state, historyAction(state, { type: "open-navigation" })); setView("index", "nav"); writeHistory("push"); requestAnimationFrame(() => el.nav.querySelector(".nav-item")?.focus()); });
el.back.addEventListener("click", () => { if (state.view === "article" && state.returnView === "timeline") void openTimeline(state.activeTimelineId); else if (state.view === "article" && state.returnView === "whereabouts") void openWhereabouts(); else { const generation = beginNavigation(); Object.assign(state, historyAction(state, { type: "back-to-list" })); renderIndex({ focus: true, navigationGeneration: generation }); writeHistory("push"); } });
for (const button of el.nav.querySelectorAll(".nav-item")) button.addEventListener("click", () => { activateNav(button); if (button.dataset.view === "timeline") void openTimeline(); else if (button.dataset.view === "whereabouts") void openWhereabouts(); else if (button.dataset.view === "possibilities") void openPossibilities(); else browse(button.dataset.kind || "", button.textContent); }); window.addEventListener("popstate", (event) => { void restore(event.state); });
boot().catch(() => { text(el.appStatus, "The compendium could not be opened."); text(el.entitiesStatus, "Lore is unavailable until the connection is restored."); busy(el.entities, false); });
