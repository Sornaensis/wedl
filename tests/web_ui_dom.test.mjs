// A tiny DOM harness keeps this dependency-free while exercising the module
// that the browser actually loads.  It is intentionally narrower than a
// browser emulator: it verifies a successful whereabouts read replaces the
// loading placeholder with visible author-facing content.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

class ClassList {
  constructor(owner) { this.owner = owner; this.values = new Set(); }
  add(...values) { values.forEach((value) => this.values.add(value)); }
  toggle(value, force) { if (force === false) this.values.delete(value); else this.values.add(value); }
  contains(value) { return this.values.has(value); }
}

class Element {
  constructor(tag = "div") { this.tagName = tag; this.attributes = new Map(); this.children = []; this.classList = new ClassList(this); this.dataset = {}; this.events = new Map(); this.hidden = false; this.id = ""; this.className = ""; this.value = ""; this._text = ""; }
  append(...values) { this.children.push(...values.filter((value) => value != null)); }
  replaceChildren(...values) { this.children = values.filter((value) => value != null); this._text = ""; }
  set textContent(value) { this.children = []; this._text = String(value); }
  get textContent() { return `${this._text}${this.children.map((child) => typeof child === "string" ? child : child.textContent).join("")}`; }
  get childElementCount() { return this.children.filter((child) => child instanceof Element).length; }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  removeAttribute(name) { this.attributes.delete(name); }
  addEventListener(name, listener) { this.events.set(name, listener); }
  click() { this.events.get("click")?.({ preventDefault() {} }); }
  dispatch(name) { this.events.get(name)?.({ preventDefault() {} }); }
  focus() { this.focused = true; }
  querySelectorAll(selector) {
    const matches = (element) => selector.startsWith(".") ? (element.className.split(/\s+/).includes(selector.slice(1)) || element.classList.contains(selector.slice(1))) : (selector.startsWith("#") ? element.id === selector.slice(1) : false);
    const found = [];
    const visit = (value) => { if (!(value instanceof Element)) return; if (matches(value)) found.push(value); value.children.forEach(visit); };
    this.children.forEach(visit); return found;
  }
  querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
}

function makeDom() {
  const named = new Map(); const make = (id, tag = "div") => { const element = new Element(tag); element.id = id; named.set(`#${id}`, element); return element; };
  const nav = new Element("nav"); nav.className = "compendium-nav"; named.set(".compendium-nav", nav);
  const shell = new Element("main"); shell.className = "compendium-shell"; named.set(".compendium-shell", shell);
  const all = new Element("button"); all.className = "nav-item"; all.dataset.kind = "";
  const timeline = new Element("button"); timeline.className = "nav-item"; timeline.dataset.view = "timeline";
  const whereabouts = new Element("button"); whereabouts.className = "nav-item"; whereabouts.dataset.view = "whereabouts";
  const possibilities = new Element("button"); possibilities.className = "nav-item"; possibilities.dataset.view = "possibilities";
  nav.append(all, timeline, whereabouts, possibilities);
  const elements = {
    appStatus: make("app-status"), article: make("entity-detail-content"), articleStatus: make("article-status"), back: make("back-to-timeline", "button"),
    entities: make("entities"), entitiesForm: make("entities-form", "form"), entitiesStatus: make("entities-status"), entityText: make("entity-text", "input"), searchKind: make("search-kind", "select"), horizon: make("author-horizon", "select"), retrySearch: make("retry-search", "button"), searchRecovery: make("search-recovery"),
    indexHeading: make("entries-heading"), mobileBack: make("back-to-navigation", "button"), nav, shell, worldName: make("world-name"), timeline, whereabouts, possibilities,
  };
  const document = {
    querySelector(selector) { return named.get(selector) || null; },
    createElement(tag) {
      if (!/^[A-Za-z][A-Za-z0-9-]*$/.test(tag)) throw new DOMException(`Invalid element name: ${tag}`, "InvalidCharacterError");
      return new Element(tag);
    },
    createDocumentFragment() { return new Element("fragment"); },
  };
  return { document, elements };
}

const waitForUi = () => new Promise((resolve) => setTimeout(resolve, 0));
const dataModule = (source) => `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
let browserModuleNonce = 0;

async function loadBrowserModule() {
  const read = async (name) => readFile(new URL(`../src/wedl/static/${name}`, import.meta.url), "utf8");
  const timeline = dataModule(await read("timeline.mjs"));
  const lore = dataModule((await read("lore.mjs")).replace("./timeline.mjs", timeline));
  const search = dataModule((await read("search.mjs")).replace("./lore.mjs", lore));
  const api = dataModule(await read("api.js")); const query = dataModule(await read("query.mjs")); const navigation = dataModule(await read("navigation.mjs"));
  let app = await read("app.js");
  for (const [relative, module] of Object.entries({ "./api.js": api, "./query.mjs": query, "./lore.mjs": lore, "./search.mjs": search, "./navigation.mjs": navigation, "./timeline.mjs": timeline })) app = app.replace(relative, module);
  return import(dataModule(`${app}\n// browser test instance ${++browserModuleNonce}`));
}

test("successful whereabouts load replaces its placeholder with visible independent locations and journeys", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const calls = [];
  globalThis.document = document;
  globalThis.location = { pathname: "/" };
  globalThis.history = { state: null, pushState() {}, replaceState() {} };
  globalThis.requestAnimationFrame = (callback) => callback();
  globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  const entities = [
    { id: "world_story", kind: "world", title: "The Test World" },
    { id: "char_rhea", kind: "character", title: "Rhea" },
    { id: "loc_road", kind: "location", title: "Warden Road" },
    { id: "event_move", kind: "event", title: "Rhea reaches the road" },
    { id: "scene_patrol", kind: "scene", title: "Road patrol" },
  ];
  globalThis.fetch = async (path) => {
    calls.push(path);
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { recordCount: entities.length, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? entities
          : path === "/api/entities/char_rhea" ? { ...entities[1], bodyMarkdown: "A veteran.", frontmatter: { role: "shield-veteran" } }
            : path === "/api/entities/event_move" ? { ...entities[3], bodyMarkdown: "A move is recorded.", frontmatter: {} }
          : path === "/api/whereabouts" ? {
            effectiveTime: { timeline: "main", tick: "5", order: "0" }, activeScenes: [{ scene: entities[4], location: entities[2], characters: [entities[1]] }],
            characters: [
              { character: entities[1], presence: "offstage", location: entities[2], lastKnownLocation: null, activeScene: null, journey: [{ kind: "initial", at: null, from: null, to: entities[2], event: null }, { kind: "move", at: { timeline: "main", tick: "5", order: "0" }, from: null, to: entities[2], event: entities[3] }] },
              { character: { id: "char_lost", kind: "character", title: "Lost Scout" }, presence: "unlocated", location: null, lastKnownLocation: entities[2], activeScene: null, journey: [{ kind: "clear", at: { timeline: "main", tick: "4", order: "0" }, from: entities[2], to: null, event: entities[3] }] },
            ],
          } : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule();
    await waitForUi(); await waitForUi();
    const rhea = elements.entities.querySelectorAll(".entity-select").find((button) => button.dataset.entityId === "char_rhea");
    rhea.click(); await waitForUi(); await waitForUi();
    elements.whereabouts.click(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Whereabouts/);
    assert.match(elements.article.textContent, /Active story fronts/);
    assert.match(elements.article.textContent, /Road patrol/);
    assert.match(elements.article.textContent, /Warden Road/);
    assert.match(elements.article.textContent, /Starting place:/);
    assert.match(elements.article.textContent, /At the story’s beginning/);
    assert.match(elements.article.textContent, /At current authored moment/);
    assert.match(elements.article.textContent, /No current place recorded/);
    assert.match(elements.article.textContent, /Shield Veteran/);
    assert.doesNotMatch(elements.article.textContent, /Opening whereabouts|Gathering the recorded locations/);
    assert.doesNotMatch(elements.article.textContent, /Whereabouts unavailable/);
    assert.equal(calls.filter((path) => path === "/api/whereabouts").length, 1);
    assert.equal(calls.filter((path) => path === "/api/entities/char_rhea").length, 1, "the cached character detail supplies the role without a second read");
    const eventLink = elements.article.querySelectorAll(".lore-link").find((button) => button.textContent === "Rhea reaches the road");
    eventLink.click(); await waitForUi(); await waitForUi();
    elements.back.click(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Whereabouts/, "a lore link opened from whereabouts returns there, not to the timeline");
  } finally {
    Object.assign(globalThis, previous);
  }
});

test("Possibilities is text-safe, has no opaque IDs or edit controls, and restores as a non-horizon view", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const historyWrites = [];
  globalThis.document = document; globalThis.location = { pathname: "/" };
  globalThis.history = { state: null, pushState(value) { historyWrites.push(value); this.state = value; }, replaceState(value) { historyWrites.push(value); this.state = value; } };
  globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  const entities = [{ id: "world_story", kind: "world", title: "The Test World" }];
  globalThis.fetch = async (path) => {
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { recordCount: 1, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? entities
          : path === "/api/hypotheses" ? { nonCanonical: true, hypotheses: [{ title: "<b>hyp_01SECRET</b>", status: "open", nonCanonical: true, statement: "<img src=x> hyp_01SECRET", subjects: [{ kind: "character", title: "Mara hyp_01SECRET" }], alternatives: ["<script>bad</script> hyp_01SECRET"], context: "hyp_01SECRET", placement: { context: "<em>hyp_01SECRET</em>" } }, { title: "Rejected reading", status: "rejected", nonCanonical: true, statement: "An earlier possibility.", subjects: [], alternatives: [], context: "Archive review", placement: {}, resolution: { note: "The sealed ledger rules it out." } }] }
            : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    elements.possibilities.click(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Possibilities/);
    assert.match(elements.article.textContent, /Non-canonical/);
    assert.match(elements.article.textContent, /Retained rejection note: The sealed ledger rules it out\./);
    assert.doesNotMatch(elements.article.textContent, /hyp_01SECRET/);
    assert.match(elements.article.textContent, /<script>bad<\/script>/, "hostile markup is rendered as text, not as HTML");
    assert.equal(elements.article.querySelectorAll(".lore-link").length, 0, "possibilities have no entity navigation or edit controls");
    assert.equal(elements.article.querySelectorAll(".edit").length, 0);
    assert.equal(historyWrites.at(-1).view, "possibilities");
    assert.equal(historyWrites.at(-1).horizon, null, "possibilities does not manufacture a horizon");
    assert.doesNotMatch(elements.article.textContent, /Possibilities unavailable/, "a successful nonempty response renders its author notes");
  } finally { Object.assign(globalThis, previous); }
});

test("Possibilities clears the active horizon across history and opens Whereabouts at the current moment", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const calls = []; const historyWrites = []; const windowListeners = new Map();
  globalThis.document = document; globalThis.location = { pathname: "/" };
  globalThis.history = { state: null, pushState(value) { historyWrites.push(value); this.state = value; }, replaceState(value) { historyWrites.push(value); this.state = value; } };
  globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener(name, listener) { windowListeners.set(name, listener); }, matchMedia: () => ({ matches: false }) };
  const horizon = { timeline: "main", tick: "5", order: "0" };
  const entities = [{ id: "world_story", kind: "world", title: "The Test World" }, { id: "event_signal", kind: "event", title: "A signal answers" }];
  globalThis.fetch = async (path) => {
    calls.push(path);
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { recordCount: entities.length, timeModel: { timelineDeclarations: [{ id: "main", label: "Main" }], defaultTimeline: "main" } }
        : path === "/api/entities" ? entities
          : path === "/api/timeline?timeline=main" ? { timeline: { id: "main" }, points: [{ at: horizon, kind: "event", entity: entities[1] }] }
            : path === "/api/hypotheses" ? { nonCanonical: true, hypotheses: [] }
              : path === "/api/whereabouts" ? { effectiveTime: horizon, activeScenes: [], characters: [] }
                : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    elements.timeline.click(); await waitForUi(); await waitForUi();
    elements.horizon.value = "main\u00005\u00000"; elements.horizon.dispatch("change"); await waitForUi(); await waitForUi();
    const timelineSnapshot = historyWrites.at(-1);
    assert.deepEqual(timelineSnapshot.horizon, horizon, "the selected timeline moment is saved exactly");
    elements.possibilities.click(); await waitForUi(); await waitForUi();
    const possibilitiesSnapshot = historyWrites.at(-1);
    assert.equal(possibilitiesSnapshot.view, "possibilities");
    assert.equal(possibilitiesSnapshot.horizon, null, "Possibilities is saved outside the author horizon");
    assert.match(elements.article.textContent, /No possibilities have been recorded\./, "an empty successful response has its own rendered view");
    assert.doesNotMatch(elements.article.textContent, /Possibilities unavailable/);
    assert.equal(elements.horizon.disabled, true, "the horizon control is unavailable for non-canonical possibilities");
    windowListeners.get("popstate")({ state: timelineSnapshot }); await waitForUi(); await waitForUi();
    assert.equal(elements.horizon.disabled, false);
    assert.equal(elements.horizon.value, "main\u00005\u00000", "Back restores the exact selected horizon");
    windowListeners.get("popstate")({ state: possibilitiesSnapshot }); await waitForUi(); await waitForUi();
    assert.equal(elements.horizon.value, "", "Forward restores Possibilities with no stale horizon selection");
    elements.whereabouts.click(); await waitForUi(); await waitForUi();
    assert.equal(calls.at(-1), "/api/whereabouts", "Whereabouts from Possibilities uses the world current moment, not stale query parameters");
  } finally { Object.assign(globalThis, previous); }
});

test("Possibilities refreshes a scoped search for the full story before returning to its index", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const calls = [];
  globalThis.document = document; globalThis.location = { pathname: "/" };
  globalThis.history = { state: null, pushState(value) { this.state = value; }, replaceState(value) { this.state = value; } };
  globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  const horizon = { timeline: "main", tick: "5", order: "0" };
  const entities = [{ id: "world_story", kind: "world", title: "The Test World" }, { id: "event_signal", kind: "event", title: "A signal answers" }];
  globalThis.fetch = async (path) => {
    calls.push(path);
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { recordCount: entities.length, timeModel: { timelineDeclarations: [{ id: "main", label: "Main" }], defaultTimeline: "main" } }
        : path === "/api/entities" ? entities
          : path === "/api/timeline?timeline=main" ? { timeline: { id: "main" }, points: [{ at: horizon, kind: "event", entity: entities[1] }] }
            : path === "/api/search?q=signal&perspective=author&timeline=main&tick=5&order=0" ? { results: [{ entityId: "event_signal", heading: "Observation", snippet: "Scoped result only." }] }
              : path === "/api/search?q=signal&perspective=author&allTime=true" ? { results: [{ entityId: "event_signal", heading: "Observation", snippet: "Full-story result." }] }
                : path === "/api/hypotheses" ? { nonCanonical: true, hypotheses: [] }
                  : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    elements.horizon.value = "main\u00005\u00000"; elements.horizon.dispatch("change"); await waitForUi(); await waitForUi();
    elements.entityText.value = "signal"; elements.entitiesForm.dispatch("submit"); await waitForUi(); await waitForUi();
    assert.match(elements.entities.textContent, /Scoped result only/);
    elements.possibilities.click(); await waitForUi(); await waitForUi();
    assert.ok(calls.includes("/api/search?q=signal&perspective=author&allTime=true"), "Possibilities re-reads a scoped search at full-story scope");
    elements.back.click(); await waitForUi(); await waitForUi();
    assert.match(elements.entities.textContent, /Full-story result/, "Back to list renders the refreshed full-story search projection");
    assert.equal(elements.entitiesStatus.textContent, "1 matching entries");
  } finally { Object.assign(globalThis, previous); }
});

test("Possibilities restores horizon-filtered entries for an empty-query index", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom();
  globalThis.document = document; globalThis.location = { pathname: "/" };
  globalThis.history = { state: null, pushState(value) { this.state = value; }, replaceState(value) { this.state = value; } };
  globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  const early = { timeline: "main", tick: "5", order: "0" }; const later = { timeline: "main", tick: "10", order: "0" };
  const entities = [{ id: "world_story", kind: "world", title: "The Test World" }, { id: "event_early", kind: "event", title: "Early signal" }, { id: "event_later", kind: "event", title: "Later signal" }];
  globalThis.fetch = async (path) => {
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { recordCount: entities.length, timeModel: { timelineDeclarations: [{ id: "main", label: "Main" }], defaultTimeline: "main" } }
        : path === "/api/entities" ? entities
          : path === "/api/timeline?timeline=main" ? { timeline: { id: "main" }, points: [{ at: early, kind: "event", entity: entities[1] }, { at: later, kind: "event", entity: entities[2] }] }
            : path === "/api/hypotheses" ? { nonCanonical: true, hypotheses: [] }
              : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    elements.horizon.value = "main\u00005\u00000"; elements.horizon.dispatch("change"); await waitForUi(); await waitForUi();
    assert.doesNotMatch(elements.entities.textContent, /Later signal/, "the selected horizon hides later canonical entries");
    elements.possibilities.click(); await waitForUi(); await waitForUi();
    assert.match(elements.entities.textContent, /Later signal/, "entering Possibilities immediately rerenders the cleared full-story index");
    assert.equal(elements.entitiesStatus.textContent, "3 matching entries");
    elements.back.click(); await waitForUi(); await waitForUi();
    assert.match(elements.entities.textContent, /Later signal/, "the full-story index is rerendered when Possibilities clears the horizon");
    assert.equal(elements.entitiesStatus.textContent, "3 matching entries");
  } finally { Object.assign(globalThis, previous); }
});

test("Possibilities failure focuses its replacement heading", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom();
  globalThis.document = document; globalThis.location = { pathname: "/" };
  globalThis.history = { state: null, pushState() {}, replaceState() {} };
  globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.fetch = async (path) => {
    if (path === "/api/hypotheses") return { ok: false, json: async () => ({}), text: async () => "offline" };
    const payload = path === "/api/session" ? { token: "test" } : path === "/api/status" ? { recordCount: 1, timeModel: { timelineDeclarations: [], defaultTimeline: "" } } : [{ id: "world_story", kind: "world", title: "The Test World" }];
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    elements.possibilities.click(); await waitForUi(); await waitForUi();
    const heading = elements.article.querySelector("#article-heading");
    assert.match(heading.textContent, /Possibilities unavailable/);
    assert.equal(heading.focused, true, "the failure replacement heading receives focus");
  } finally { Object.assign(globalThis, previous); }
});

test("Possibilities reports a post-response render failure without claiming its API was unavailable", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); let hypothesesLoaded = false;
  globalThis.document = document; globalThis.location = { pathname: "/" };
  globalThis.history = { state: null, pushState() {}, replaceState() {} };
  globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.fetch = async (path) => {
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { recordCount: 1, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "The Test World" }]
          : path === "/api/hypotheses" ? { nonCanonical: true, hypotheses: [] } : {};
    if (path === "/api/hypotheses") hypothesesLoaded = true;
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    const replaceChildren = elements.article.replaceChildren.bind(elements.article);
    elements.article.replaceChildren = (...children) => {
      if (hypothesesLoaded) { hypothesesLoaded = false; throw new Error("injected post-response render failure"); }
      replaceChildren(...children);
    };
    elements.possibilities.click(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Possibilities display problem/);
    assert.doesNotMatch(elements.article.textContent, /Possibilities unavailable/, "a successful API response is not reported as an API failure when rendering fails");
  } finally { Object.assign(globalThis, previous); }
});

test("Whereabouts preserves its successful projection when saving its post-response history fails", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom();
  globalThis.document = document; globalThis.location = { pathname: "/" };
  globalThis.history = { state: null, pushState() { throw new Error("injected history failure"); }, replaceState() {} };
  globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.fetch = async (path) => {
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { recordCount: 1, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "The Test World" }]
          : path === "/api/whereabouts" ? { activeScenes: [], characters: [] } : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    elements.whereabouts.click(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Whereabouts/);
    assert.doesNotMatch(elements.article.textContent, /Whereabouts unavailable/, "a history failure after a 200 response keeps the valid projection visible");
    assert.match(elements.articleStatus.textContent, /could not save this navigation step/);
  } finally { Object.assign(globalThis, previous); }
});

test("Possibilities preserves its successful projection when post-response focus fails", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom();
  globalThis.document = document; globalThis.location = { pathname: "/" };
  globalThis.history = { state: null, pushState() {}, replaceState() {} };
  globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.fetch = async (path) => {
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { recordCount: 1, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "The Test World" }]
          : path === "/api/hypotheses" ? { nonCanonical: true, hypotheses: [] } : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    const querySelector = elements.article.querySelector.bind(elements.article);
    elements.article.querySelector = (selector) => {
      const target = querySelector(selector);
      return selector === "#article-heading" && target && target.textContent === "Possibilities" ? { focus() { throw new Error("injected focus failure"); } } : target;
    };
    elements.possibilities.click(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /No possibilities have been recorded\./);
    assert.doesNotMatch(elements.article.textContent, /Possibilities unavailable/, "a focus failure after a 200 response does not replace valid author notes with an API error");
    assert.match(elements.articleStatus.textContent, /could not receive focus/);
  } finally { Object.assign(globalThis, previous); }
});

test("Whereabouts sends exact signed horizons and caches each coordinate independently", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const calls = [];
  globalThis.document = document; globalThis.location = { pathname: "/" };
  globalThis.history = { state: null, pushState(value) { this.state = value; }, replaceState(value) { this.state = value; } };
  globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  const first = { timeline: "main", tick: "9223372036854775807", order: "-2147483648" };
  const second = { timeline: "main", tick: "9223372036854775806", order: "2147483647" };
  const entities = [{ id: "world_story", kind: "world", title: "The Test World" }, { id: "event_first", kind: "event", title: "First" }, { id: "event_second", kind: "event", title: "Second" }, { id: "char_scout", kind: "character", title: "Scout" }, { id: "loc_gate", kind: "location", title: "North Gate" }, { id: "scene_watch", kind: "scene", title: "Gate watch" }];
  globalThis.fetch = async (path) => {
    calls.push(path);
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { recordCount: entities.length, timeModel: { timelineDeclarations: [{ id: "main", label: "Main" }], defaultTimeline: "main" } }
        : path === "/api/entities" ? entities
          : path === "/api/timeline?timeline=main" ? { timeline: { id: "main" }, points: [{ at: second, kind: "event", entity: entities[2] }, { at: first, kind: "event", entity: entities[1] }] }
            : path.startsWith("/api/whereabouts?") ? { effectiveTime: first, activeScenes: [{ scene: entities[5], location: entities[4], characters: [entities[3]] }], characters: [{ character: entities[3], presence: "active-scene", location: entities[4], lastKnownLocation: null, activeScene: entities[5], journey: [{ kind: "move", at: first, from: null, to: entities[4], event: entities[1] }] }] }
              : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  const selectAndOpen = async (at) => { elements.horizon.value = `${at.timeline}\u0000${at.tick}\u0000${at.order}`; elements.horizon.dispatch("change"); await waitForUi(); await waitForUi(); elements.whereabouts.click(); await waitForUi(); await waitForUi(); };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    await selectAndOpen(first); assert.match(elements.article.textContent, /At selected moment/, "a selected horizon renders the matching authored journey"); assert.match(elements.article.textContent, /Gate watch/); assert.doesNotMatch(elements.article.textContent, /Whereabouts unavailable/); elements.back.click(); await waitForUi(); await waitForUi();
    await selectAndOpen(second); elements.back.click(); await waitForUi(); await waitForUi();
    await selectAndOpen(first);
    const whereaboutsCalls = calls.filter((path) => path.startsWith("/api/whereabouts?"));
    assert.deepEqual(whereaboutsCalls, [
      "/api/whereabouts?timeline=main&tick=9223372036854775807&order=-2147483648",
      "/api/whereabouts?timeline=main&tick=9223372036854775806&order=2147483647",
    ], "signed coordinates retain their exact query values and only an identical coordinate hits the cache");
  } finally { Object.assign(globalThis, previous); }
});
