// The file stays dependency-free so it can run with Node's built-in test API.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { authorSearchRequestPath, characterKnowledgeRequestPath, contextRequestPath, conversationRequestPath, entityRequestPath, entityStateRequestPath, whereaboutsRequestPath } from "../src/wedl/static/query.mjs";
import { authorText, buildLoreArticle, conversationTranscriptBeats, createEntityRegistry, intervalContains, observationVisibleAt, referenceVisibleAt, safeDisplayName } from "../src/wedl/static/lore.mjs";
import { authorSearchHeading, filterSearchResultsByKind, isAuthorSearchResult, plainSearchText, presentSearchResults, refreshAuthorSearch } from "../src/wedl/static/search.mjs";
import { clearSupersededNavigationLoading, createNavigationGeneration, historyAction, navigationSnapshot, restoreNavigation } from "../src/wedl/static/navigation.mjs";
import { chronologyEntries, classifyStoryMoment, compareStoryTime, horizonForTimeline, isAtOrBeforeHorizon, isEntityAvailable, originLabel, referencePresentAt, sceneContainsMoment, sceneSnapshotsAt, selectedTimelineId, timelineDisplayLabel, timelinePresentationGroups, trailPositionLabel } from "../src/wedl/static/timeline.mjs";

const source = await readFile(new URL("../src/wedl/static/api.js", import.meta.url), "utf8");
const { createApiClient } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
const appSource = await readFile(new URL("../src/wedl/static/app.js", import.meta.url), "utf8");
const htmlSource = await readFile(new URL("../src/wedl/static/index.html", import.meta.url), "utf8");
const stylesheetSource = await readFile(new URL("../src/wedl/static/style.css", import.meta.url), "utf8");

function contrastRatio(foreground, background) {
  const luminance = (hex) => {
    const channels = hex.slice(1).match(/../g).map((part) => Number.parseInt(part, 16) / 255).map((channel) => channel <= .04045 ? channel / 12.92 : ((channel + .055) / 1.055) ** 2.4);
    return .2126 * channels[0] + .7152 * channels[1] + .0722 * channels[2];
  };
  const [first, second] = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (first + .05) / (second + .05);
}

test("API client returns JSON and attaches the session token", async () => {
  let received;
  globalThis.fetch = async (path, options) => {
    received = { path, options };
    return { ok: true, json: async () => ({ results: ["record"] }) };
  };

  const client = createApiClient();
  client.setToken("session-token");
  const payload = await client.get("/api/search?q=flood&perspective=author&allTime=true");

  assert.deepEqual(payload, { results: ["record"] });
  assert.equal(received.path, "/api/search?q=flood&perspective=author&allTime=true");
  assert.equal(received.options.headers["X-Wedl-Token"], "session-token");
});

test("API client exposes JSON and plain-text failures to UI status messages", async () => {
  const client = createApiClient();
  globalThis.fetch = async () => ({ ok: false, status: 400, text: async () => '{"message":"Invalid time scope"}' });
  await assert.rejects(client.get("/api/search"), { message: "Invalid time scope" });

  globalThis.fetch = async () => ({ ok: false, status: 503, text: async () => "Service unavailable" });
  await assert.rejects(client.get("/api/status"), { message: "Service unavailable" });
});

test("entity and context request builders omit blank optional filters", () => {
  assert.equal(entityRequestPath("", ""), "/api/entities");
  assert.deepEqual(
    Object.fromEntries(new URL(entityRequestPath("character", "Mara Vale"), "http://wedl.test").searchParams),
    { kind: "character", text: "Mara Vale" },
  );
  assert.deepEqual(
    Object.fromEntries(new URL(contextRequestPath("char-mara", "", ""), "http://wedl.test").searchParams),
    { character: "char-mara" },
  );
  assert.deepEqual(
    Object.fromEntries(new URL(contextRequestPath("char-mara", "scene-1", "register ribbon"), "http://wedl.test").searchParams),
    { character: "char-mara", scene: "scene-1", q: "register ribbon" },
  );
});

test("author search and character moment request builders use the shared horizon or full story", () => {
  const sceneSearch = new URL(authorSearchRequestPath("flood register", { timeline: "main", tick: "17", order: "3" }), "http://wedl.test").searchParams;
  assert.equal(sceneSearch.get("q"), "flood register");
  assert.equal(sceneSearch.get("perspective"), "author");
  assert.equal(sceneSearch.get("timeline"), "main");
  assert.equal(sceneSearch.get("tick"), "17");
  assert.equal(sceneSearch.get("order"), "3");
  assert.equal(sceneSearch.has("allTime"), false);

  const allTimeSearch = new URL(authorSearchRequestPath("flood register", ""), "http://wedl.test").searchParams;
  assert.equal(allTimeSearch.get("allTime"), "true");
  assert.equal(allTimeSearch.has("timeline"), false);

  const state = new URL(entityStateRequestPath("char_123", { timeline: "main", tick: "17", order: "3" }), "http://wedl.test");
  const knowledge = new URL(characterKnowledgeRequestPath("char_123", { timeline: "main", tick: "17", order: "3" }), "http://wedl.test");
  assert.equal(state.pathname, "/api/entities/char_123/state");
  assert.equal(knowledge.pathname, "/api/entities/char_123/knowledge");
  assert.deepEqual(Object.fromEntries(state.searchParams), { timeline: "main", tick: "17", order: "3" });
});

test("whereabouts uses the existing horizon-aware read projection and kind filtering stays local", () => {
  assert.equal(whereaboutsRequestPath(), "/api/whereabouts");
  assert.deepEqual(Object.fromEntries(new URL(whereaboutsRequestPath({ timeline: "main", tick: "9223372036854775807", order: "-1" }), "http://wedl.test").searchParams), { timeline: "main", tick: "9223372036854775807", order: "-1" });
  const cards = [
    { entity: { kind: "character", title: "Rhea" } },
    { entity: { kind: "location", title: "Blackwater Crossing" } },
  ];
  assert.deepEqual(filterSearchResultsByKind(cards, "character"), [cards[0]]);
  assert.deepEqual(filterSearchResultsByKind(cards), cards);
  assert.match(appSource, /whereaboutsRequestPath\(at\)/);
  assert.match(appSource, /Every person is shown independently/);
  assert.match(appSource, /does not invent routes, travel, or collective membership/);
  assert.match(htmlSource, /data-view="whereabouts"/);
  assert.match(htmlSource, /id="search-kind"/);
});

test("whereabouts mounts its successful projection and keeps cached roles and journey labels honest", () => {
  assert.match(appSource, /el\.article\.replaceChildren\(renderWhereaboutsProjection\(payload\)\)/);
  assert.match(appSource, /const cachedCharacterRole = \(entity\) => \{/);
  assert.match(appSource, /const detail = state\.details\.get\(entity\.id\)/);
  assert.doesNotMatch(appSource, /api\.get\([^\n]*frontmatter\.role/);
  assert.match(appSource, /entry\.lastKnownLocation \? "No current place recorded" : "No place has been recorded"/);
  assert.match(appSource, /item\.kind === "initial"/);
  assert.match(appSource, /"Starting place: "/);
  assert.match(appSource, /"At the story’s beginning"/);
  assert.match(appSource, /activeHorizon\(\) \|\| \(payload && payload\.effectiveTime\)/);
  assert.match(appSource, /"At current authored moment"/);
  assert.match(appSource, /"At selected moment"/);
  assert.match(appSource, /"Earlier in the story"/);
});

test("search cards are named, deduplicated, and strip indexed markup without exposing IDs", () => {
  const registry = createEntityRegistry([{ id: "char_1234567890ABC", kind: "character", title: "Mara Vale" }]);
  const cards = presentSearchResults({ results: [
    { entityId: "char_1234567890ABC", heading: "<mark>Background</mark>", snippet: "Mara studies char_1234567890ABC." },
    { entityId: "char_1234567890ABC", heading: "Other", snippet: "Duplicate" },
    { entityId: "missing_1234567890ABC", heading: "Lost", snippet: "Ignored" },
  ] }, registry);
  assert.deepEqual(cards, [{ entity: registry.get("char_1234567890ABC"), match: { heading: "Background", snippet: "Mara studies Mara Vale." } }]);
  assert.equal(plainSearchText("# <mark>Safe</mark> text", registry), "Safe text");
  assert.equal(authorSearchHeading("verbatim-turn", registry), "the canonical transcript");
  assert.equal(authorSearchHeading("scene_observation", registry), "what happens in the scene");
  assert.equal(authorSearchHeading("observation", registry), "what happens in the scene");
  assert.equal(presentSearchResults({ results: [{ entityId: "char_1234567890ABC", heading: "event-effect", snippet: "The archive closes." }] }, registry)[0].match.heading, "what changes after the event");
});

test("author search suppresses generated metadata grounding and keeps authored excerpts", () => {
  const registry = createEntityRegistry([{ id: "char_1234567890ABC", kind: "character", title: "Mara Vale" }]);
  const cards = presentSearchResults({ results: [
    { entityId: "char_1234567890ABC", documentKind: "entity-author-metadata", heading: "Domain", snippet: "Kind: character Domain: archive Tags: witness" },
    { entityId: "char_1234567890ABC", documentKind: "scene-observation", heading: "scene-observation", snippet: "Mara hears the archive bell." },
  ] }, registry);
  assert.equal(isAuthorSearchResult({ documentKind: "entity-author-metadata" }), false);
  assert.equal(isAuthorSearchResult({ heading: "schema" }), false);
  assert.deepEqual(cards, [{ entity: registry.get("char_1234567890ABC"), match: { heading: "what happens in the scene", snippet: "Mara hears the archive bell." } }]);
  assert.doesNotMatch(JSON.stringify(cards.map((card) => card.match)), /\b(?:Kind|Domain|Tags|Schema)\b/i);
  assert.equal(authorSearchHeading("Domain", registry), "");
});

test("a rejected horizon search keeps prior cards and reports a retryable error", async () => {
  const previous = [{ entity: { title: "Mara Vale" }, match: { snippet: "Earlier reading." } }];
  const refreshed = await refreshAuthorSearch(
    async () => { throw new Error("offline"); },
    () => { throw new Error("must not present a failed payload"); },
    previous,
  );
  assert.equal(refreshed.error.message, "offline");
  assert.deepEqual(refreshed.results, previous);
  assert.match(appSource, /Search results from the earlier reading context are still shown/);
  assert.match(appSource, /retrySearch\.addEventListener\("click"/);
  assert.doesNotMatch(appSource, /catch \{ if \(navigationIsCurrent\(generation\)\) state\.searchResults = \[\]; \}/);
});

test("retrying the same failed author search keeps its earlier-context cards", async () => {
  const previous = [{ entity: { title: "Mara Vale" }, match: { snippet: "Earlier reading." } }];
  const retry = await refreshAuthorSearch(
    async () => { throw new Error("still offline"); },
    () => { throw new Error("must not present a failed payload"); },
    previous,
  );
  assert.deepEqual(retry.results, previous);
  assert.match(appSource, /async function search\(\{ preserveResults = false \} = \{\}\)/);
  assert.match(appSource, /const priorResults = preserveResults && query === state\.query \? state\.searchResults : null/);
  assert.match(appSource, /refreshAuthorSearch\(\(\) => api\.get\(authorSearchRequestPath\(state\.query, activeHorizon\(\)\)\), \(payload\) => presentSearchResults\(payload, state\.registry\), priorResults\)/);
  assert.match(appSource, /retrySearch\.addEventListener\("click", \(\) => \{ void search\(\{ preserveResults: true \}\); \}\)/);
});

test("lore presentation resolves names and never falls back to opaque identifiers", () => {
  const registry = createEntityRegistry([
    { id: "char_1234567890ABC", kind: "character", title: "Mara Vale" },
    { id: "loc_1234567890ABC", kind: "location", title: "Reading Room" },
  ]);
  assert.equal(safeDisplayName("char_1234567890ABC", registry), "Mara Vale");
  assert.equal(safeDisplayName("missing_1234567890ABC", registry, "character"), "Unavailable person reference");
  const article = buildLoreArticle({
    id: "scene_1234567890ABC", kind: "scene", title: "A Quiet Meeting", bodyMarkdown: "A meeting in the archive.",
    frontmatter: { kind: "scene", id: "scene_1234567890ABC", location: "loc_1234567890ABC", participants: [{ character: "char_1234567890ABC", role: "viewpoint" }] },
  }, registry);
  assert.equal(article.title, "A Quiet Meeting");
  assert.equal(article.kindLabel, "Scene");
  assert.equal(article.sections.find((section) => section.title === "Place").items[0].id, "loc_1234567890ABC");

  const trails = buildLoreArticle({ id: "sp_123", kind: "story-point", status: "draft", title: "Find the archive", bodyMarkdown: "", frontmatter: { lifecycle: { initial_state: "dormant", transitions: [{ state: "active", note: "A lead appears.", time: { timeline: "main", tick: "4", order: "0" } }, { state: "resolved", note: "Later resolution.", time: { timeline: "main", tick: "9", order: "0" } }] } } }, registry, { includeTransition: (item) => item.time.tick === "4" });
  assert.equal(trails.sections.find((section) => section.title === "Plot thread trail").type, "story-trail");
  assert.deepEqual(trails.sections.find((section) => section.type === "plot-status"), { type: "plot-status", title: "Plot thread status", recordStatus: "draft", initialState: "dormant", currentState: "active", stateContext: "full-story" });
  const horizonTrail = buildLoreArticle({ id: "sp_456", kind: "story-point", title: "Find the archive", frontmatter: { lifecycle: { initial_state: "dormant", transitions: [{ state: "active", time: { timeline: "main", tick: "4", order: "0" } }] } } }, registry, { horizon: { timeline: "main", tick: "4", order: "0" } });
  assert.equal(horizonTrail.sections.find((section) => section.type === "plot-status").stateContext, "horizon");

  const sceneAtHorizon = buildLoreArticle({ id: "scene_123", kind: "scene", title: "A Quiet Meeting", bodyMarkdown: "Static authored prose.", frontmatter: { time: { start: { timeline: "main", tick: "4", order: "0" }, end: { timeline: "main", tick: "9", order: "0" } }, participants: [{ character: "char_1234567890ABC", role: "host", from: { timeline: "main", tick: "4", order: "2" }, to: { timeline: "main", tick: "6", order: "4" } }, { character: "loc_1234567890ABC", role: "late arrival", from: { timeline: "main", tick: "6", order: "4" }, until: { timeline: "main", tick: "9", order: "0" } }], observations: [{ text: "The rain begins.", at: { timeline: "main", tick: "4", order: "2" }, until: { timeline: "main", tick: "6", order: "4" } }, { text: "A later bell rings.", at: { timeline: "main", tick: "6", order: "5" } }] } }, registry, { horizon: { timeline: "main", tick: "6", order: "4" } });
  assert.deepEqual(sceneAtHorizon.sections.find((section) => section.title === "Cast").items.map((item) => item.id), ["char_1234567890ABC", "loc_1234567890ABC"]);
  assert.deepEqual(sceneAtHorizon.sections.find((section) => section.title === "What happens here").items, ["The rain begins."]);
  assert.equal(sceneAtHorizon.body, "Static authored prose.");
});

test("location articles preserve authored connection prose and directional roles separately", () => {
  const registry = createEntityRegistry([
    { id: "loc_archive", kind: "location", title: "The Archive" },
    { id: "loc_stacks", kind: "location", title: "The Stacks" },
    { id: "loc_gate", kind: "location", title: "River Gate" },
  ]);
  const article = buildLoreArticle({ id: "loc_archive", kind: "location", title: "The Archive", frontmatter: { parent: "loc_gate", links: ["loc_stacks"] } }, registry, {
    locationContext: {
      parent: { id: "loc_gate", kind: "location", title: "River Gate" },
      children: [{ id: "loc_stacks", kind: "location", title: "The Stacks" }],
      outgoing: [{ place: { id: "loc_stacks", kind: "location", title: "The Stacks" }, reciprocal: false, description: "A sealed stair", summary: "Shelves rise around a quiet lamp." }],
      incoming: [{ place: { id: "loc_gate", kind: "location", title: "River Gate" }, reciprocal: false, description: "A gateward return through loc_stacks", summary: "The road meets UNKNOWN_1234567890." }],
    },
  });
  assert.deepEqual(article.sections.slice(0, 3).map((section) => section.title), ["Within", "Places within", "Connections from here"]);
  assert.deepEqual(article.sections.find((section) => section.title === "Connections from here").items, [{ id: "loc_stacks", description: "A sealed stair", summary: "Shelves rise around a quiet lamp.", role: "One-way authored connection" }]);
  assert.deepEqual(article.sections.find((section) => section.title === "Connections to here").items, [{ id: "loc_gate", description: "A gateward return through loc_stacks", summary: "The road meets UNKNOWN_1234567890.", role: "One-way authored connection" }]);
  assert.equal(authorText(article.sections.find((section) => section.title === "Connections to here").items[0].description, registry), "A gateward return through The Stacks");
  assert.equal(authorText(article.sections.find((section) => section.title === "Connections to here").items[0].summary, registry), "The road meets Unavailable reference.");
  assert.doesNotMatch(JSON.stringify(article.sections.map((section) => section.items)), /loc_archive/);
  assert.match(appSource, /authorText\(prose, state\.registry\), "reference-detail"/);
  assert.match(appSource, /humanizeToken\(role\), "reference-role"/);
});

test("scene horizon filtering follows WEDL at/until and presence intervals exactly", () => {
  const at = { timeline: "main", tick: "41", order: "7" };
  const until = { timeline: "main", tick: "41", order: "9" };
  assert.equal(intervalContains(until, at, until), true, "both interval endpoints are inclusive");
  assert.equal(intervalContains({ timeline: "main", tick: "41", order: "10" }, at, until), false);
  assert.equal(intervalContains({ timeline: "branch", tick: "41", order: "7" }, at, until), false);
  assert.equal(intervalContains(until, { tick: "41", order: "7" }, { tick: "41", order: "9" }), true, "omitted source timelines use the active chronology");
  assert.equal(observationVisibleAt({ at, until }, until), true);
  assert.equal(observationVisibleAt({ at, until }, { timeline: "main", tick: "41", order: "10" }), false);
  assert.equal(referenceVisibleAt({ from: at, until }, until), true);
  assert.equal(referenceVisibleAt({ from: at, to: until }, until), true, "published participant `to` remains supported");
  assert.equal(referenceVisibleAt({ from: at, until }, { timeline: "main", tick: "41", order: "10" }), false);
});

test("API-shaped decimal story coordinates keep adjacent signed64 scene bounds distinct", () => {
  const registry = createEntityRegistry([{ id: "char_1234567890ABC", kind: "character", title: "Mara Vale" }]);
  const first = "9007199254740993";
  const next = "9007199254740994";
  const detail = {
    id: "scene_123", kind: "scene", title: "Exact clock", bodyMarkdown: "",
    frontmatter: {
      time: { start: { timeline: "main", tick: first, order: "2147483647" }, end: { timeline: "main", tick: next, order: "-2147483648" } },
      participants: [{ character: "char_1234567890ABC", from: { timeline: "main", tick: next, order: "-2147483648" } }],
      observations: [{ text: "The second beat arrives.", at: { timeline: "main", tick: next, order: "-2147483648" } }],
    },
  };
  const before = buildLoreArticle(detail, registry, { horizon: { timeline: "main", tick: first, order: "2147483647" } });
  const atNext = buildLoreArticle(detail, registry, { horizon: { timeline: "main", tick: next, order: "-2147483648" } });
  assert.equal(before.sections.some((section) => section.title === "Cast"), false);
  assert.equal(before.sections.some((section) => section.title === "What happens here"), false);
  assert.deepEqual(atNext.sections.find((section) => section.title === "Cast").items.map((item) => item.id), ["char_1234567890ABC"]);
  assert.deepEqual(atNext.sections.find((section) => section.title === "What happens here").items, ["The second beat arrives."]);
});

test("inbound related lore uses explicit references, never nearby timeline material", () => {
  const registry = createEntityRegistry([
    { id: "char_1234567890ABC", kind: "character", title: "Mara Vale" },
    { id: "scene_1234567890ABC", kind: "scene", title: "The Flood Register" },
    { id: "event_1234567890ABC", kind: "event", title: "An Unrelated Bell" },
  ]);
  const article = buildLoreArticle({ id: "char_1234567890ABC", kind: "character", title: "Mara Vale", frontmatter: {} }, registry, {
    inboundReferences: [{ id: "scene_1234567890ABC" }],
    // A caller may have timeline information available, but it is not evidence
    // of a lore link and must not turn into a false-positive backlink.
    timelineEntries: [{ entity: { id: "event_1234567890ABC" }, at: { timeline: "main", tick: "1", order: "0" } }],
  });
  assert.deepEqual(article.sections.find((section) => section.title === "Related lore").items, [{ id: "scene_1234567890ABC" }]);
  assert.equal(JSON.stringify(article.sections).includes("event_1234567890ABC"), false);
});

test("conversation request builder follows the shared author horizon", () => {
  const scoped = new URL(conversationRequestPath("conv_123", "as-of", { timeline: "main", tick: 42, order: 3 }), "http://wedl.test").searchParams;
  assert.deepEqual(Object.fromEntries(scoped), { perspective: "author", timeline: "main", tick: "42", order: "3" });
  const fullStory = new URL(conversationRequestPath("conv_123", "all-time"), "http://wedl.test").searchParams;
  assert.equal(fullStory.get("allTime"), "true");
});

test("conversation beats render safe named speech, action, and interrupted exchanges with a legacy fallback", () => {
  const registry = createEntityRegistry([
    { id: "char_1234567890ABC", kind: "character", title: "Mara Vale" },
    { id: "char_ABCDEFGHIJKLMN", kind: "character", title: "Nessa Quill" },
  ]);
  const beats = conversationTranscriptBeats({ beats: [
    { id: "turn_opaque_1234567890", kind: "speech", speaker: "Mara Vale", addressee: "Nessa Quill", text: "Move, char_ABCDEFGHIJKLMN." },
    { id: "turn_opaque_1234567891", kind: "action", actors: ["Nessa Quill"], text: "Nessa catches the door." },
    { id: "turn_opaque_1234567892", kind: "speech", speaker: "Nessa Quill", interrupts: "turn_opaque_1234567890", text: "Too late." },
  ] }, registry);
  assert.deepEqual(beats.map((beat) => beat.kind), ["speech", "action", "speech"]);
  assert.equal(beats[0].addressee, "Nessa Quill");
  assert.deepEqual(beats[1].actors, ["Nessa Quill"]);
  assert.deepEqual(beats[2].interruption, { speaker: "Mara Vale", text: "Move, Nessa Quill." });
  assert.equal("interrupts" in beats[2], false);
  assert.equal("id" in beats[2], false);
  assert.equal(JSON.stringify(beats).includes("char_ABCDEFGHIJKLMN"), false);
  assert.equal(JSON.stringify(beats).includes("turn_opaque_1234567890"), false);
  assert.deepEqual(conversationTranscriptBeats({ verbatimTurns: [{ speaker: "Mara Vale", text: "The old transcript still reads." }] }, registry).map((beat) => beat.kind), ["speech"]);
  assert.match(appSource, /conversationTranscriptBeats\(data, state\.registry\)/);
  assert.match(appSource, /Cuts in on \$\{beat\.interruption\.speaker\}'s earlier line/);
  assert.doesNotMatch(appSource, /conversation-interruption-group/);
  assert.match(stylesheetSource, /\.conversation-beat--action/);
});

test("timeline view consumes the bounded timeline read contract instead of fetching every detail", () => {
  assert.match(appSource, /api\/timeline/);
  assert.match(appSource, /entriesFromTimeline/);
  assert.doesNotMatch(appSource, /timeBearing\.map/);
  assert.match(appSource, /equally spaced; the gaps do not indicate elapsed duration/);
});

test("timeline ordering and horizon coordinates preserve adjacent signed-64 ticks exactly", () => {
  const justBeforeMax = { timeline: "main", tick: "9223372036854775806", order: "2147483647" };
  const max = { timeline: "main", tick: "9223372036854775807", order: "-2147483648" };
  const min = { timeline: "main", tick: "-9223372036854775808", order: "0" };
  assert.equal(compareStoryTime(justBeforeMax, max), -1);
  assert.equal(compareStoryTime(max, justBeforeMax), 1);
  assert.equal(compareStoryTime(min, justBeforeMax), -1);
  const scoped = new URL(conversationRequestPath("conv_123", "as-of", max), "http://wedl.test").searchParams;
  assert.equal(scoped.get("tick"), "9223372036854775807");
  assert.equal(scoped.get("order"), "-2147483648");
});

test("timeline entries retain authored state, lifecycle state, and same-coordinate closure", () => {
  const entries = chronologyEntries({
    points: [{ at: { timeline: "main", tick: "7", order: "0" }, kind: "plot-transition", status: "draft", lifecycleState: "active", summary: "A lead appears.", entity: { id: "sp_1", title: "A lead", kind: "story-point" } }],
    spans: [{ kind: "scene", status: "closed", summary: "A conclusion.", entity: { id: "scene_1", title: "A scene", kind: "scene" }, start: { timeline: "main", tick: "1", order: "0" }, current: { timeline: "main", tick: "9", order: "0" }, end: { timeline: "main", tick: "9", order: "0" } }],
  });
  assert.equal(entries.find((entry) => entry.kind === "plot-transition").status, "draft");
  assert.equal(entries.find((entry) => entry.kind === "plot-transition").lifecycleState, "active");
  assert.equal(entries.filter((entry) => entry.kind === "scene" && entry.at.tick === "9").length, 2);
  assert.deepEqual(entries.filter((entry) => entry.kind === "scene" && entry.at.tick === "9").map((entry) => entry.boundary), ["current", "end"]);
});

test("concurrent scene projection keeps one chronology while grouping active fronts and whereabouts", () => {
  const at = { timeline: "main", tick: "210", order: "0" };
  const later = { timeline: "main", tick: "211", order: "0" };
  const payload = { spans: [
    { kind: "scene", status: "active", entity: { id: "scene_south", title: "Southward Cut" }, start: { timeline: "main", tick: "200", order: "0" }, current: at, location: { id: "loc_road", title: "The South Road" }, participants: [{ id: "char_rhea", title: "Rhea" }] },
    { kind: "scene", status: "active", entity: { id: "scene_watch", title: "The Watchtower" }, start: { timeline: "main", tick: "205", order: "0" }, current: at, location: { id: "loc_tower", title: "The Watchtower" }, participants: [{ id: "char_pip", title: "Pip", from: at }] },
    { kind: "scene", status: "closed", entity: { id: "scene_old", title: "The Old Road" }, start: { timeline: "main", tick: "100", order: "0" }, current: { timeline: "main", tick: "110", order: "0" }, end: { timeline: "main", tick: "120", order: "0" }, location: { id: "loc_old", title: "Old Road" }, participants: [{ id: "char_jorund", title: "Jorund" }] },
  ] };
  assert.equal(sceneContainsMoment(payload.spans[0], at), true);
  assert.equal(sceneContainsMoment(payload.spans[2], at), false);
  assert.deepEqual(sceneSnapshotsAt(payload, at).map((scene) => scene.entity.title), ["Southward Cut", "The Watchtower"]);
  assert.deepEqual(sceneSnapshotsAt(payload, null, { activeSceneIds: ["scene_south", "scene_watch"] }).map((scene) => scene.entity.title), ["Southward Cut", "The Watchtower"]);
  const entries = chronologyEntries(payload).map((entry) => ({ ...entry, chronologyKind: entry.kind }));
  const groups = timelinePresentationGroups(entries.filter((entry) => compareStoryTime(entry.at, at) === 0));
  assert.equal(groups.length, 1);
  assert.equal(groups[0].type, "concurrent-scenes");
  assert.equal(groups[0].entries.length, 2);
  assert.equal(timelinePresentationGroups(groups[0].entries, at)[0].classification, "current");
  assert.equal(timelinePresentationGroups(groups[0].entries, { timeline: "main", tick: "209", order: "0" })[0].classification, "later");
  assert.match(appSource, /timelinePresentationGroups\(currentTimeline\(\), activeHorizon\(\)\)/);
  assert.match(appSource, /timeline-concurrent-scenes/);
  assert.match(stylesheetSource, /timeline-concurrent-scenes\.is-revealed::before/);
  assert.match(stylesheetSource, /timeline-concurrent-scenes::before \{ background: #718796/);
  assert.equal(sceneSnapshotsAt(payload, later).length, 2, "open scenes remain visible at a later selected horizon");
});

test("same-coordinate scene and event cards are honestly presented as Meanwhile", () => {
  const at = { timeline: "main", tick: "210", order: "4" };
  const groups = timelinePresentationGroups([
    { at, chronologyKind: "scene", boundary: "current", entity: { title: "Southward Cut" } },
    { at, chronologyKind: "event", boundary: "point", entity: { title: "A signal answers" } },
  ], at);
  assert.equal(groups.length, 1);
  assert.equal(groups[0].type, "meanwhile");
  assert.equal(groups[0].classification, "current");
  assert.equal(groups[0].entries.length, 2);
  assert.match(appSource, /"Meanwhile"/);
  assert.match(appSource, /entry\.chronologyKind !== "scene" && entry\.chronologyKind !== "event"/);
  assert.match(appSource, /entry\.chronologyKind === "event" \? "Involving "/);
  assert.match(stylesheetSource, /\.meanwhile-group/);
});

test("timeline card cast presence uses inclusive arrival and departure intervals", () => {
  const arrival = { timeline: "main", tick: "210", order: "2" };
  const departure = { timeline: "main", tick: "210", order: "7" };
  const entry = {
    at: { timeline: "main", tick: "210", order: "5" },
    participants: [
      { id: "char_rhea", title: "Rhea", from: { timeline: "main", tick: "200", order: "0" }, to: departure },
      { id: "char_pip", title: "Pip", from: arrival, to: departure },
      { id: "char_jorund", title: "Jorund", from: { timeline: "main", tick: "210", order: "6" } },
    ],
  };
  assert.deepEqual(entry.participants.filter((participant) => referencePresentAt(participant, entry.at)).map((participant) => participant.title), ["Rhea", "Pip"]);
  assert.equal(referencePresentAt(entry.participants[1], arrival), true, "arrival is inclusive");
  assert.equal(referencePresentAt(entry.participants[1], departure), true, "departure is inclusive");
  assert.match(appSource, /entry\.participants \|\| \[\]\)\.filter\(\(participant\) => referencePresentAt\(participant, entry\.at\)\)/);
});

test("timeline selection honors the declared default, resets a foreign horizon, and labels the named origin", () => {
  const declarations = [
    { id: "main", label: "Main chronology", origin: { tick: -100, label: "The first record" } },
    { id: "after", label: "Aftermath" },
  ];
  assert.equal(selectedTimelineId("", "after", declarations), "after");
  assert.equal(selectedTimelineId("main", "after", declarations), "main");
  assert.equal(horizonForTimeline({ timeline: "main", tick: "9", order: "0" }, "after"), null);
  assert.deepEqual(horizonForTimeline({ timeline: "main", tick: "9", order: "0" }, "main"), { timeline: "main", tick: "9", order: "0" });
  assert.equal(originLabel(declarations, "main"), "The first record");
  assert.equal(originLabel(declarations, "after"), "");
  assert.equal(timelineDisplayLabel({ id: "timeline_internal_01" }), "Untitled chronology");
  assert.equal(timelineDisplayLabel(declarations[0]), "Main chronology");
  assert.equal(trailPositionLabel({ timeline: "main", tick: "8", order: "0" }, { timeline: "main", tick: "9", order: "0" }), "Past");
  assert.equal(trailPositionLabel({ timeline: "main", tick: "9", order: "0" }, { timeline: "main", tick: "9", order: "0" }), "At selected moment");
  assert.equal(trailPositionLabel({ timeline: "main", tick: "10", order: "0" }, { timeline: "main", tick: "9", order: "0" }), "Beyond selected moment");
});

test("author horizon is inclusive, filters later entries, and keeps exact same-tick ordering", () => {
  const horizon = { timeline: "main", tick: "17", order: "3" };
  const entries = [
    { boundary: "start", at: { timeline: "main", tick: "17", order: "3" }, entity: { id: "scene_early" } },
    { boundary: "point", at: { timeline: "main", tick: "17", order: "4" }, entity: { id: "event_later" } },
  ];
  assert.equal(isAtOrBeforeHorizon(entries[0].at, horizon), true);
  assert.equal(isAtOrBeforeHorizon(entries[1].at, horizon), false);
  assert.equal(classifyStoryMoment(entries[0].at, horizon), "current");
  assert.equal(classifyStoryMoment(entries[1].at, horizon), "later");
  assert.equal(classifyStoryMoment({ timeline: "branch", tick: "1", order: "0" }, horizon), "later");
  assert.equal(isEntityAvailable(entries, "scene_early", horizon), true);
  assert.equal(isEntityAvailable(entries, "event_later", horizon), false);
  assert.equal(isEntityAvailable(entries, "place_without_a_beat", horizon), true);
});

test("navigation state serializes article, timeline, horizon, and index position without changing URLs", () => {
  const state = historyAction({ view: "index", activeTimelineId: "main", kind: "scene", query: "quiet", horizon: { timeline: "main", tick: "17", order: "3" } }, { type: "open-article", entryId: "scene_opaque", returnView: "timeline" });
  const snapshot = navigationSnapshot(state, { listScroll: 240 });
  assert.deepEqual(restoreNavigation(snapshot), { view: "article", mobilePane: "article", returnView: "timeline", entryId: "scene_opaque", timelineId: "main", horizon: { timeline: "main", tick: "17", order: "3" }, kind: "scene", query: "quiet", listScroll: 240 });
  assert.equal(restoreNavigation({ view: "nonsense", listScroll: -1 }).view, "index");
  assert.match(appSource, /history\.pushState\(snapshot, "", location\.pathname\)/);
  assert.doesNotMatch(appSource, /location\.search|URLSearchParams\(location/);
});

test("navigation persists an explicit mobile nav-only pane and duplicate-coordinate horizon anchor", () => {
  const snapshot = navigationSnapshot({ view: "index", mobilePane: "nav", selectedEntityId: "scene_1", activeTimelineId: "main", horizon: { timeline: "main", tick: "4", order: "0", anchorId: "scene_1" } });
  assert.equal(restoreNavigation(snapshot).mobilePane, "nav");
  assert.equal(restoreNavigation(snapshot).horizon.anchorId, "scene_1");
  assert.equal(restoreNavigation({ view: "article", mobilePane: "nav" }).mobilePane, "article");
  assert.equal(historyAction({ view: "index", mobilePane: "nav" }, { type: "open-timeline" }).mobilePane, "article");
  assert.match(appSource, /mobilePane/);
  assert.match(appSource, /restoreRequestId/);
  assert.match(appSource, /anchorId/);
  assert.match(appSource, /writeHistory\("replace"\)/);
  assert.match(appSource, /invalidateTimeline\(\)/);
  assert.equal(restoreNavigation({ view: "whereabouts" }).view, "whereabouts");
  assert.equal(historyAction({ view: "index" }, { type: "open-whereabouts" }).mobilePane, "article");
  assert.equal(restoreNavigation({ view: "article", returnView: "whereabouts" }).returnView, "whereabouts");
  const possibility = navigationSnapshot({ view: "possibilities", activeTimelineId: "main", horizon: { timeline: "main", tick: "4", order: "0" } });
  assert.equal(restoreNavigation(possibility).view, "possibilities");
  assert.equal(restoreNavigation(possibility).mobilePane, "article");
  assert.equal(restoreNavigation(possibility).horizon, null, "Possibilities snapshots never retain a canonical reading horizon");
  assert.equal(restoreNavigation({ view: "possibilities", horizon: { timeline: "main", tick: "4", order: "0" } }).horizon, null, "legacy Possibilities snapshots discard stale horizons");
  assert.equal(historyAction({ view: "index", horizon: { timeline: "main", tick: "4", order: "0" } }, { type: "open-possibilities" }).horizon, null);
  assert.match(appSource, /const applicable = state\.view !== "possibilities" && state\.horizonEntries\.size > 0/);
  assert.match(appSource, /state\.horizon = null; setView\("possibilities"\)/);
});

test("kind search follows a browse selection and a changed search selector updates the visible section", () => {
  assert.match(appSource, /function browse\(kind, label\).*state\.searchKind = kind/s);
  assert.match(appSource, /state\.kind = kind; state\.searchKind = kind/);
  assert.match(appSource, /el\.searchKind\.addEventListener\("change", \(\) => \{ state\.searchKind = el\.searchKind\.value; state\.kind = state\.searchKind; updateIndexHeading\(\); syncNavigation\(\);/);
  assert.match(appSource, /function updateIndexHeading\(\)/);
});

test("horizon choices dedupe coordinates and preserve an older anchor by coordinate", () => {
  assert.match(appSource, /function horizonCoordinateKey\(at\)/);
  assert.match(appSource, /if \(state\.horizonEntries\.has\(key\)\) continue/);
  assert.match(appSource, /compareStoryTime\(item\.at, previous\) === 0/);
  assert.doesNotMatch(appSource, /anchorId: entry\.entity\.id/);
});

test("a delayed popstate restore cannot repaint after a newer browse", async () => {
  const navigation = createNavigationGeneration();
  let resolveTimeline;
  const delayedTimeline = new Promise((resolve) => { resolveTimeline = resolve; });
  const paints = [];

  const restore = async () => {
    const generation = navigation.begin();
    const timeline = await delayedTimeline;
    if (navigation.isCurrent(generation)) paints.push(`restore:${timeline}`);
  };

  const pendingRestore = restore();
  const browseGeneration = navigation.begin();
  if (navigation.isCurrent(browseGeneration)) paints.push("browse:index");
  resolveTimeline("main");
  await pendingRestore;

  assert.deepEqual(paints, ["browse:index"]);
});

test("a delayed restore search cannot overwrite a newer navigation result", async () => {
  const navigation = createNavigationGeneration();
  let restoreRequestId = 0;
  let resolveSearch;
  const delayedSearch = new Promise((resolve) => { resolveSearch = resolve; });
  const state = { searchResults: ["newer search"] };

  const restoreSearch = async () => {
    const requestId = ++restoreRequestId;
    const generation = navigation.begin();
    const payload = await delayedSearch;
    if (!navigation.isCurrent(generation) || requestId !== restoreRequestId) return;
    state.searchResults = payload;
  };

  const pendingRestore = restoreSearch();
  navigation.begin();
  restoreRequestId += 1;
  resolveSearch(["stale restore"]);
  await pendingRestore;

  assert.deepEqual(state.searchResults, ["newer search"]);
  assert.match(appSource, /const refreshed = await refreshAuthorSearch\(\(\) => api\.get\(authorSearchRequestPath\(state\.query, activeHorizon\(\)\)\)/);
  assert.match(appSource, /if \(!currentRestore\(\)\) return; state\.searchResults = refreshed\.results;/);
});

test("browsing away from delayed article and timeline loads clears their visible loading state", async () => {
  const navigation = createNavigationGeneration();
  const article = { attributes: new Map(), setAttribute(name, value) { this.attributes.set(name, value); } };
  const entities = { attributes: new Map(), setAttribute(name, value) { this.attributes.set(name, value); } };
  const articleStatus = { textContent: "" };
  const entitiesStatus = { textContent: "" };
  let resolveDetail;
  let resolveTimeline;
  const delayedDetail = new Promise((resolve) => { resolveDetail = resolve; });
  const delayedTimeline = new Promise((resolve) => { resolveTimeline = resolve; });

  const startLoading = async (status, response) => {
    const generation = navigation.begin();
    article.setAttribute("aria-busy", "true");
    articleStatus.textContent = status;
    await response;
    if (navigation.isCurrent(generation)) articleStatus.textContent = "Loaded";
  };

  const pendingDetail = startLoading("Opening The Flood Register…", delayedDetail);
  const pendingTimeline = startLoading("Gathering the story timeline…", delayedTimeline);
  navigation.begin();
  clearSupersededNavigationLoading({ article, articleStatus, entities, entitiesStatus });
  resolveDetail();
  resolveTimeline();
  await Promise.all([pendingDetail, pendingTimeline]);

  assert.equal(article.attributes.get("aria-busy"), "false");
  assert.equal(entities.attributes.get("aria-busy"), "false");
  assert.equal(articleStatus.textContent, "");
  assert.doesNotMatch(articleStatus.textContent, /Opening|Gathering/);
});

test("article rendering receives contract-shaped horizon filters and mobile navigation uses one active pane", () => {
  assert.match(appSource, /includeReference: \(id\) => available/);
  assert.match(appSource, /includeTransition: \(transition\) => !transition \|\| !transition\.time \|\| isAtOrBeforeHorizon/);
  assert.match(appSource, /horizon: activeHorizon\(\)/);
  assert.match(appSource, /inboundReferences: detail\.inboundReferences/);
  assert.match(appSource, /window\.addEventListener\("popstate"/);
  assert.match(appSource, /timelineCache\.has/);
  assert.match(appSource, /Back to list/);
  assert.match(appSource, /authorSearchRequestPath\(state\.query, activeHorizon\(\)\)/);
  assert.match(appSource, /renderCharacterMoment/);
  assert.match(appSource, /Full story is not one story moment/);
  assert.match(appSource, /The recorded state could not be loaded/);
  assert.match(appSource, /Found in \$\{state\.searchMatch\.heading\}/);
  assert.match(appSource, /search-grounding/);
  assert.match(appSource, /trailPositionLabel/);
  assert.match(appSource, /classifyStoryMoment/);
  assert.match(appSource, /locationContext: detail\.locationContext/);
  assert.match(appSource, /Journey trail/);
  assert.match(appSource, /Authored record/);
  assert.match(appSource, /Starting story status/);
  assert.match(appSource, /Status at this reading/);
  assert.match(appSource, /Recorded narrative outcome/);
  assert.match(htmlSource, /<article class="lore-article" aria-labelledby="article-heading">/);
  assert.match(appSource, /heading\.id = "article-heading"/);
  assert.match(appSource, /articleHeading\(model\.title\)/);
  assert.match(appSource, /articleHeading\("Story trail"\)/);
  assert.doesNotMatch(appSource, /api\/timeline\?timeline=.*Promise\.all/);
});

test("mobile pane changes focus visible index or article content, including failed reads", () => {
  assert.match(appSource, /heading\.tabIndex = -1/);
  assert.match(appSource, /function focusArticle\(\) \{ const target = el\.article\.querySelector\("#article-heading"\) \|\| el\.article; target\.focus\(\); \}/);
  assert.match(appSource, /renderIndex\(\{ focus: isMobileLayout\(\), navigationGeneration: generation \}\)/);
  assert.match(appSource, /renderArticleMessage\(`Opening \$\{name\(entity\)\}`, "Loading this lore entry…"\); if \(focus\) focusArticle\(\);/);
  assert.match(appSource, /renderArticleMessage\("Lore unavailable", "The requested lore could not be loaded\. Please try again\."\); text\(el\.articleStatus, ""\); if \(focus\) focusArticle\(\);/);
  assert.match(appSource, /setView\("timeline"\); renderArticleMessage\("Opening story trail", "Gathering the story timeline…"\); focusArticle\(\);/);
  assert.match(appSource, /renderArticleMessage\("Timeline unavailable", "The requested lore could not be loaded\. Please try again\."\); text\(el\.articleStatus, ""\); focusArticle\(\);/);
});

test("future timeline cards retain normal-text contrast without parent opacity", () => {
  const futureRule = stylesheetSource.match(/\.timeline-entry\.is-future\s*\{([^}]*)\}/)?.[1] || "";
  assert.ok(futureRule, "future timeline cards have an explicit visual treatment");
  assert.doesNotMatch(futureRule, /opacity\s*:/i);
  assert.doesNotMatch(stylesheetSource, /\.timeline-entry\.is-future\s*\{[^}]*opacity\s*:/i);
  assert.ok(contrastRatio("#abc4d5", "#14212d") >= 4.5, "future kicker and trail text meets WCAG AA normal-text contrast");
  assert.match(stylesheetSource, /\.timeline-entry\.is-revealed/);
  assert.match(stylesheetSource, /\.timeline-rail\s*\{[^}]*#3d5665/);
});
