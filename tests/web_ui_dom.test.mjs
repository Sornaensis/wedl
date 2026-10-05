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
  const chronology = new Element("button"); chronology.className = "nav-item"; chronology.dataset.view = "chronology";
  nav.append(all, timeline, whereabouts, possibilities, chronology);
  const elements = {
    appStatus: make("app-status"), article: make("entity-detail-content"), articleStatus: make("article-status"), back: make("back-to-timeline", "button"),
    entities: make("entities"), entitiesForm: make("entities-form", "form"), entitiesStatus: make("entities-status"), entityText: make("entity-text", "input"), searchKind: make("search-kind", "select"), horizon: make("author-horizon", "select"), retrySearch: make("retry-search", "button"), searchRecovery: make("search-recovery"), threadFilter: make("thread-filter", "fieldset"), threadFilterOptions: make("thread-filter-options"), threadFilterStatus: make("thread-filter-status"),
    indexHeading: make("entries-heading"), indexSort: make("index-sort", "select"), mobileBack: make("back-to-navigation", "button"), nav, shell, worldName: make("world-name"), timeline, whereabouts, possibilities, chronology,
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
  const api = dataModule(await read("api.js")); const chronology = dataModule(await read("chronology.mjs")); const query = dataModule(await read("query.mjs")); const navigation = dataModule(await read("navigation.mjs")); const sorting = dataModule(await read("sorting.mjs"));
  let app = await read("app.js");
  for (const [relative, module] of Object.entries({ "./api.js": api, "./chronology.mjs": chronology, "./query.mjs": query, "./lore.mjs": lore, "./search.mjs": search, "./sorting.mjs": sorting, "./navigation.mjs": navigation, "./timeline.mjs": timeline })) app = app.replace(relative, module);
  return import(dataModule(`${app}\n// browser test instance ${++browserModuleNonce}`));
}

test("event operations display named destinations and safe values with keyboard links", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window, WebSocket: globalThis.WebSocket, sessionStorage: globalThis.sessionStorage };
  const { document, elements } = makeDom(); const calls = [];
  const entities = [{ id: "world_test", kind: "world", title: "Test world" }, { id: "event_rescue", kind: "event", title: "Rescue" }, { id: "obj_token", kind: "object", title: "Token" }, { id: "char_mara", kind: "character", title: "Mara" }, { id: "loc_gate", kind: "location", title: "<svg onload=evil()>Gate</svg>" }];
  const effects = [
    { target: "obj_token", key: "holder", operation: "set", value: { entity: "char_mara", label: "extension" } },
    { target: "char_mara", key: "location", operation: "set", value: { entity: "loc_gate" } },
    { target: "char_mara", key: "condition", operation: "set", value: "injured" },
    { target: "char_mara", key: "count", operation: "set", value: 0 },
    { target: "char_mara", key: "awake", operation: "set", value: false },
    { target: "char_mara", key: "note", operation: "set", value: null },
    { target: "char_mara", key: "flags", operation: "add-to-set", value: "brave" },
    { target: "char_mara", key: "flags", operation: "remove-from-set", value: "afraid" },
    { target: "char_mara", key: "condition", operation: "clear" },
    { target: "obj_token", key: "holder", operation: "set", value: { entity: "char_1234567890ABC" } },
    { target: "char_mara", key: "note", operation: "set", value: '<img src=x onerror="evil()">' },
    { target: "char_mara", key: "nested", operation: "set", value: { entity: "char_mara", label: "data" } },
    { target: "char_mara", key: "literal", operation: "set", value: { entity: "char_mara" } },
    { target: "char_mara", key: "missing", operation: "set" },
  ];
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) }; globalThis.WebSocket = undefined; globalThis.sessionStorage = undefined;
  globalThis.fetch = async (path) => {
    calls.push(path);
    const entity = entities.find((entry) => path === `/api/entities/${encodeURIComponent(entry.id)}`);
    const payload = path === "/api/session" ? { token: "test" } : path === "/api/status" ? { revision: "r1", timeModel: { timelineDeclarations: [], defaultTimeline: "" } } : path === "/api/entities" ? entities : path === "/api/threads" ? { revision: "r1", groupingAvailable: false, threads: [] } : entity ? { ...entity, frontmatter: entity.kind === "event" ? { effects } : entity.kind === "world" ? { state_keys: { object: { holder: { type: "entity" } }, character: { location: { type: "entity" }, nested: { type: "object" }, literal: { type: "object" } } } } : {} } : {};
    return { ok: true, status: 200, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    elements.entities.querySelectorAll(".entity-select").find((button) => button.dataset.entityId === "event_rescue").click(); await waitForUi(); await waitForUi(); await waitForUi(); await waitForUi();
    const prose = elements.article.textContent;
    for (const expected of ["Holder — Set to: Mara", "{label: extension}", "Location — Set to: <svg onload=evil()>Gate</svg>", "Condition — Set to: injured", "Count — Set to: 0", "Awake — Set to: false", "Note — Set to: null", "Add to set: brave", "Remove from set: afraid", "Clear: no value remains", "Unavailable reference", "Unavailable value", '<img src=x onerror="evil()">', "entity: Unavailable reference; label: data", "Literal — Set to: {entity: Unavailable reference}"]) assert.ok(prose.includes(expected), `${expected}: ${prose}`);
    assert.match(prose, /Authored operations/); assert.doesNotMatch(prose, /char_1234567890ABC|char_mara|obj_token/);
    const tags = []; const visit = (entry) => { if (!(entry instanceof Element)) return; tags.push(entry.tagName); entry.children.forEach(visit); }; visit(elements.article);
    assert.ok(!tags.includes("svg") && !tags.includes("img"), "hostile source stays text, never markup");
    const destination = elements.article.querySelectorAll(".lore-link").find((button) => button.textContent === "<svg onload=evil()>Gate</svg>");
    assert.equal(destination.tagName, "button"); assert.equal(destination.type, "button"); destination.focus(); assert.equal(destination.focused, true); destination.click(); await waitForUi(); await waitForUi();
    assert.ok(calls.includes("/api/entities/loc_gate")); assert.match(elements.article.textContent, /Gate/);
  } finally { Object.assign(globalThis, previous); }
});

test("event value definitions fail honestly and stale revision reads cannot supply new article links", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window, WebSocket: globalThis.WebSocket, sessionStorage: globalThis.sessionStorage };
  const { document, elements } = makeDom(); let revision = "r1"; let releaseOld; let worldReads = 0;
  const entities = [{ id: "world_test", kind: "world", title: "World" }, { id: "event_test", kind: "event", title: "Rescue" }, { id: "obj_test", kind: "object", title: "Token" }, { id: "loc_test", kind: "location", title: "Harbor" }];
  class Socket { static instance; constructor() { this.events = new Map(); Socket.instance = this; } addEventListener(key, listener) { this.events.set(key, listener); } emit() { this.events.get("message")?.({ data: JSON.stringify({ revision }) }); } }
  const response = (payload) => ({ ok: true, status: 200, json: async () => payload, text: async () => "" });
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) }; globalThis.WebSocket = Socket; globalThis.sessionStorage = undefined;
  globalThis.fetch = async (path) => {
    if (path === "/api/entities/world_test") {
      worldReads += 1;
      if (revision === "r1") return { ok: false, status: 503, text: async () => "Unavailable" };
      if (revision === "r2") return new Promise((resolve) => { releaseOld = () => resolve(response({ revision: "r2", frontmatter: { state_keys: { object: { holder: { type: "entity" } } } } })); });
      return response({ revision, frontmatter: { state_keys: { object: { holder: { type: "object" } } } } });
    }
    const entity = entities.find((entry) => path === `/api/entities/${entry.id}`);
    return response(path === "/api/session" ? { token: "test" } : path === "/api/status" ? { revision, timeModel: { timelineDeclarations: [], defaultTimeline: "" } } : path === "/api/entities" ? entities : path === "/api/threads" ? { revision, groupingAvailable: false, threads: [] } : entity ? { ...entity, revision, frontmatter: entity.kind === "event" ? { effects: [{ target: "obj_test", key: "holder", operation: "set", value: { entity: "loc_test" } }] } : {} } : {});
  };
  const settle = async () => { for (let index = 0; index < 8; index += 1) await waitForUi(); };
  try {
    await loadBrowserModule(); await settle();
    elements.entities.querySelectorAll(".entity-select").find((button) => button.dataset.entityId === "event_test").click(); await settle();
    assert.match(elements.article.textContent, /State value types are unavailable/);
    assert.match(elements.article.textContent, /Holder — Set to: \{entity: Unavailable reference\}/);
    assert.ok(!elements.article.querySelectorAll(".lore-link").some((button) => button.textContent === "Harbor"));
    revision = "r2"; Socket.instance.emit(); await settle(); assert.equal(typeof releaseOld, "function");
    revision = "r3"; Socket.instance.emit(); await settle();
    assert.doesNotMatch(elements.article.textContent, /State value types are unavailable/);
    assert.match(elements.article.textContent, /Holder — Set to: \{entity: Unavailable reference\}/);
    releaseOld(); await settle();
    assert.match(elements.article.textContent, /Holder — Set to: \{entity: Unavailable reference\}/);
    assert.ok(!elements.article.querySelectorAll(".lore-link").some((button) => button.textContent === "Harbor"), "old entity type cannot become a destination under the new object type");
    assert.equal(worldReads, 4, "a stale read reconciles to the current revision instead of publishing its definitions");
  } finally { Object.assign(globalThis, previous); }
});

test("spatial lore handoff is consumed once and opens only a registry-backed ID", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history,
    location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame,
    window: globalThis.window, WebSocket: globalThis.WebSocket, sessionStorage: globalThis.sessionStorage };
  const { document, elements } = makeDom(); const calls = []; const storage = new Map([["wedl.spatial.lore.once", "place:known"]]);
  globalThis.document = document;
  globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" };
  globalThis.history = { state: null, pushState() {}, replaceState() {} };
  globalThis.requestAnimationFrame = (callback) => callback();
  globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.WebSocket = undefined;
  globalThis.sessionStorage = { getItem: (key) => storage.get(key) || null, removeItem: (key) => storage.delete(key) };
  globalThis.fetch = async (path) => {
    calls.push(path);
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision: "r1", recordCount: 2, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world:known", kind: "world", title: "Test world" }, { id: "place:known", kind: "location", title: "Known place" }]
          : path === "/api/threads" ? { revision: "r1", groupingAvailable: false, threads: [] }
            : path === "/api/entities/place%3Aknown" ? { id: "place:known", kind: "location", title: "Known place", body: "Authored lore", frontmatter: {} }
              : {};
    return { ok: true, status: 200, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    assert.equal(storage.has("wedl.spatial.lore.once"), false);
    assert.ok(calls.includes("/api/entities/place%3Aknown"));
    assert.match(elements.article.textContent, /Known place/);
    assert.equal(globalThis.location.pathname, "/");
  } finally { Object.assign(globalThis, previous); }
});

test("narrative group catalog is server-ordered, works in browse and search, and sends sorted ANY selectors", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window, WebSocket: globalThis.WebSocket };
  const { document, elements } = makeDom(); const calls = []; let revision = "r1";
  class WebSocketMock { static instances = []; constructor() { this.events = new Map(); WebSocketMock.instances.push(this); } addEventListener(name, listener) { this.events.set(name, listener); } emit(value) { this.events.get("message")?.({ data: JSON.stringify(value) }); } }
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: true }) }; globalThis.WebSocket = WebSocketMock;
  globalThis.fetch = async (path) => {
    calls.push(path);
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision, recordCount: 1, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "The Test World" }]
          : path === "/api/threads" ? revision === "r3" ? { protocol: "wedl-threads/v1", revision, sourceSchema: "wedl/v0.3", groupingAvailable: false, threads: [] } : { protocol: "wedl-threads/v1", revision, sourceSchema: "wedl/v0.5", groupingAvailable: true, threads: revision === "r1" ? [{ id: "thread_z", label: "Road" }, { id: "thread_a", label: "Archive" }] : [{ id: "thread_z", label: "Road" }] }
            : path.startsWith("/api/search") ? { protocol: "wedl-search/v5", revision, results: [] }
              : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    assert.equal(elements.threadFilter.hidden, false);
    assert.equal(elements.threadFilter.disabled, false, "browse can project selected narrative groups");
    assert.deepEqual(elements.threadFilterOptions.querySelectorAll(".thread-filter-option").map((input) => input.dataset.threadId), ["thread_z", "thread_a"], "the catalog keeps server display order");
    elements.entityText.value = "signal"; elements.entitiesForm.dispatch("submit"); await waitForUi(); await waitForUi();
    assert.equal(elements.threadFilter.disabled, false);
    assert.ok(calls.includes("/api/search?q=signal&perspective=author&allTime=true"), "no selector preserves the existing request exactly");
    const options = elements.threadFilterOptions.querySelectorAll(".thread-filter-option"); options[0].checked = true; options[1].checked = true; options[1].focus(); options[1].dispatch("change"); await waitForUi(); await waitForUi();
    assert.equal(options[1].focused, true, "a checkbox change retains keyboard focus on the live mobile control");
    assert.ok(calls.includes("/api/search?q=signal&perspective=author&allTime=true&threadId=thread_a&threadId=thread_z"), "the server receives sorted repeated ANY selectors");
    revision = "r2"; WebSocketMock.instances[0].emit({ revision }); await waitForUi(); await waitForUi(); await waitForUi();
    assert.deepEqual(elements.threadFilterOptions.querySelectorAll(".thread-filter-option").map((input) => input.dataset.threadId), ["thread_z"], "a revision clears group IDs removed from the catalog");
    assert.ok(calls.includes("/api/search?q=signal&perspective=author&allTime=true&threadId=thread_z"), "a revision re-runs the surviving group selection");
    elements.entityText.value = ""; elements.entitiesForm.dispatch("submit"); await waitForUi(); await waitForUi();
    assert.equal(elements.threadFilter.disabled, false, "clearing text retains the browse selector");
    revision = "r3"; WebSocketMock.instances[0].emit({ revision }); await waitForUi(); await waitForUi(); await waitForUi();
    assert.equal(elements.threadFilter.hidden, true, "v0.3 does not show a synthesized narrative group selector");
  } finally { Object.assign(globalThis, previous); }
});

test("Chronology is a server-backed navigation pane with ordered selectors and an ordinal-only horizon notice", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const calls = [];
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" };
  globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  const catalog = { protocol: "wedl-chronology/v1", revision: "r1", capability: { mode: "chronology-enabled", publicReads: true }, calendars: [{ id: "calendar_z", label: "Zeta", basisId: "basis_z" }, { id: "calendar_a", label: "Alpha", basisId: "basis_a" }], eras: [{ id: "era_z", label: "Z era", basisId: "basis_z" }, { id: "era_a", label: "A era", basisId: "basis_a" }], anchors: [] };
  globalThis.fetch = async (path) => { calls.push(path); const payload = path === "/api/session" ? { token: "test" }
    : path === "/api/status" ? { revision: "r1", recordCount: 1, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
      : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "The Test World" }]
        : path === "/api/chronology" ? catalog : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi(); elements.chronology.click(); await waitForUi(); await waitForUi();
    assert.ok(calls.includes("/api/chronology"));
    assert.match(elements.article.textContent, /Query the published chronology catalogue/);
    assert.equal(elements.article.querySelectorAll(".chronology-calendar")[0].children.map((item) => item.textContent).join(","), "Zeta,Alpha", "calendar order remains server order");
  } finally { Object.assign(globalThis, previous); }
});

test("Chronology keeps source values separate from conversion targets and retains an entered range upper bound", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const requests = [];
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  const catalog = {
    protocol: "wedl-chronology/v1", revision: "r1", capability: { mode: "chronology-enabled", publicReads: true },
    calendars: [{ id: "calendar_shared", label: "Shared", basisId: "basis_shared" }, { id: "calendar_other", label: "Other", basisId: "basis_other" }, { id: "calendar_isolated", label: "Isolated", basisId: "basis_isolated" }],
    eras: [{ id: "era_shared", label: "Shared era", basisId: "basis_shared" }, { id: "era_other", label: "Other era", basisId: "basis_other" }], anchors: [],
  };
  globalThis.fetch = async (path, options = {}) => {
    if (options.method === "POST") {
      const body = JSON.parse(options.body); requests.push({ path, body });
      if (path === "/api/chronology/convert" && body.target.calendarId === "calendar_isolated") return { ok: true, json: async () => ({ outcome: "unavailable", detail: "isolated target has no shared axis", advisories: [] }), text: async () => "" };
      if (path === "/api/chronology/search") return { ok: true, json: async () => ({ outcome: "ok", result: { matches: [] }, advisories: [] }), text: async () => "" };
      if (path === "/api/chronology/story-times") return { ok: true, json: async () => ({ outcome: "ok", result: { mapping: "ambiguous", storyTimes: [{ timeline: "main", tick: "-1", order: "0" }, { timeline: "main", tick: "1", order: "0" }] }, advisories: [] }), text: async () => "" };
      return { ok: true, json: async () => ({ outcome: "ok", result: { formatted: "server formatted", axisDay: "-9" }, advisories: [] }), text: async () => "" };
    }
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision: "r1", recordCount: 1, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "World" }]
          : path === "/api/threads" ? { revision: "r1", groupingAvailable: false, threads: [] }
            : path === "/api/chronology" ? catalog : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi(); elements.chronology.click(); await waitForUi(); await waitForUi();
    const sourceKind = elements.article.querySelector(".chronology-kind"); const sourceCalendar = elements.article.querySelector(".chronology-calendar"); const sourceEra = elements.article.querySelector(".chronology-era"); const year = elements.article.querySelector(".chronology-year"); const upperYear = elements.article.querySelector(".chronology-upper-year"); const targetKind = elements.article.querySelector(".chronology-target-kind"); const targetCalendar = elements.article.querySelector(".chronology-target-calendar"); const targetEra = elements.article.querySelector(".chronology-target-era");
    sourceKind.value = "civil"; sourceCalendar.value = "calendar_shared"; sourceEra.value = "era_shared"; year.value = "-9007199254740993";
    targetKind.value = "era"; targetKind.dispatch("change"); targetEra.value = "era_shared";
    const convert = elements.article.querySelector(".chronology-actions").children.find((item) => item.textContent === "Convert"); convert.click(); await waitForUi(); await waitForUi();
    assert.deepEqual(requests.at(-1).body.target, { eraId: "era_shared" });
    assert.deepEqual(requests.at(-1).body.value, { kind: "civil", calendarId: "calendar_shared", year: "-9007199254740993" });
    assert.equal(sourceCalendar.value, "calendar_shared", "switching conversion target never changes the source calendar"); assert.equal(year.value, "-9007199254740993");
    targetKind.value = "calendar"; targetKind.dispatch("change"); targetCalendar.value = "calendar_other"; convert.click(); await waitForUi(); await waitForUi();
    assert.deepEqual(requests.at(-1).body.target, { calendarId: "calendar_other" }); assert.equal(sourceEra.value, "era_shared");
    sourceKind.value = "era"; sourceEra.value = "era_shared"; targetCalendar.value = "calendar_shared"; convert.click(); await waitForUi(); await waitForUi();
    assert.deepEqual(requests.at(-1).body, { protocol: "wedl-chronology/v1", value: { kind: "era", eraId: "era_shared", year: "-9007199254740993" }, target: { calendarId: "calendar_shared" } });
    targetCalendar.value = "calendar_isolated"; convert.click(); await waitForUi(); await waitForUi(); assert.match(elements.article.textContent, /isolated target has no shared axis/);
    sourceKind.value = "range"; sourceCalendar.value = "calendar_shared"; year.value = "-5"; upperYear.value = "9007199254740993"; elements.article.querySelector(".chronology-predicate").value = "overlaps";
    const actions = elements.article.querySelector(".chronology-actions"); for (const label of ["Format", "Convert", "Search", "Map to StoryTime"]) { actions.children.find((item) => item.textContent === label).click(); await waitForUi(); await waitForUi(); }
    const rangeRequests = requests.slice(-4); for (const request of rangeRequests) assert.equal(request.body.value.upper.year, "9007199254740993", `${request.path} preserves the exact entered range upper endpoint`);
    assert.match(elements.article.textContent, /Ambiguous explicit StoryTimes/);
    upperYear.value = ""; actions.children.find((item) => item.textContent === "Format").click(); await waitForUi(); await waitForUi(); assert.equal(requests.at(-1).body.value.upper, null, "a blank upper year remains an open range");
  } finally { Object.assign(globalThis, previous); }
});

test("Chronology replacement previews are draft-keyed, show exact diffs, and reconcile a stale record without a write", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const previews = []; let applyCalls = 0; let revision = "a".repeat(40); let currentDisplay = "Current server annotation";
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  const annotation = (display) => ({ id: "chronology_one", display, provenance: ["test"], value: { kind: "civil", calendarId: "calendar_shared", year: "0" } });
  const detail = () => ({ id: "char_one", kind: "character", title: "Record one", bodyMarkdown: "A record.", frontmatter: {}, chronologyAnnotations: [annotation(currentDisplay)] });
  globalThis.fetch = (path, options = {}) => {
    if (path === "/api/authoring/preview") return new Promise((resolve) => previews.push(resolve));
    if (path === "/api/authoring/apply") { applyCalls += 1; revision = "b".repeat(40); currentDisplay = "Current server annotation after concurrent edit"; return Promise.resolve({ ok: false, status: 409, text: async () => JSON.stringify({ code: "stale_revision", message: "record changed", details: {} }) }); }
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision, recordCount: 2, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "World" }, { id: "char_one", kind: "character", title: "Record one" }]
          : path === "/api/entities/char_one" ? detail()
            : path === "/api/threads" ? { revision, groupingAvailable: false, threads: [] }
              : path === "/api/chronology" ? { protocol: "wedl-chronology/v1", revision, capability: { mode: "chronology-enabled", publicReads: true }, calendars: [], eras: [], anchors: [] } : {};
    return Promise.resolve({ ok: true, json: async () => payload, text: async () => "" });
  };
  const previewResponse = (token, diff) => ({ ok: true, json: async () => ({ preview: { valid: true, confirmationToken: token, diff }, authorImpact: { summary: "Changed chronology" } }), text: async () => "" });
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi(); elements.entities.querySelector(".entity-select").click(); await waitForUi(); await waitForUi(); elements.article.querySelector(".chronology-edit-button").click(); await waitForUi();
    let form = elements.article.querySelector(".chronology-editor"); let textarea = elements.article.querySelector(".chronology-annotations-json"); let replay = elements.article.querySelector(".chronology-editor").children.find((item) => item.tagName === "input"); let apply = elements.article.querySelector(".chronology-editor").children.find((item) => item.textContent === "Confirm and apply replacement");
    textarea.value += "\n"; textarea.dispatch("input"); form.dispatch("submit"); await waitForUi(); assert.equal(previews.length, 1);
    textarea.value += " "; textarea.dispatch("input"); previews[0](previewResponse("old-token", "old diff")); await waitForUi(); await waitForUi(); assert.equal(apply.disabled, true, "an edited draft cannot inherit an earlier preview");
    form.dispatch("submit"); await waitForUi(); assert.equal(previews.length, 2); previews[1](previewResponse("fresh-token", "--- a/story/record.md\n+++ b/story/record.md\n@@\n-old\n+new\n")); await waitForUi(); await waitForUi();
    const diff = elements.article.querySelector(".chronology-preview-diff"); assert.equal(diff.textContent, "--- a/story/record.md\n+++ b/story/record.md\n@@\n-old\n+new\n"); assert.equal(diff.hidden, false); assert.equal(apply.disabled, false);
    replay.value = "safe-replay"; replay.dispatch("input"); assert.equal(apply.disabled, true, "a replay-key edit invalidates the confirmation token"); assert.equal(diff.hidden, true); assert.equal(diff.textContent, ""); assert.match(elements.article.querySelector(".chronology-editor").textContent, /Draft changed\. Preview the current draft before applying\./); assert.doesNotMatch(elements.article.querySelector(".chronology-editor").textContent, /Preview ready/); apply.click(); await waitForUi(); assert.equal(applyCalls, 0, "apply cannot write after any draft/key edit");
    form.dispatch("submit"); await waitForUi(); assert.equal(previews.length, 3); previews[2](previewResponse("stale-token", "changed source")); await waitForUi(); await waitForUi(); apply = elements.article.querySelector(".chronology-editor").children.find((item) => item.textContent === "Confirm and apply replacement"); assert.equal(apply.disabled, false); const retainedDraft = elements.article.querySelector(".chronology-annotations-json").value;
    apply.click(); await waitForUi(); await waitForUi(); await waitForUi(); await waitForUi();
    assert.equal(applyCalls, 1); assert.match(elements.article.textContent, /Reconcile the current record and your draft/); assert.match(elements.article.querySelector(".chronology-current-annotations").textContent, /Current server annotation after concurrent edit/); textarea = elements.article.querySelector(".chronology-annotations-json"); assert.equal(textarea.value, retainedDraft, "the user draft is retained separately from the reloaded source"); apply = elements.article.querySelector(".chronology-editor").children.find((item) => item.textContent === "Confirm and apply replacement"); assert.equal(apply.disabled, true); apply.click(); await waitForUi(); assert.equal(applyCalls, 1, "a stale reconciliation requires an explicit fresh preview before another write");
    form = elements.article.querySelector(".chronology-editor"); form.dispatch("submit"); await waitForUi(); assert.equal(previews.length, 4); previews[3](previewResponse("no-op-token", "")); await waitForUi(); await waitForUi(); assert.match(elements.article.querySelector(".chronology-editor").textContent, /No source changes/); assert.equal(elements.article.querySelector(".chronology-preview-diff").hidden, true);
  } finally { Object.assign(globalThis, previous); }
});

test("Chronology uses in-world labels for hit and annotation presentation, and an older read cannot repaint the panel", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const formats = []; const revision = "r1";
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  const keeper = { id: "char_opaque_keeper", kind: "character", title: "Keeper of the Archive" };
  const catalog = { protocol: "wedl-chronology/v1", revision, capability: { mode: "chronology-enabled", publicReads: true }, calendars: [{ id: "calendar_opaque_sun", label: "Sun Reckoning", basisId: "shared" }], eras: [{ id: "era_opaque_foundation", label: "Foundation Era", basisId: "shared" }], anchors: [] };
  const detail = { ...keeper, bodyMarkdown: "Keeps the archive.", frontmatter: {}, chronologyAnnotations: [{ id: "chronology_opaque_entry", value: { kind: "era", eraId: "era_opaque_foundation", year: "-5" }, provenance: ["archive ledger"] }] };
  globalThis.fetch = (path, options = {}) => {
    if (options.method === "POST") {
      if (path === "/api/chronology/format") return new Promise((resolve) => formats.push(resolve));
      if (path === "/api/chronology/search") return Promise.resolve({ ok: true, json: async () => ({ outcome: "ok", result: { matches: [{ recordId: keeper.id, annotationId: "chronology_opaque_entry", sourceOrdinal: 0, role: "authored", display: null, provenance: ["archive ledger"], value: { kind: "era", eraId: "era_opaque_foundation", year: "-5" }, precision: "year", relation: "overlaps" }] }, advisories: [{ code: "result-limit", count: 1, message: "The server retained the first result." }] }), text: async () => "" });
      return Promise.resolve({ ok: true, json: async () => ({ outcome: "ok", result: { mapping: "none", storyTimes: [] }, advisories: [] }), text: async () => "" });
    }
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision, recordCount: 2, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "World" }, keeper]
          : path === `/api/entities/${keeper.id}` ? detail
            : path === "/api/threads" ? { revision, groupingAvailable: false, threads: [] }
              : path === "/api/chronology" ? catalog : {};
    return Promise.resolve({ ok: true, json: async () => payload, text: async () => "" });
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi(); elements.entities.querySelectorAll(".entity-select").find((button) => button.dataset.entityId === keeper.id).click(); await waitForUi(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Foundation Era: -5/); assert.doesNotMatch(elements.article.textContent, /era_opaque_foundation|chronology_opaque_entry/);
    const annotationFormat = elements.article.querySelector(".chronology-format-button"); assert.equal(annotationFormat.attributes.get("aria-label"), "Format Foundation Era: -5", "per-annotation formatting has a contextual accessible name");
    elements.chronology.click(); await waitForUi(); await waitForUi();
    const actions = elements.article.querySelector(".chronology-actions"); const controls = { kind: elements.article.querySelector(".chronology-kind"), era: elements.article.querySelector(".chronology-era"), year: elements.article.querySelector(".chronology-year"), predicate: elements.article.querySelector(".chronology-predicate") };
    controls.kind.value = "era"; controls.era.value = "era_opaque_foundation"; controls.predicate.value = "overlaps";
    actions.children.find((item) => item.textContent === "Search").click(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Keeper of the Archive: Foundation Era: -5 Overlaps for Overlaps, Year precision\. Provenance: archive ledger\./);
    assert.doesNotMatch(elements.article.textContent, /char_opaque_keeper|chronology_opaque_entry|era_opaque_foundation/, "primary hit text never exposes opaque implementation IDs");
    const format = actions.children.find((item) => item.textContent === "Format"); format.click(); await waitForUi(); controls.year.value = "2"; controls.year.dispatch("input"); format.click(); await waitForUi();
    formats[1]({ ok: true, json: async () => ({ outcome: "ok", result: { formatted: "new server format" }, advisories: [] }), text: async () => "" }); await waitForUi(); await waitForUi();
    formats[0]({ ok: true, json: async () => ({ outcome: "ok", result: { formatted: "obsolete server format" }, advisories: [] }), text: async () => "" }); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /new server format/); assert.doesNotMatch(elements.article.textContent, /obsolete server format/, "a reversed older response cannot overwrite the current operation");
  } finally { Object.assign(globalThis, previous); }
});

test("Every record offers chronology authoring when empty, with an empty complete replacement and no write for invalid JSON", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const previews = []; let applyCalls = 0; const revision = "a".repeat(40); const empty = { id: "char_empty", kind: "character", title: "An Empty Chronicle" };
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.fetch = (path, options = {}) => {
    if (path === "/api/authoring/preview") { previews.push(JSON.parse(options.body)); return Promise.resolve({ ok: true, json: async () => ({ preview: { valid: true, confirmationToken: "empty-token", diff: "add []" }, authorImpact: { summary: "Empty replacement" } }), text: async () => "" }); }
    if (path === "/api/authoring/apply") { applyCalls += 1; return Promise.resolve({ ok: true, json: async () => ({ newHead: revision }), text: async () => "" }); }
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision, recordCount: 2, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "World" }, empty]
          : path === "/api/entities/char_empty" ? { ...empty, bodyMarkdown: "No chronology yet.", frontmatter: {}, chronologyAnnotations: [] }
            : path === "/api/threads" ? { revision, groupingAvailable: false, threads: [] } : {};
    return Promise.resolve({ ok: true, json: async () => payload, text: async () => "" });
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi(); elements.entities.querySelectorAll(".entity-select").find((button) => button.dataset.entityId === empty.id).click(); await waitForUi(); await waitForUi();
    const add = elements.article.querySelector(".chronology-edit-button"); assert.ok(add); assert.equal(add.textContent, "Add chronology entry"); add.click(); await waitForUi();
    const form = elements.article.querySelector(".chronology-editor"); const textarea = elements.article.querySelector(".chronology-annotations-json"); assert.equal(textarea.value, "[]", "an empty record starts a complete empty replacement draft");
    form.dispatch("submit"); await waitForUi(); await waitForUi(); assert.deepEqual(previews[0].change.records[0].annotations, []); assert.equal(elements.article.querySelector(".chronology-preview-diff").hidden, false);
    textarea.value = "[not valid JSON"; textarea.dispatch("input"); form.dispatch("submit"); await waitForUi(); await waitForUi(); assert.equal(previews.length, 1, "invalid JSON is rejected before preview or any write"); assert.equal(applyCalls, 0); assert.match(elements.article.textContent, /Unexpected token|JSON/);
  } finally { Object.assign(globalThis, previous); }
});

test("membership projection filters browse and timeline candidates, labels matches, and reuses a complete batch", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const calls = []; const revision = "r1";
  const world = { id: "world_story", kind: "world", title: "World" }; const member = { id: "char_member", kind: "character", title: "Member" }; const other = { id: "char_other", kind: "character", title: "Other" }; const event = { id: "event_member", kind: "event", title: "Member event" };
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.fetch = async (path) => {
    calls.push(path);
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision, recordCount: 4, timeModel: { timelineDeclarations: [{ id: "main", label: "Main" }], defaultTimeline: "main" } }
        : path === "/api/entities" ? [world, member, other, event]
          : path === "/api/threads" ? { protocol: "wedl-threads/v1", revision, sourceSchema: "wedl/v0.5", groupingAvailable: true, threads: [{ id: "thread_a", label: "Archive" }] }
            : path === "/api/timeline?timeline=main" ? { revision, timeline: { id: "main" }, points: [{ at: { timeline: "main", tick: "1", order: "0" }, kind: "event", entity: event }], spans: [] }
              : path.startsWith("/api/thread-memberships?") ? { protocol: "wedl-thread-memberships/v1", revision, sourceSchema: "wedl/v0.5", selectedThreadIds: ["thread_a"], records: path.includes("event_member") ? [{ recordId: "char_member", threadIds: ["thread_a"] }, { recordId: "event_member", threadIds: ["thread_a"] }] : [{ recordId: "char_member", threadIds: ["thread_a"] }] }
                : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    const option = elements.threadFilterOptions.querySelector(".thread-filter-option"); option.checked = true; option.dispatch("change"); await waitForUi(); await waitForUi();
    const browsePath = "/api/thread-memberships?recordId=char_member&recordId=char_other&recordId=event_member&recordId=world_story&threadId=thread_a";
    assert.ok(calls.includes(browsePath));
    assert.match(elements.entities.textContent, /Member[\s\S]*Narrative groups: Archive/);
    assert.doesNotMatch(elements.entities.textContent, /Other/);
    const firstCount = calls.filter((path) => path === browsePath).length;
    option.checked = false; option.dispatch("change"); await waitForUi(); option.checked = true; option.dispatch("change"); await waitForUi(); await waitForUi();
    assert.equal(calls.filter((path) => path === browsePath).length, firstCount, "a complete cached batch is reused");
    elements.timeline.click(); await waitForUi(); await waitForUi();
    assert.ok(calls.includes("/api/thread-memberships?recordId=event_member&threadId=thread_a"));
    assert.match(elements.article.textContent, /Member eventNarrative groups: Archive/);
  } finally { Object.assign(globalThis, previous); }
});

test("a failed membership batch closes the selected browse lane and retry replaces it atomically", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const calls = []; let failSecondBatch = true; let membershipCalls = 0; const revision = "r1";
  const world = { id: "world_story", kind: "world", title: "World" }; const records = Array.from({ length: 257 }, (_, index) => ({ id: `char_${String(index).padStart(3, "0")}`, kind: "character", title: `Record ${index}` }));
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.fetch = async (path) => {
    calls.push(path);
    if (path.startsWith("/api/thread-memberships?")) { membershipCalls += 1; if (failSecondBatch && membershipCalls === 2) return { ok: false, status: 503, json: async () => ({}), text: async () => "second batch offline" }; return { ok: true, json: async () => ({ protocol: "wedl-thread-memberships/v1", revision, sourceSchema: "wedl/v0.5", selectedThreadIds: ["thread_a"], records: [{ recordId: "char_000", threadIds: ["thread_a"] }] }), text: async () => "" }; }
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision, recordCount: records.length + 1, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [world, ...records]
          : path === "/api/threads" ? { protocol: "wedl-threads/v1", revision, sourceSchema: "wedl/v0.5", groupingAvailable: true, threads: [{ id: "thread_a", label: "Archive" }] }
            : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    const option = elements.threadFilterOptions.querySelector(".thread-filter-option"); option.checked = true; option.dispatch("change"); await waitForUi(); await waitForUi();
    assert.equal(calls.filter((path) => path.startsWith("/api/thread-memberships?")).length, 2, "257 candidates use two bounded requests");
    assert.doesNotMatch(elements.entities.textContent, /Record 0/, "a partial projection never opens an unfiltered lane");
    assert.match(elements.entitiesStatus.textContent, /selected view is closed/);
    failSecondBatch = false; elements.retrySearch.click(); await waitForUi(); await waitForUi();
    assert.match(elements.entities.textContent, /Record 0/);
    assert.doesNotMatch(elements.entitiesStatus.textContent, /selected view is closed/);
  } finally { Object.assign(globalThis, previous); }
});

test("timeline projection failure exposes a local retry while a search query is active", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const calls = []; let failed = false; const revision = "r1"; const event = { id: "event_member", kind: "event", title: "Member event" }; const searchOnly = { id: "event_search", kind: "event", title: "Search-only event" };
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: true }) };
  globalThis.fetch = async (path) => { calls.push(path);
    if (path.startsWith("/api/thread-memberships?")) { if (!failed) { failed = true; return { ok: false, status: 503, json: async () => ({}), text: async () => "offline" }; } const record = path.includes("event_search") ? searchOnly : event; return { ok: true, json: async () => ({ protocol: "wedl-thread-memberships/v1", revision, sourceSchema: "wedl/v0.5", selectedThreadIds: ["thread_a"], records: [{ recordId: record.id, threadIds: ["thread_a"] }] }), text: async () => "" }; }
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision, recordCount: 2, timeModel: { timelineDeclarations: [{ id: "main", label: "Main" }], defaultTimeline: "main" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "World" }, event, searchOnly]
          : path === "/api/threads" ? { protocol: "wedl-threads/v1", revision, sourceSchema: "wedl/v0.5", groupingAvailable: true, threads: [{ id: "thread_a", label: "Archive" }] }
            : path === "/api/timeline?timeline=main" ? { revision, timeline: { id: "main" }, points: [{ at: { timeline: "main", tick: "1", order: "0" }, kind: "event", entity: event }], spans: [] }
              : path.startsWith("/api/search") ? { protocol: "wedl-search/v5", revision, results: [{ entityId: searchOnly.id, kind: "event", title: searchOnly.title }] }
                : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi(); elements.entityText.value = "signal"; elements.entitiesForm.dispatch("submit"); await waitForUi(); await waitForUi(); elements.timeline.click(); await waitForUi(); await waitForUi();
    assert.match(elements.entities.textContent, /Search-only event/); const option = elements.threadFilterOptions.querySelector(".thread-filter-option"); option.checked = true; option.dispatch("change"); await waitForUi(); await waitForUi();
    const retry = elements.article.querySelector(".thread-membership-retry");
    assert.ok(retry, "timeline exposes a reachable local recovery control"); assert.match(elements.article.textContent, /selected view is closed/); assert.equal(elements.articleStatus.textContent.includes("selected view is closed"), true); assert.ok(calls.includes("/api/thread-memberships?recordId=event_member&threadId=thread_a")); assert.equal(calls.some((path) => path.includes("recordId=event_search")), false, "timeline selection ignores distinct search candidates");
    retry.focus(); assert.equal(retry.focused, true); retry.click(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Member eventNarrative groups: Archive/); assert.match(elements.entities.textContent, /Search-only event/, "selector refresh did not repaint the hidden index from search candidates"); assert.equal(elements.article.querySelector(".thread-membership-retry"), null);
    elements.back.click(); await waitForUi(); await waitForUi(); await waitForUi();
    assert.ok(calls.includes("/api/search?q=signal&perspective=author&allTime=true&threadId=thread_a"), "returning to the index refreshes the active server-filtered search"); assert.ok(calls.includes("/api/thread-memberships?recordId=event_search&threadId=thread_a"), "the refreshed search receives its own membership projection"); assert.match(elements.entities.textContent, /Search-only event[\s\S]*Narrative groups: Archive/); assert.doesNotMatch(elements.entities.textContent, /Member event/);
  } finally { Object.assign(globalThis, previous); }
});

test("overlapping membership requests cannot overwrite the newest selected lane", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const deferred = []; const revision = "r1"; const old = { id: "char_old", kind: "character", title: "Old" }; const fresh = { id: "char_fresh", kind: "character", title: "Fresh" };
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.fetch = (path) => {
    if (path.startsWith("/api/thread-memberships?")) { const call = deferred.length; if (call < 2) return new Promise((resolve, reject) => deferred.push({ resolve, reject })); const payload = { protocol: "wedl-thread-memberships/v1", revision, sourceSchema: "wedl/v0.5", selectedThreadIds: ["thread_b"], records: [{ recordId: fresh.id, threadIds: ["thread_b"] }] }; return Promise.resolve({ ok: true, json: async () => payload, text: async () => "" }); }
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision, recordCount: 3, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "World" }, old, fresh]
          : path === "/api/threads" ? { protocol: "wedl-threads/v1", revision, sourceSchema: "wedl/v0.5", groupingAvailable: true, threads: [{ id: "thread_a", label: "A" }, { id: "thread_b", label: "B" }] }
            : {};
    return Promise.resolve({ ok: true, json: async () => payload, text: async () => "" });
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi(); const [a, b] = elements.threadFilterOptions.querySelectorAll(".thread-filter-option");
    a.checked = true; a.dispatch("change"); await waitForUi(); b.checked = true; b.dispatch("change"); await waitForUi(); a.checked = false; a.dispatch("change"); await waitForUi(); await waitForUi();
    deferred[0].reject(new Error("stale failure")); deferred[1].resolve({ ok: true, json: async () => ({ protocol: "wedl-thread-memberships/v1", revision, sourceSchema: "wedl/v0.5", selectedThreadIds: ["thread_a", "thread_b"], records: [{ recordId: old.id, threadIds: ["thread_a"] }] }), text: async () => "" }); await waitForUi(); await waitForUi();
    assert.match(elements.entities.textContent, /Fresh/); assert.doesNotMatch(elements.entities.textContent, /Old/); assert.doesNotMatch(elements.entitiesStatus.textContent, /selected view is closed/);
  } finally { Object.assign(globalThis, previous); }
});

test("a WebSocket revision invalidates and reloads the membership projection before repainting", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window, WebSocket: globalThis.WebSocket };
  const { document, elements } = makeDom(); let revision = "r1"; class WebSocketMock { static instances = []; constructor() { this.events = new Map(); WebSocketMock.instances.push(this); } addEventListener(name, listener) { this.events.set(name, listener); } emit(value) { this.events.get("message")?.({ data: JSON.stringify(value) }); } }
  const old = { id: "char_old", kind: "character", title: "Old" }; const fresh = { id: "char_fresh", kind: "character", title: "Fresh" };
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) }; globalThis.WebSocket = WebSocketMock;
  globalThis.fetch = async (path) => {
    const current = revision === "r1" ? old : fresh;
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision, recordCount: 2, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "World" }, current]
          : path === "/api/threads" ? { protocol: "wedl-threads/v1", revision, sourceSchema: "wedl/v0.5", groupingAvailable: true, threads: [{ id: "thread_a", label: "Archive" }] }
            : path.startsWith("/api/thread-memberships?") ? { protocol: "wedl-thread-memberships/v1", revision, sourceSchema: "wedl/v0.5", selectedThreadIds: ["thread_a"], records: [{ recordId: current.id, threadIds: ["thread_a"] }] }
              : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi(); const option = elements.threadFilterOptions.querySelector(".thread-filter-option"); option.checked = true; option.dispatch("change"); await waitForUi(); await waitForUi(); assert.match(elements.entities.textContent, /Old/);
    revision = "r2"; WebSocketMock.instances[0].emit({ revision }); await waitForUi(); await waitForUi(); await waitForUi();
    assert.match(elements.entities.textContent, /Fresh[\s\S]*Narrative groups: Archive/); assert.doesNotMatch(elements.entities.textContent, /Old/);
  } finally { Object.assign(globalThis, previous); }
});

test("returning from timeline restores distinct selected browse and unchanged-search lanes", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const revision = "r1"; const browse = { id: "char_browse", kind: "character", title: "Browse match" }; const searched = { id: "char_search", kind: "character", title: "Search match" }; const timeline = { id: "event_timeline", kind: "event", title: "Timeline match" };
  globalThis.document = document; globalThis.location = { pathname: "/", protocol: "http:", host: "wedl.test" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.fetch = async (path) => {
    let record = browse;
    if (path.includes("recordId=event_timeline")) record = timeline; else if (path.startsWith("/api/thread-memberships?") && !path.includes("char_browse") && path.includes("char_search")) record = searched;
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision, recordCount: 4, timeModel: { timelineDeclarations: [{ id: "main", label: "Main" }], defaultTimeline: "main" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "World" }, browse, searched]
          : path === "/api/threads" ? { protocol: "wedl-threads/v1", revision, sourceSchema: "wedl/v0.5", groupingAvailable: true, threads: [{ id: "thread_a", label: "Archive" }] }
            : path === "/api/timeline?timeline=main" ? { revision, timeline: { id: "main" }, points: [{ at: { timeline: "main", tick: "1", order: "0" }, kind: "event", entity: timeline }], spans: [] }
              : path.startsWith("/api/search") ? { protocol: "wedl-search/v5", revision, results: [{ entityId: searched.id, kind: searched.kind, title: searched.title }] }
                : path.startsWith("/api/thread-memberships?") ? { protocol: "wedl-thread-memberships/v1", revision, sourceSchema: "wedl/v0.5", selectedThreadIds: ["thread_a"], records: [{ recordId: record.id, threadIds: ["thread_a"] }] }
                  : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi(); const option = elements.threadFilterOptions.querySelector(".thread-filter-option");
    option.checked = true; option.dispatch("change"); await waitForUi(); await waitForUi(); assert.match(elements.entities.textContent, /Browse match/);
    elements.timeline.click(); await waitForUi(); await waitForUi(); assert.match(elements.article.textContent, /Timeline match/); elements.back.click(); await waitForUi(); await waitForUi(); assert.match(elements.entities.textContent, /Browse match/); assert.doesNotMatch(elements.entities.textContent, /Timeline match/);
    option.checked = false; option.dispatch("change"); await waitForUi(); elements.entityText.value = "signal"; elements.entitiesForm.dispatch("submit"); await waitForUi(); await waitForUi(); const searchOption = elements.threadFilterOptions.querySelector(".thread-filter-option"); searchOption.checked = true; searchOption.dispatch("change"); await waitForUi(); await waitForUi(); assert.match(elements.entities.textContent, /Search match/);
    elements.timeline.click(); await waitForUi(); await waitForUi(); elements.back.click(); await waitForUi(); await waitForUi(); assert.match(elements.entities.textContent, /Search match/); assert.doesNotMatch(elements.entities.textContent, /Timeline match/);
  } finally { Object.assign(globalThis, previous); }
});

test("a failed narrative group catalog keeps the existing unfiltered author search available", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window };
  const { document, elements } = makeDom(); const calls = [];
  globalThis.document = document; globalThis.location = { pathname: "/" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.fetch = async (path) => {
    calls.push(path);
    if (path === "/api/threads") return { ok: false, status: 503, json: async () => ({}), text: async () => "catalog offline" };
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision: "r1", recordCount: 1, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/entities" ? [{ id: "world_story", kind: "world", title: "The Test World" }]
          : path.startsWith("/api/search") ? { protocol: "wedl-search/v5", revision: "r1", results: [] }
            : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    assert.equal(elements.threadFilter.hidden, true);
    assert.equal(elements.threadFilterStatus.hidden, false);
    assert.match(elements.threadFilterStatus.textContent, /unavailable/);
    elements.entityText.value = "signal"; elements.entitiesForm.dispatch("submit"); await waitForUi(); await waitForUi();
    assert.ok(calls.includes("/api/search?q=signal&perspective=author&allTime=true"));
  } finally { Object.assign(globalThis, previous); }
});

test("successful whereabouts load replaces its placeholder with visible independent locations and journeys", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window, WebSocket: globalThis.WebSocket };
  const { document, elements } = makeDom(); const calls = []; let revision = "r1";
  class WebSocketMock { static instances = []; constructor() { this.events = new Map(); WebSocketMock.instances.push(this); } addEventListener(name, listener) { this.events.set(name, listener); } emit(value) { this.events.get("message")?.({ data: JSON.stringify(value) }); } }
  globalThis.document = document;
  globalThis.location = { pathname: "/" };
  globalThis.history = { state: null, pushState() {}, replaceState() {} };
  globalThis.requestAnimationFrame = (callback) => callback();
  globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) }; globalThis.WebSocket = WebSocketMock;
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
      : path === "/api/status" ? { revision, recordCount: entities.length, timeModel: { timelineDeclarations: [], defaultTimeline: "" } }
        : path === "/api/threads" ? { revision, groupingAvailable: false, threads: [] }
          : path === "/api/entities" ? entities
          : path === "/api/entities/char_rhea" ? { ...entities[1], bodyMarkdown: "A veteran.", frontmatter: { role: "shield-veteran" } }
            : path === "/api/entities/event_move" ? { ...entities[3], bodyMarkdown: "A move is recorded.", frontmatter: {} }
          : path === "/api/whereabouts" ? {
            revision, effectiveTime: { timeline: "main", tick: "5", order: "0" }, importancePolicy: { calculated: true }, activeScenes: [{ scene: entities[4], location: entities[2], characters: [entities[1]] }],
            characters: [
              { character: entities[1], role: revision === "r1" ? "shield-veteran" : "trail-guide", importance: { score: 3, raw: { scenes: 1, pointOfViewScenes: 0, events: 1, relationshipNeighbors: 0 }, contributions: { scenes: 2, pointOfViewScenes: 0, events: 1, relationshipNeighbors: 0 }, explanation: "Calculated evidence." }, presence: "offstage", location: entities[2], lastKnownLocation: null, activeScene: null, journey: [{ kind: "initial", at: null, from: null, to: entities[2], event: null }, { kind: "move", at: { timeline: "main", tick: "5", order: "0" }, from: null, to: entities[2], event: entities[3] }] },
              { character: { id: "char_lost", kind: "character", title: "Lost Scout" }, role: null, importance: { score: 10, raw: { scenes: 0, pointOfViewScenes: 0, events: 0, relationshipNeighbors: 0 }, contributions: { scenes: 0, pointOfViewScenes: 0, events: 0, relationshipNeighbors: 0 }, explanation: "Calculated evidence." }, presence: "unlocated", location: null, lastKnownLocation: entities[2], activeScene: null, journey: [{ kind: "clear", at: { timeline: "main", tick: "4", order: "0" }, from: entities[2], to: null, event: entities[3] }] },
              { character: { id: "char_ada", kind: "character", title: "Ada" }, role: null, importance: { score: 0, raw: {}, contributions: {}, explanation: "Calculated evidence." }, presence: "unlocated", location: null, lastKnownLocation: null, activeScene: null, journey: [] },
              { character: { id: "char_zed", kind: "character", title: "Zed" }, role: null, importance: { score: 0, raw: {}, contributions: {}, explanation: "Calculated evidence." }, presence: "unlocated", location: null, lastKnownLocation: null, activeScene: null, journey: [] },
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
    assert.doesNotMatch(elements.article.textContent, /Calculated prominence:|Score:|weighted contribution/);
    assert.match(elements.article.textContent, /lower calculated prominence/);
    assert.match(elements.article.textContent, /Order characters by/);
    const people = elements.article.querySelectorAll(".whereabouts-person");
    assert.match(people[0].textContent, /Lost Scout/, "people are score-sorted from the bulk projection");
    assert.match(people[2].textContent, /Ada/); assert.match(people[3].textContent, /Zed/, "zero-score ties have deterministic title ordering");
    assert.doesNotMatch(elements.article.textContent, /Scene appearances: 1/);
    assert.match(elements.article.textContent, /Shield Veteran/);
    assert.doesNotMatch(elements.article.textContent, /Opening whereabouts|Gathering the recorded locations/);
    assert.doesNotMatch(elements.article.textContent, /Whereabouts unavailable/);
    assert.equal(calls.filter((path) => path === "/api/whereabouts").length, 1);
    assert.equal(calls.filter((path) => path === "/api/entities/char_rhea").length, 1, "opening the prior detail did not trigger a whereabouts role read");
    revision = "r2"; WebSocketMock.instances[0].emit({ compile: { revision: "r2" } }); await waitForUi(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Trail Guide/, "a revision event replaces the old bulk role rather than retaining a stale cache");
    assert.equal(calls.filter((path) => path === "/api/whereabouts").length, 2, "the revision invalidated the horizon projection cache");
    const eventLink = elements.article.querySelectorAll(".lore-link").find((button) => button.textContent === "Rhea reaches the road");
    eventLink.click(); await waitForUi(); await waitForUi();
    elements.back.click(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Whereabouts/, "a lore link opened from whereabouts returns there, not to the timeline");
  } finally {
    Object.assign(globalThis, previous);
  }
});

test("a read-detected revision atomically clears a removed timeline, horizon, and selected entity", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window, WebSocket: globalThis.WebSocket };
  const { document, elements } = makeDom(); const calls = []; let phase = "r1";
  class WebSocketMock { static instances = []; constructor() { this.events = new Map(); WebSocketMock.instances.push(this); } addEventListener(name, listener) { this.events.set(name, listener); } emit(value) { this.events.get("message")?.({ data: JSON.stringify(value) }); } }
  globalThis.document = document; globalThis.location = { pathname: "/" }; globalThis.history = { state: null, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) }; globalThis.WebSocket = WebSocketMock;
  const world = { id: "world_story", kind: "world", title: "The Test World" }; const rhea = { id: "char_rhea", kind: "character", title: "Rhea" };
  const status = () => phase === "r1" ? { revision: "r1", recordCount: 2, timeModel: { timelineDeclarations: [{ id: "main", label: "Main" }], defaultTimeline: "main" } } : { revision: "r3", recordCount: 1, timeModel: { timelineDeclarations: [], defaultTimeline: "" } };
  globalThis.fetch = async (path) => { calls.push(path); const payload = path === "/api/session" ? { token: "test" } : path === "/api/status" ? status() : path === "/api/threads" ? { revision: phase === "r1" ? "r1" : "r3", groupingAvailable: false, threads: [] } : path === "/api/entities" ? (phase === "r1" ? [world, rhea] : [world]) : path === "/api/timeline?timeline=main" ? { revision: "r1", timeline: { id: "main" }, points: [{ at: { timeline: "main", tick: "5", order: "0" }, entity: rhea, kind: "event" }], spans: [] } : path === "/api/entities/char_rhea" ? { ...rhea, bodyMarkdown: "A veteran.", frontmatter: {} } : {}; return { ok: true, json: async () => payload, text: async () => "" }; };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    elements.horizon.value = "main\u00005\u00000"; elements.horizon.dispatch("change"); await waitForUi();
    elements.entities.querySelectorAll(".entity-select").find((button) => button.dataset.entityId === "char_rhea").click(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Rhea/);
    phase = "r3"; elements.entities.querySelectorAll(".entity-select").find((button) => button.dataset.entityId === "char_rhea").click(); await waitForUi(); await waitForUi(); await waitForUi(); await waitForUi();
    assert.match(elements.article.textContent, /Lore changed/);
    assert.doesNotMatch(elements.article.textContent, /A veteran/);
    assert.equal(elements.horizon.disabled, true, "the removed timeline clears its horizon control");
    assert.equal(calls.filter((path) => path === "/api/timeline?timeline=main").length, 1, "a removed timeline is never requested after the revision race");
  } finally { Object.assign(globalThis, previous); }
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
    assert.equal(calls.filter((path) => path.startsWith("/api/whereabouts")).length, 0, "a character-free timeline does not request prominence data");
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

test("timeline participant prominence loads per horizon and refreshes after a revision", async () => {
  const previous = { document: globalThis.document, fetch: globalThis.fetch, history: globalThis.history, location: globalThis.location, requestAnimationFrame: globalThis.requestAnimationFrame, window: globalThis.window, WebSocket: globalThis.WebSocket };
  const { document, elements } = makeDom(); const calls = []; let revision = "r1";
  class WebSocketMock { static instances = []; constructor() { this.events = new Map(); WebSocketMock.instances.push(this); } addEventListener(name, listener) { this.events.set(name, listener); } emit(value) { this.events.get("message")?.({ data: JSON.stringify(value) }); } }
  globalThis.document = document; globalThis.location = { pathname: "/" }; globalThis.history = { state: { view: "index", kind: "event", searchKind: "event" }, pushState() {}, replaceState() {} }; globalThis.requestAnimationFrame = (callback) => callback(); globalThis.window = { addEventListener() {}, matchMedia: () => ({ matches: false }) }; globalThis.WebSocket = WebSocketMock;
  const horizon = { timeline: "main", tick: "5", order: "0" }; const world = { id: "world_story", kind: "world", title: "The Test World" }; const event = { id: "event_signal", kind: "event", title: "A signal answers" }; const scout = { id: "char_scout", kind: "character", title: "Scout" };
  globalThis.fetch = async (path) => {
    calls.push(path);
    const payload = path === "/api/session" ? { token: "test" }
      : path === "/api/status" ? { revision, recordCount: 2, timeModel: { timelineDeclarations: [{ id: "main", label: "Main" }], defaultTimeline: "main" } }
        : path === "/api/threads" ? { revision, groupingAvailable: false, threads: [] }
          : path === "/api/entities" ? [world, event, scout]
          : path === "/api/timeline?timeline=main" ? { revision, timeline: { id: "main" }, points: [{ at: horizon, kind: "event", entity: event, participants: [scout] }], spans: [] }
            : path.startsWith("/api/whereabouts") ? { revision, effectiveTime: horizon, activeScenes: [], characters: [{ character: scout, importance: { score: revision === "r1" ? 42 : 84 } }] }
              : {};
    return { ok: true, json: async () => payload, text: async () => "" };
  };
  try {
    await loadBrowserModule(); await waitForUi(); await waitForUi();
    assert.equal(calls.filter((path) => path.startsWith("/api/whereabouts")).length, 0, "the character-free index does not request prominence data");
    elements.timeline.click(); await waitForUi(); await waitForUi();
    assert.equal(calls.filter((path) => path === "/api/whereabouts").length, 1, "a timeline participant requests prominence data even outside the index");
    assert.ok(elements.article.querySelectorAll(".character-prominence").some((item) => /Scout/.test(item.textContent)), "the timeline participant receives the prominence presentation");
    elements.horizon.value = "main\u00005\u00000"; elements.horizon.dispatch("change"); await waitForUi(); await waitForUi();
    const horizonPath = "/api/whereabouts?timeline=main&tick=5&order=0";
    assert.equal(calls.filter((path) => path === horizonPath).length, 1, "a selected horizon has its own prominence projection");
    revision = "r2"; WebSocketMock.instances[0].emit({ revision }); await waitForUi(); await waitForUi(); await waitForUi();
    assert.equal(calls.filter((path) => path === horizonPath).length, 2, "a revision invalidates the horizon prominence cache for timeline participants");
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
