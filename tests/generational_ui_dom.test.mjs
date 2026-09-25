import assert from "node:assert/strict";
import test from "node:test";
import { createGenerationalApi } from "../src/wedl/static/generational_api.mjs";
import { mountGenerationalExplorer } from "../src/wedl/static/generational_app.mjs";

const revision = "a".repeat(40);
const capabilities = ["generational-core-v1"];
const ids = `gen-status horizon-form gen-timeline gen-tick gen-order gen-refresh horizon-label discover-form gen-kind gen-query discover-status discover-results discover-more detail-title detail-status detail-content`;

class Element {
  constructor(tag = "div") {
    this.tagName = tag; this.children = []; this.handlers = new Map(); this._text = ""; this.value = "";
    this.hidden = false; this.disabled = false; this.classList = { toggle() {} };
  }
  set textContent(value) { this.children = []; this._text = String(value); }
  get textContent() { return this._text + this.children.map((child) => child.textContent).join(""); }
  append(...children) { for (const child of children) { child.parent = this; this.children.push(child); } if (this.tagName === "select" && !this.value) this.value = this.children[0]?.value || ""; }
  replaceChildren(...children) { this.children = []; this._text = ""; if (this.tagName === "select") this.value = ""; this.append(...children); }
  get lastElementChild() { return this.children.at(-1); }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter((child) => child !== this); }
  addEventListener(name, handler) { this.handlers.set(name, handler); }
  dispatch(name) { this.handlers.get(name)?.({ preventDefault() {} }); }
  click() { this.dispatch("click"); }
  querySelector(name) { return name === "button" ? this.submitButton : null; }
}
function makeDocument() {
  const nodes = new Map(ids.split(" ").map((id) => [id, new Element(id.endsWith("form") ? "form" : id === "gen-timeline" ? "select" : "div")]));
  nodes.get("discover-form").submitButton = new Element("button");
  nodes.get("gen-kind").value = "character";
  return { nodes, document: { getElementById: (id) => nodes.get(id), createElement: (tag) => new Element(tag) } };
}
const flush = async () => { for (let index = 0; index < 8; index += 1) await new Promise((resolve) => setTimeout(resolve, 0)); };
function countMounted(nodes) {
  const count = (element) => 1 + element.children.reduce((sum, child) => sum + count(child), 0);
  return [...nodes.values()].reduce((sum, element) => sum + count(element), 0);
}
const citation = { path: "story/generational.md", applicability: { applicability_kind: "instant", point: { timeline: "main", tick: "-1", order: "0" } } };
const row = (recordId, state, value) => ({ recordId, state, value, citations: [citation], causes: [] });
function fixture(size = 1000) {
  const calls = [], labels = {
    "character:one": "Mara <script>alert(1)</script>", "character:two": "Ilyra", "organization:one": "House Aster",
    "legacy:one": "Keeper of Keys", "event:one": "Council's decree",
  };
  let deferred = null, drift = false, bootstrapMode = "valid", deferAllTime = false, releaseAllTime = null;
  const fetcher = async (path, options) => {
    assert.ok(path.startsWith("/")); assert.doesNotMatch(path, /\/api\/entities|https?:\/\//);
    const operation = path.split("/").at(-1), request = options.body ? JSON.parse(options.body) : null;
    calls.push({ path, request, requestBytes: Buffer.byteLength(options.body || "") + path.length });
    if (request) { assert.equal(request.revision, revision); assert.deepEqual(request.capabilities, capabilities); assert.equal(request.at?.tick === undefined || typeof request.at.tick === "string", true); }
    if (operation === "session") return { ok: true, json: async () => ({ token: "local" }) };
    const common = { protocol: "wedl-generational/v1", operation, revision: drift ? "b".repeat(40) : revision };
    const payload = operation === "bootstrap" ? { ...common,
        ...(bootstrapMode === "malformed" ? { protocol: "unexpected" } : {}), state: "available",
        capabilities: bootstrapMode === "missing-capability" ? [] : capabilities, timelines: ["main"] }
      : operation === "discover" ? { ...common, state: "available", results: request.kind === "character"
        ? Array.from({ length: Math.min(20, size) }, (_, i) => ({ id: i ? `character:${i + 1}` : "character:one", title: i ? `Person ${i + 1}` : labels["character:one"], kind: request.kind, matchedName: "Mar" }))
        : [{ id: `${request.kind}:one`, title: labels[`${request.kind}:one`], kind: request.kind, matchedName: request.text }], cursor: request.kind === "character" && size > 20 ? "next" : null }
      : operation === "labels" ? { ...common, state: "available", labels: request.ids.filter((id) => labels[id]).map((id) => ({ id, title: labels[id] })) }
      : operation === "parents" ? { ...common, state: "unknown" }
      : operation === "ancestors" ? { ...common, state: "limit", code: "GEN-LIMIT-001" }
      : operation === "descendants" ? { ...common, state: "available", relations: [] }
      : operation === "vital" ? { ...common, state: "available", vital: request.at.order === "0" ? "living" : "dead", citations: [citation] }
      : operation === "character-unions" ? { ...common, state: "available", unions: [row("union:one", "formed", { participant_ids: ["character:one", "character:two", "character:secret"] })] }
      : operation === "organization" ? { ...common, state: "available", organization: row("organization:one", "active", { organization_kind: "house" }), parentPath: [],
        roles: [row("affiliation:one", "active", { character_id: "character:one", role: "warden" })],
        formerRoles: [row("affiliation:two", "ended", { character_id: "character:two", role: null })] }
      : operation === "organization-legacies" ? { ...common, state: "available", legacies: [row("legacy:one", "active", { legacy_kind: "office" })] }
      : operation === "legacy" ? { ...common, state: "available", legacy: row("legacy:one", "active", { organization_id: "organization:one", legacy_kind: "office" }),
        tenures: [row("tenure:one", "holding", { holder_id: "character:one", basis: "legal", successor_tenure_id: "tenure:two" }), row("tenure:two", "vacant", { holder_id: null, basis: "de-facto", predecessor_tenure_id: "tenure:one" })],
        holders: request.mode === "author-all-time" ? [] : [row("tenure:one", "holding", { holder_id: "character:one", basis: "legal" })],
        claims: [row("claim:one", "asserted", { claimant_id: "character:two" })], succession: [{ from: "tenure:one", to: "tenure:two", citations: [citation], causes: [{ eventId: "event:one", citation }] }] }
      : { ...common, state: "unknown" };
    const bytes = Buffer.byteLength(JSON.stringify(payload));
    assert.ok(bytes < 80_000, `response budget ${bytes}`);
    if (deferred && operation === "discover") return new Promise((resolve) => { deferred = () => resolve({ ok: true, json: async () => payload }); });
    if (deferAllTime && operation === "legacy" && request.mode === "author-all-time") {
      return new Promise((resolve) => { releaseAllTime = () => resolve({ ok: true, json: async () => ({ ...payload, revision: "b".repeat(40) }) }); });
    }
    return { ok: !["limit", "invalid", "unavailable"].includes(payload.state), json: async () => payload };
  };
  return { calls, fetcher, setDeferred: (value) => { deferred = value; }, release: () => deferred?.(), setDrift: (value) => { drift = value; },
    setBootstrapMode: (value) => { bootstrapMode = value; }, setDeferAllTime: (value) => { deferAllTime = value; },
    releaseAllTime: () => releaseAllTime?.() };
}

async function ready(size = 1000) {
  const { nodes, document } = makeDocument(), service = fixture(size);
  const app = mountGenerationalExplorer(document, createGenerationalApi(service.fetcher)); await flush();
  nodes.get("gen-tick").value = "-9223372036854775808";
  nodes.get("gen-order").value = "0";
  nodes.get("horizon-form").dispatch("submit");
  return { nodes, service, app };
}

test("bounded discovery replaces pages; character horizon keeps unknown, empty, limit, names and malicious text inert", async () => {
  const { nodes, service } = await ready();
  nodes.get("gen-query").value = "Mar"; nodes.get("discover-form").dispatch("submit"); await flush();
  assert.equal(nodes.get("discover-results").children.length, 20);
  assert.equal(nodes.get("discover-more").hidden, false);
  nodes.get("discover-more").click(); await flush();
  assert.equal(nodes.get("discover-results").children.length, 20);
  nodes.get("discover-results").children[0].children[0].click(); await flush();
  const detail = nodes.get("detail-content").textContent;
  assert.match(detail, /-9223372036854775808/);
  assert.match(detail, /Unknown: no admitted affirmative evidence/);
  assert.match(detail, /GEN-LIMIT-001/);
  assert.match(detail, /No authored relations/);
  assert.match(detail, /Mara <script>alert\(1\)<\/script>/);
  assert.match(detail, /Unavailable at this horizon/);
  assert.doesNotMatch(detail, /character:|union:|affiliation:/);
  assert.ok(countMounted(nodes) < 650, `mounted DOM ${countMounted(nodes)}`);
  assert.ok(service.calls.every((call) => call.requestBytes < 5000));
  assert.ok(service.calls.filter((call) => call.path.endsWith("/labels")).every((call) => call.request.ids.length <= 100));
  nodes.get("gen-order").value = "1"; nodes.get("horizon-form").dispatch("submit");
  assert.equal(nodes.get("detail-content").textContent, "");
  assert.equal(nodes.get("discover-results").children.length, 0);
});

test("organization active/former roles and legacy holder, vacancy, claim, succession, and separate all-time are explicit", async () => {
  const { nodes, service } = await ready();
  nodes.get("gen-kind").value = "organization"; nodes.get("gen-query").value = "House";
  nodes.get("discover-form").dispatch("submit"); await flush();
  nodes.get("discover-results").children[0].children[0].click(); await flush();
  let detail = nodes.get("detail-content").textContent;
  assert.match(detail, /Active affiliations/);
  assert.match(detail, /Former affiliations/);
  assert.match(detail, /Role: warden/);
  assert.match(detail, /Role: Role unspecified/);
  assert.match(detail, /Keeper of Keys/);
  assert.doesNotMatch(detail, /organization:|affiliation:|legacy:/);
  assert.equal(service.calls.find((call) => call.path.endsWith("/organization")).request.includeFormerRoles, true);

  nodes.get("gen-kind").value = "legacy"; nodes.get("gen-query").value = "Keeper";
  nodes.get("discover-form").dispatch("submit"); await flush();
  nodes.get("discover-results").children[0].children[0].click(); await flush();
  detail = nodes.get("detail-content").textContent;
  assert.match(detail, /Current holding tenures: 1/);
  assert.match(detail, /Holder: Vacant/);
  assert.match(detail, /Claims, distinct from tenures/);
  assert.match(detail, /Tenure 1 → Tenure 2/);
  assert.match(detail, /Basis: legal/);
  assert.match(detail, /Basis: de-facto/);
  assert.doesNotMatch(detail, /tenure:|claim:/);
  nodes.get("detail-content").lastElementChild.click(); await flush();
  assert.match(nodes.get("detail-content").textContent, /All-time authored history/);
  const allTime = service.calls.findLast((call) => call.path.endsWith("/legacy"));
  assert.equal(allTime.request.mode, "author-all-time");
  assert.equal(Object.hasOwn(allTime.request, "at"), false);
});

test("horizon changes cancel stale discovery responses and revision drift closes the view", async () => {
  const { nodes, service } = await ready(20);
  service.setDeferred(true);
  nodes.get("gen-query").value = "Mar"; nodes.get("discover-form").dispatch("submit"); await flush();
  nodes.get("gen-tick").value = "9223372036854775807";
  nodes.get("gen-order").value = "-9223372036854775808";
  nodes.get("horizon-form").dispatch("submit");
  service.release(); await flush();
  assert.equal(nodes.get("discover-results").children.length, 0);
  assert.match(nodes.get("horizon-label").textContent, /9223372036854775807, order -9223372036854775808/);
  service.setDeferred(null); service.setDrift(true);
  nodes.get("gen-query").value = "Mar"; nodes.get("discover-form").dispatch("submit"); await flush();
  assert.equal(nodes.get("discover-form").submitButton.disabled, true);
  assert.match(nodes.get("gen-status").textContent, /world changed/i);
  assert.equal(nodes.get("detail-content").textContent, "");
});

test("a prior legacy all-time reply cannot clear a newer selected organization", async () => {
  const { nodes, service, app } = await ready();
  const legacy = { id: "legacy:one", kind: "legacy", title: "Keeper of Keys" };
  await app.select(legacy);
  service.setDeferAllTime(true);
  nodes.get("detail-content").lastElementChild.click(); await flush();
  await app.select({ id: "organization:one", kind: "organization", title: "House Aster" });
  service.releaseAllTime(); await flush();
  assert.match(nodes.get("horizon-label").textContent, /Local-author evidence at/);
  assert.equal(nodes.get("detail-title").textContent, "House Aster");
  assert.match(nodes.get("detail-content").textContent, /Former affiliations/);
  assert.equal(nodes.get("discover-form").submitButton.disabled, false);
  assert.doesNotMatch(nodes.get("gen-status").textContent, /world changed/i);
});

test("capability loss or malformed bootstrap clears mounted horizon, search, and detail", async () => {
  for (const mode of ["missing-capability", "malformed"]) {
    const { nodes, service, app } = await ready();
    nodes.get("gen-query").value = "Mar"; nodes.get("discover-form").dispatch("submit"); await flush();
    await app.select({ id: "character:one", kind: "character", title: "Mara" });
    assert.ok(nodes.get("detail-content").textContent.length > 0);
    service.setBootstrapMode(mode);
    nodes.get("discover-form").dispatch("submit"); await flush();
    assert.equal(nodes.get("horizon-label").textContent, "No horizon selected.");
    assert.equal(nodes.get("discover-form").submitButton.disabled, true);
    assert.equal(nodes.get("discover-results").children.length, 0);
    assert.equal(nodes.get("detail-content").textContent, "");
    assert.match(nodes.get("gen-status").textContent, /world changed/i);
  }
});
