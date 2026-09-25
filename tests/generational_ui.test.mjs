import assert from "node:assert/strict";
import test from "node:test";
import { createGenerationalApi, exactPoint, GenerationalReadError } from "../src/wedl/static/generational_api.mjs";

const revision = "a".repeat(40);
const caps = ["generational-core-v1", "spatial-core-v1"];
const point = { timeline: "main", tick: "-9223372036854775808", order: "9223372036854775807" };

test("exact signed 64-bit horizon stays as decimal strings", () => {
  assert.deepEqual(exactPoint("main", point.tick, point.order), point);
  for (const value of ["-0", "01", "+1", " 1", "9223372036854775808", "-9223372036854775809"]) {
    assert.throws(() => exactPoint("main", value, "0"), GenerationalReadError);
  }
});

test("session, bootstrap, discovery, labels and leaves are bounded, same-origin, and pinned", async () => {
  const calls = [];
  const fetcher = async (path, options) => {
    assert.ok(path.startsWith("/"));
    assert.doesNotMatch(path, /\/api\/entities|https?:\/\//);
    const operation = path.split("/").at(-1);
    const body = options.body ? JSON.parse(options.body) : null;
    calls.push({ path, body, options });
    const payload = operation === "session" ? { token: "local" } : operation === "bootstrap"
      ? { protocol: "wedl-generational/v1", operation, state: "available", revision, capabilities: caps, timelines: ["main"] }
      : { protocol: "wedl-generational/v1", operation, state: "available", revision,
        ...(operation === "discover" ? { results: [], cursor: null } : operation === "labels" ? { labels: [] } : { relations: [] }) };
    return { ok: true, json: async () => payload };
  };
  const api = createGenerationalApi(fetcher);
  await api.boot();
  await api.discover(point, "character", "Mar", null);
  await api.labels(point, ["character:one"]);
  await api.read("organization", point, { subject: "organization:one", items: 20, depth: 8, includeFormerRoles: true });
  await api.read("legacy", point, { subject: "legacy:one", items: 20 }, undefined, "author-all-time");
  assert.deepEqual(calls.map(({ path }) => path), ["/api/session", "/api/generational/bootstrap", "/api/generational/discover", "/api/generational/labels", "/api/generational/organization", "/api/generational/legacy"]);
  for (const { body, options } of calls.slice(2)) {
    assert.equal(body.revision, revision);
    assert.deepEqual(body.capabilities, caps);
    assert.equal(options.headers["X-Wedl-Token"], "local");
  }
  assert.deepEqual(calls[2].body.at, point);
  assert.equal(calls[3].body.ids.length, 1);
  assert.equal(calls[4].body.includeFormerRoles, true);
  assert.equal(calls[5].body.mode, "author-all-time");
  assert.equal(Object.hasOwn(calls[5].body, "at"), false);
  await assert.rejects(() => api.labels(point, Array.from({ length: 101 }, (_, i) => String(i))), GenerationalReadError);
});

test("revision or capability drift invalidates the pinned envelope", async () => {
  let current = revision, currentCaps = caps;
  const api = createGenerationalApi(async (path) => {
    const operation = path.split("/").at(-1);
    const payload = operation === "session" ? { token: "local" } : operation === "bootstrap"
      ? { protocol: "wedl-generational/v1", operation, state: "available", revision: current, capabilities: currentCaps, timelines: ["main"] }
      : { protocol: "wedl-generational/v1", operation, state: "unknown", revision: current };
    return { ok: true, json: async () => payload };
  });
  await api.boot();
  current = "b".repeat(40);
  await assert.rejects(() => api.refresh(), { state: "stale_revision" });
  assert.equal(api.revision, current);
  currentCaps = ["generational-core-v1"];
  await assert.rejects(() => api.refresh(), { state: "stale_revision" });
});

test("capability loss and malformed bootstrap close a previously pinned envelope", async () => {
  for (const broken of ["missing-capability", "malformed"]) {
    let degrade = false;
    const api = createGenerationalApi(async (path) => {
      const operation = path.split("/").at(-1);
      const payload = operation === "session" ? { token: "local" } : {
        protocol: degrade && broken === "malformed" ? "unexpected" : "wedl-generational/v1",
        operation, state: "available", revision,
        capabilities: degrade && broken === "missing-capability" ? [] : caps,
        timelines: ["main"],
      };
      return { ok: true, json: async () => payload };
    });
    await api.boot();
    degrade = true;
    await assert.rejects(() => api.refresh(), { state: "stale_revision" });
    assert.equal(api.revision, "");
    assert.deepEqual(api.capabilities, []);
  }
});
