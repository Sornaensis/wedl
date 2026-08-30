import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  CHRONOLOGY_PROTOCOL,
  assertCanonicalDecimal,
  buildChronologyReplaceIntent,
  buildConvertRequest,
  buildFormatRequest,
  buildSearchRequest,
  buildStoryTimesRequest,
  chronologyValueAriaLabel,
  chronologyValueLabel,
  cloneChronologyAnnotation,
  cloneChronologyValue,
  isCanonicalDecimal,
} from "../src/wedl/static/chronology.mjs";
import { ApiError, createApiClient } from "../src/wedl/static/api.js";
import { historyAction, navigationSnapshot, restoreNavigation } from "../src/wedl/static/navigation.mjs";

const civil = { kind: "civil", calendarId: "calendar_archive", year: "-9223372036854775808", month: "1", day: "1", "x-payload": { calendar_id: "$calendar.keep", nested: ["9223372036854775807"] }, tagExtensions: { "x-tag": { temporaryId: "keep" } } };

test("chronology UI values retain every exact coordinate and opaque extension without numeric coercion", () => {
  for (const value of ["-9223372036854775808", "-9007199254740993", "0", "9007199254740993", "9223372036854775807"]) assert.equal(isCanonicalDecimal(value), true);
  for (const value of ["-0", "+1", "01", " 1", "9223372036854775808", "-9223372036854775809"]) assert.equal(isCanonicalDecimal(value), false);
  assert.throws(() => assertCanonicalDecimal("9223372036854775808"), /canonical signed i64/);
  const value = cloneChronologyValue({ kind: "conflict", claims: [civil, { kind: "approximate", displayValue: "about the old archive", bounds: { lower: null, upper: null, "x-bounds": { era_id: "$era.keep" } }, "x-approx": [7, { snake_key: true }], tagExtensions: { "x-approx-tag": { temporaryId: "keep" } } }] });
  assert.deepEqual(value.claims[0]["x-payload"], civil["x-payload"]);
  assert.deepEqual(value.claims[0].tagExtensions, civil.tagExtensions);
  assert.deepEqual(value.claims[1].bounds["x-bounds"], { era_id: "$era.keep" });
  assert.match(chronologyValueLabel(value), /Conflicting dates/);
  assert.match(chronologyValueAriaLabel(value.claims[1]), /Approximate/);
});

test("every public chronology kind has a closed, cloneable authoring shape", () => {
  const values = [
    civil,
    { kind: "era", eraId: "era_foundation", year: "0" },
    { kind: "range", calendarId: "calendar_archive", lower: { calendarId: "calendar_archive", year: "-1" }, upper: null },
    { kind: "approximate", displayValue: "late winter", bounds: { calendarId: "calendar_archive", lower: { calendarId: "calendar_archive", year: "1" }, upper: null } },
    { kind: "conflict", claims: [{ kind: "civil", calendarId: "calendar_archive", year: "1" }, { kind: "era", eraId: "era_foundation", year: "1" }] },
    { kind: "relative", relation: "before", beforeId: "$chronology.reference", "x-relative": { after_id: "$chronology.keep" } },
    { kind: "duration", unit: "day", value: "9223372036854775807" },
  ];
  assert.deepEqual(values.map(cloneChronologyValue), values);
  assert.throws(() => cloneChronologyValue({ kind: "civil", calendarId: "calendar_archive", day: "1", year: "0" }), /requires month/);
  assert.throws(() => cloneChronologyValue({ kind: "relative", relation: "before" }), /beforeId and\/or afterId/);
});

test("request builders use raw v1 bodies and retain BETWEEN, era mode, and exact values", () => {
  assert.deepEqual(buildFormatRequest(civil), { protocol: CHRONOLOGY_PROTOCOL, value: civil });
  assert.deepEqual(buildStoryTimesRequest(civil), { protocol: CHRONOLOGY_PROTOCOL, value: civil });
  assert.deepEqual(buildConvertRequest(civil, { calendarId: "calendar_archive" }).target, { calendarId: "calendar_archive" });
  const request = buildSearchRequest({ predicate: "between", value: civil, upper: { kind: "civil", calendarId: "calendar_archive", year: "9007199254740993" }, eraFilter: { eraId: "era_foundation", mode: "overlaps_bounds" }, limit: 256 });
  assert.equal(request.protocol, CHRONOLOGY_PROTOCOL);
  assert.equal(request.upper.year, "9007199254740993");
  assert.deepEqual(request.eraFilter, { eraId: "era_foundation", mode: "overlaps_bounds" });
  assert.throws(() => buildSearchRequest({ predicate: "between", value: civil }), /requires upper/);
  assert.throws(() => buildConvertRequest(civil, { calendarId: "a", eraId: "b" }), /exactly/);
});

test("complete record replacement preserves annotation identity, provenance, relative and duration values", () => {
  const annotations = [
    { id: "chronology_one", role: "authored", display: "First", provenance: ["ledger"], value: civil, "x-note": { calendar_id: "$calendar.keep" } },
    { temporaryId: "$chronology.second", provenance: ["memory"], value: { kind: "relative", relation: "after", afterId: "$chronology.one" } },
    { id: "chronology_three", provenance: [], value: { kind: "duration", unit: "month", value: "-9007199254740993" } },
  ];
  const intent = buildChronologyReplaceIntent({ expectedHead: "a".repeat(40), record: "A record title", annotations, idempotencyKey: "ui-replay" });
  assert.equal(intent.action, "chronology.replace");
  assert.deepEqual(intent.change.records[0].annotations, annotations);
  assert.deepEqual(cloneChronologyAnnotation(annotations[0])["x-note"], annotations[0]["x-note"]);
});

test("API client POST carries JSON, session and confirmation while preserving structured WEDL failures", async () => {
  const original = globalThis.fetch; let request;
  globalThis.fetch = async (path, options) => { request = { path, options }; return { ok: true, json: async () => ({ outcome: "ok" }) }; };
  try {
    const client = createApiClient(); client.setToken("session");
    await client.post("/api/authoring/apply", { action: "chronology.replace" }, { confirmationToken: "wedl-confirmation/v1:proof" });
    assert.equal(request.options.method, "POST"); assert.equal(request.options.headers["X-Wedl-Token"], "session"); assert.equal(request.options.headers["X-Wedl-Confirmation"], "wedl-confirmation/v1:proof"); assert.equal(request.options.headers["Content-Type"], "application/json");
    globalThis.fetch = async () => ({ ok: false, status: 400, text: async () => JSON.stringify({ code: "upgrade_required", message: "Upgrade first", details: { sourceSchema: "wedl/v0.5" } }) });
    await assert.rejects(client.post("/api/authoring/preview", {}), (error) => error instanceof ApiError && error.code === "upgrade_required" && error.details.sourceSchema === "wedl/v0.5");
  } finally { globalThis.fetch = original; }
});

test("chronology navigation is a stable article pane without changing existing timeline state", () => {
  const opened = historyAction({ view: "index", mobilePane: "index" }, { type: "open-chronology" });
  assert.deepEqual({ view: opened.view, mobilePane: opened.mobilePane }, { view: "chronology", mobilePane: "article" });
  const snapshot = navigationSnapshot({ ...opened, returnView: "index", activeTimelineId: "main", horizon: null, kind: "", query: "" });
  assert.equal(restoreNavigation(snapshot).view, "chronology");
});

test("UI module advertises server-owned chronology ordering and does not perform chronology arithmetic", async () => {
  const source = await readFile(new URL("../src/wedl/static/app.js", import.meta.url), "utf8");
  const helper = await readFile(new URL("../src/wedl/static/chronology.mjs", import.meta.url), "utf8");
  assert.match(source, /\/api\/chronology\/search/);
  assert.match(source, /server-ordered matching annotations/);
  assert.match(source, /Choose a reading horizon explicitly/);
  assert.match(source, /chronologyAnnotations/);
  assert.doesNotMatch(helper, /parseInt|parseFloat|\bNumber\b|\bDate\b|\bIntl\b/);
});
