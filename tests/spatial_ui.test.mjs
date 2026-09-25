import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { createSpatialApi, EXPLORER_PROTOCOL, PAGE_LIMIT, SpatialReadError } from "../src/wedl/static/spatial_api.mjs";

const root = new URL("../src/wedl/static/", import.meta.url);
const read = (name) => readFile(new URL(name, root), "utf8");
const revision = "a".repeat(40);
const capabilities = ["spatial-core-v1", "geometry-v1", "route-v1", "overlay-v1"];

test("entrypoint is local, separate from compendium boot, and exposes keyboard/StoryTime controls", async () => {
  const [html, app, css, index] = await Promise.all([read("spatial.html"), read("spatial_app.mjs"), read("spatial.css"), read("index.html")]);
  assert.match(index, /href="\/assets\/spatial\.html"/);
  assert.match(html, /spatial_app\.mjs/);
  assert.doesNotMatch(html + app, /\/api\/entities|app\.js|https?:\/\//);
  assert.match(html, /id="story-timeline"/);
  assert.match(html, /id="story-tick"/);
  assert.match(html, /id="story-order"/);
  assert.match(html, /aria-live="polite"/);
  assert.match(css, /prefers-reduced-motion/);
  assert.match(css, /max-width: 700px/);
  assert.match(css, /:focus-visible/);
});

test("client binds bounded pages to exact revision and capabilities and rejects closed failures", async () => {
  const calls = [];
  const fetcher = async (path, options) => {
    calls.push({ path, options });
    const operation = path.includes("/catalog") ? "catalog" : "places";
    const payload = { protocol: EXPLORER_PROTOCOL, operation, revision, capabilities, state: "ok", result: operation === "catalog" ? { maps: [], nextCursor: "next", spatialAvailable: true } : { places: [], nextCursor: null } };
    return { ok: true, status: 200, json: async () => payload };
  };
  const api = createSpatialApi(fetcher);
  await api.catalog();
  await api.catalog(undefined, "next");
  await api.explorer("places", { mode: "roots" });
  assert.equal(calls.length, 3);
  assert.equal(new URL(calls[0].path, "http://local").searchParams.get("limit"), String(PAGE_LIMIT));
  assert.equal(new URL(calls[1].path, "http://local").searchParams.getAll("capabilities").length, 4);
  assert.deepEqual(JSON.parse(calls[2].options.body), { protocol: EXPLORER_PROTOCOL, revision, capabilities, limit: PAGE_LIMIT, cursor: null, mode: "roots" });

  const stale = createSpatialApi(async (path) => ({ ok: true, status: 200, json: async () => ({ protocol: EXPLORER_PROTOCOL,
    operation: path.includes("catalog") ? "catalog" : "places", revision: path.includes("catalog") ? revision : "b".repeat(40),
    capabilities, state: "ok", result: { maps: [], places: [], spatialAvailable: true } }) }));
  await stale.catalog();
  await assert.rejects(stale.explorer("places", { mode: "roots" }), { name: "SpatialReadError", state: "stale_revision" });
  assert.equal(stale.revision, "");

  const denied = createSpatialApi(async () => ({ ok: false, status: 403, json: async () => ({ state: "forbidden", code: "SPATIAL-OVERLAY-001", detail: "not visible" }) }));
  await assert.rejects(denied.catalog(), (error) => error instanceof SpatialReadError && error.state === "forbidden" && error.code === "SPATIAL-OVERLAY-001");
});
