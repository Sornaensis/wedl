import assert from "node:assert/strict";
import test from "node:test";
import { createSpatialApi } from "../src/wedl/static/spatial_api.mjs";
import { mountSpatialExplorer } from "../src/wedl/static/spatial_app.mjs";

// Budgets apply to boot plus one hierarchy, search, route, viewport, layer,
// and path action. Each response may contain at most one 50-record page.
const MAX_REQUEST_BYTES = 16_000;
const MAX_RESPONSE_BYTES = 80_000;
const MAX_TOTAL_RESPONSE_BYTES = 250_000;
const MAX_MOUNTED_ELEMENTS = 1_000;
const revision = "a".repeat(40);
const capabilities = ["spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"];

class Element {
  constructor(tag = "div") {
    this.tagName = tag; this.children = []; this.handlers = new Map(); this.value = ""; this.hidden = false; this.disabled = false;
    this.classList = { toggle() {} }; this._text = "";
  }
  set textContent(value) { this.children = []; this._text = String(value); }
  get textContent() { return this._text + this.children.map((child) => child.textContent).join(""); }
  append(...children) { this.children.push(...children); if (this.tagName === "select" && this.children.length && !this.value) this.value = this.children[0].value; }
  replaceChildren(...children) { this.children = []; this._text = ""; if (this.tagName === "select") this.value = ""; this.append(...children); }
  addEventListener(name, listener) { this.handlers.set(name, listener); }
  dispatch(name) { this.handlers.get(name)?.({ preventDefault() {} }); }
  click() { this.dispatch("click"); }
  focus() { this.focused = true; }
}

const ids = `spatial-status refresh-catalog place-search place-query place-roots place-back place-context place-list place-more selected-place route-outgoing route-incoming route-list route-more path-form path-target path-metric path-submit path-result map-select map-more map-native viewport-form min-x min-y max-x max-y min-z max-z map-features viewport-more layer-form story-timeline story-tick story-order layer-audience layer-perspective layer-list layer-more`;
function makeDocument() {
  const nodes = new Map(ids.split(" ").map((id) => [id, new Element(id.includes("form") ? "form" : id.endsWith("select") ? "select" : "div")]));
  nodes.get("path-metric").value = "routeDistance";
  nodes.get("layer-audience").value = "author";
  nodes.get("layer-perspective").value = "author";
  return { nodes, document: { getElementById: (id) => nodes.get(id), createElement: (tag) => new Element(tag), querySelectorAll: () => [] } };
}
const flush = async () => { for (let index = 0; index < 5; index += 1) await new Promise((resolve) => setTimeout(resolve, 0)); };
function countMounted(nodes) {
  const count = (node) => 1 + node.children.reduce((sum, child) => sum + count(child), 0);
  return [...nodes.values()].reduce((sum, node) => sum + count(node), 0);
}

function fixture(size) {
  const places = Array.from({ length: size }, (_, i) => ({ id: `place:${i}`, label: `Place ${i}`, parentId: null,
    mapId: "map:local", geometryAvailable: true, basis: "authored-location" }));
  const routes = Array.from({ length: size }, (_, i) => ({ kind: "route", id: `route:${i}`, label: `Route ${i}`,
    fromLocationId: "place:0", toLocationId: `place:${i}`, authoredDirection: "one-way", reverseOfAuthored: false,
    modes: [], availability: "open", uncertainty: "known", routeDistance: { value: 1, unit: "pace" },
    travelCost: null, duration: null }));
  const layers = Array.from({ length: size }, (_, i) => ({ overlayId: `overlay:${i}`, label: `Overlay ${i}`,
    locationId: `place:${i}`, geometry: { kind: "point", coordinates: [i, i] }, basis: "authorized-authored-overlay-membership" }));
  const features = places.map((place, i) => ({ id: place.id, label: place.label, mapId: "map:local",
    geometry: { kind: "point", coordinates: [i, i] } }));
  return { places, routes, layers, features };
}

async function scenario(size) {
  const data = fixture(size), calls = [], { document, nodes } = makeDocument();
  let peak = countMounted(nodes);
  const observe = () => { peak = Math.max(peak, countMounted(nodes)); };
  const fetcher = async (path, options) => {
    assert.ok(path.startsWith("/"), "same-origin paths only");
    assert.doesNotMatch(path, /\/api\/entities|https?:\/\//);
    const request = options.body ? JSON.parse(options.body) : null;
    const operation = path.split("?")[0].split("/").at(-1);
    const next = (values, key) => ({ [key]: values.slice(0, 50), nextCursor: values.length > 50 ? "opaque-next" : null });
    let payload;
    if (operation === "session") payload = { token: "test" };
    else if (operation === "catalog") payload = { protocol: "wedl-spatial-explorer/v1", operation, revision, capabilities, state: "ok",
      result: { spatialAvailable: true, maps: [{ id: "map:local", label: "Local map", crs: "local:grid", axes: ["x", "y"], unit: "pace", bounds: { min: [0, 0], max: [100, 100] } }], nextCursor: null } };
    else if (operation === "path") payload = { protocol: "wedl-spatial/v1", operation, revision, capabilities, state: "ok",
      result: { path: { ids: ["place:0", "place:1"], metric: { metric: "routeDistance", computedTotal: 1, unit: "pace", unknown: false }, partial: false } } };
    else {
      assert.equal(request?.limit, 50);
      const records = operation === "places" ? data.places : operation === "routes" ? data.routes : operation === "viewport" ? data.features : data.layers;
      payload = { protocol: "wedl-spatial-explorer/v1", operation, revision, capabilities, state: "ok",
        result: { ...next(records, operation === "places" ? "places" : operation === "routes" ? "routes" : operation === "viewport" ? "features" : "layers"),
          crs: "local:grid", unit: "pace", mapId: "map:local" } };
    }
    const bytes = JSON.stringify(payload).length;
    calls.push({ path, request, requestBytes: (options.body || "").length + path.length, responseBytes: bytes });
    assert.ok(bytes < MAX_RESPONSE_BYTES, `single response budget: ${bytes}`);
    return { ok: true, status: 200, json: async () => payload };
  };
  const handoff = new Map();
  const browser = { sessionStorage: { setItem(key, value) { handoff.set(key, value); } } };
  mountSpatialExplorer(document, createSpatialApi(fetcher), browser); await flush(); observe();
  assert.deepEqual(calls.map((call) => call.path.split("?")[0]), ["/api/session", "/api/spatial/explorer/catalog", "/api/spatial/explorer/places"]);
  assert.equal(nodes.get("place-list").children.length, Math.min(size, 50));
  assert.equal(nodes.get("place-more").hidden, false);
  nodes.get("place-more").click(); await flush(); observe();
  assert.equal(nodes.get("place-list").children.length, 50, "manual pagination replaces its mounted page");
  const lore = nodes.get("place-list").children[0].children.find((child) => child.textContent === "Read lore");
  lore.click(); assert.equal(lore.href, "/"); assert.equal(handoff.get("wedl.spatial.lore.once"), "place:0");
  nodes.get("place-list").children[0].children.find((child) => child.textContent === "Children").click(); await flush(); observe();
  nodes.get("place-query").value = "Place"; nodes.get("place-search").dispatch("submit"); await flush(); observe();
  nodes.get("place-list").children[0].children.find((child) => child.textContent === "Inspect").click(); await flush(); observe();
  nodes.get("route-incoming").click(); await flush(); observe();
  assert.match(nodes.get("spatial-status").textContent, /incoming/);
  nodes.get("viewport-form").dispatch("submit"); await flush(); observe();
  nodes.get("story-timeline").value = "main"; nodes.get("story-tick").value = "0"; nodes.get("story-order").value = "0";
  nodes.get("layer-form").dispatch("submit"); await flush(); observe();
  nodes.get("path-target").value = "place:1"; nodes.get("path-form").dispatch("submit"); await flush(); observe();
  assert.match(nodes.get("path-result").textContent, /1 pace/);
  assert.match(nodes.get("map-native").textContent, /local:grid.*pace/);
  assert.ok(calls.some((call) => call.path.endsWith("/layers") && call.request.asOf.timeline === "main"));
  assert.ok(calls.some((call) => call.path.endsWith("/routes") && call.request.direction === "incoming"));
  assert.ok(peak < MAX_MOUNTED_ELEMENTS, `DOM budget: ${peak}`);
  const requestBytes = calls.reduce((sum, call) => sum + call.requestBytes, 0);
  const responseBytes = calls.reduce((sum, call) => sum + call.responseBytes, 0);
  assert.ok(requestBytes < MAX_REQUEST_BYTES, `request budget: ${requestBytes}`);
  assert.ok(responseBytes < MAX_TOTAL_RESPONSE_BYTES, `total response budget: ${responseBytes}`);
  return { requestBytes, responseBytes, peak, calls: calls.length };
}

test("small and 100k place/route/overlay worlds keep request and mounted DOM budgets flat", async () => {
  const small = await scenario(60), large = await scenario(100_000);
  assert.equal(large.calls, small.calls);
  assert.ok(large.requestBytes <= small.requestBytes + 1_000);
  assert.ok(large.responseBytes <= small.responseBytes + 4_000);
  assert.ok(large.peak <= small.peak + 100);
  console.info(`Spatial UI budgets: small=${JSON.stringify(small)}; 100k=${JSON.stringify(large)}`);
});

test("coordinate-free places stay browsable and superseded hostile search text cannot repaint", async () => {
  const { document, nodes } = makeDocument();
  const hostile = "<img src=x onerror=alert(1)>";
  let resolveSlow;
  const api = {
    revision, capabilities: ["spatial-core-v1"], invalidate() {}, async session() {},
    async catalog() { return { spatialAvailable: true, maps: [], nextCursor: null }; },
    async explorer(operation, fields) {
      assert.equal(operation, "places");
      if (fields.query === "slow") return new Promise((resolve) => { resolveSlow = resolve; });
      const label = fields.query === "fast" ? "Fast result" : hostile;
      return { places: [{ id: "place:safe", label, parentId: null, mapId: null, geometryAvailable: false }], nextCursor: null };
    },
  };
  mountSpatialExplorer(document, api); await flush();
  assert.match(nodes.get("place-list").textContent, /<img src=x onerror=alert\(1\)>/);
  assert.match(nodes.get("map-native").textContent, /No authored map/);
  nodes.get("place-list").children[0].children.find((child) => child.textContent === "Inspect").click(); await flush();
  assert.match(nodes.get("selected-place").textContent, /no coordinates/);
  assert.equal(nodes.get("route-outgoing").disabled, true);
  nodes.get("place-query").value = "slow"; nodes.get("place-search").dispatch("submit"); await flush();
  nodes.get("place-query").value = "fast"; nodes.get("place-search").dispatch("submit"); await flush();
  resolveSlow({ places: [{ id: "place:stale", label: "Stale", geometryAvailable: false }], nextCursor: null }); await flush();
  assert.match(nodes.get("place-list").textContent, /Fast result/);
  assert.doesNotMatch(nodes.get("place-list").textContent, /Stale/);
});

test("incoming portal inspects authored source and slow exact-ID selection cannot replace a newer direct choice", async () => {
  const { document, nodes } = makeDocument();
  const cards = ["place:target", "place:new"].map((id) => ({ id, label: id, parentId: null, mapId: null, geometryAvailable: false }));
  let resolveSlow; const selections = [];
  const api = {
    revision, capabilities: ["spatial-core-v1", "route-v1"], invalidate() {}, async session() {},
    async catalog() { return { spatialAvailable: true, maps: [], nextCursor: null }; },
    async explorer(operation, fields) {
      if (operation === "places" && fields.mode === "roots") return { places: cards, nextCursor: null };
      if (operation === "places" && fields.mode === "select") {
        selections.push(fields.ids[0]);
        return new Promise((resolve) => { resolveSlow = resolve; });
      }
      if (operation === "routes" && fields.direction === "incoming") return { routes: [{ kind: "portal", id: "portal:source-target",
        label: "Source gate", fromLocationId: "place:source", target: { kind: "location", locationId: "place:target" }, modes: [] }], nextCursor: null };
      if (operation === "routes") return { routes: [], nextCursor: null };
      throw new Error(`Unexpected ${operation}`);
    },
  };
  mountSpatialExplorer(document, api); await flush();
  nodes.get("place-list").children[0].children.find((child) => child.textContent === "Inspect").click(); await flush();
  nodes.get("route-incoming").click(); await flush();
  const incomingPortal = nodes.get("route-list").children[0];
  assert.ok(incomingPortal.children.some((child) => child.textContent === "Inspect source"));
  assert.ok(!incomingPortal.children.some((child) => child.textContent === "Inspect destination"));
  incomingPortal.children.find((child) => child.textContent === "Inspect source").click(); await flush();
  assert.deepEqual(selections, ["place:source"], "incoming portal opens its authored source, not its selected target");
  nodes.get("place-list").children[1].children.find((child) => child.textContent === "Inspect").click(); await flush();
  resolveSlow({ places: [{ id: "place:source", label: "Stale source", geometryAvailable: false }], nextCursor: null }); await flush();
  assert.match(nodes.get("selected-place").textContent, /place:new/);
  assert.doesNotMatch(nodes.get("selected-place").textContent, /Stale source/);
});
