// Public chronology values are deliberately handled as JSON-shaped data.  This
// module never turns a chronology coordinate into a JavaScript number: a
// decimal string can be a signed i64 and must remain exact in a browser.

export const CHRONOLOGY_PROTOCOL = "wedl-chronology/v1";
export const CHRONOLOGY_KINDS = Object.freeze(["civil", "era", "range", "approximate", "conflict", "relative", "duration"]);
export const SEARCH_PREDICATES = Object.freeze(["on_date", "overlaps", "before", "after", "between"]);
export const ERA_MODES = Object.freeze(["authored", "overlaps_bounds"]);

const I64_MAGNITUDE = "9223372036854775808";
const I32_MAGNITUDE = "2147483648";
const EXTENSION = /^x-[A-Za-z0-9_.-]+$/;

function plainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function exactKeys(value, allowed, required, where) {
  if (!plainObject(value)) throw new TypeError(`${where} must be an object`);
  for (const key of required) if (!(key in value)) throw new TypeError(`${where}.${key} is required`);
  for (const key of Object.keys(value)) {
    if (!allowed.has(key) && !EXTENSION.test(key)) throw new TypeError(`${where}.${key} is not a public chronology field`);
  }
}

function cloneOpaque(value) {
  if (Array.isArray(value)) return value.map(cloneOpaque);
  if (plainObject(value)) {
    const result = {};
    for (const key of Object.keys(value)) result[key] = cloneOpaque(value[key]);
    return result;
  }
  return value;
}

function nonblank(value, where) {
  if (typeof value !== "string" || !value.trim()) throw new TypeError(`${where} must be a nonblank string`);
  return value;
}

function magnitudeAtMost(value, limit) {
  return value.length < limit.length || (value.length === limit.length && value <= limit);
}

export function isCanonicalDecimal(value, bits = "i64") {
  if (typeof value !== "string" || !/^-?(0|[1-9][0-9]*)$/.test(value) || value === "-0") return false;
  const magnitude = value.startsWith("-") ? value.slice(1) : value;
  const limit = bits === "i32" ? I32_MAGNITUDE : I64_MAGNITUDE;
  // Positive i64/i32 maxima are one less than the negative magnitude.
  if (!magnitudeAtMost(magnitude, limit)) return false;
  if (!value.startsWith("-") && magnitude === limit) return false;
  return true;
}

export function assertCanonicalDecimal(value, where = "coordinate", bits = "i64") {
  if (!isCanonicalDecimal(value, bits)) throw new TypeError(`${where} must be a canonical signed ${bits} decimal string`);
  return value;
}

function cloneTagExtensions(value, where) {
  if (!("tagExtensions" in value)) return undefined;
  const extensions = value.tagExtensions;
  if (!plainObject(extensions) || Object.keys(extensions).some((key) => !EXTENSION.test(key))) {
    throw new TypeError(`${where}.tagExtensions only permits x-* members`);
  }
  return cloneOpaque(extensions);
}

function copyExtensions(source, target) {
  for (const key of Object.keys(source)) if (EXTENSION.test(key)) target[key] = cloneOpaque(source[key]);
  const tagExtensions = cloneTagExtensions(source, "chronology value");
  if (tagExtensions !== undefined) target.tagExtensions = tagExtensions;
  return target;
}

function cloneEndpoint(value, calendarId, where) {
  if (value === null) return null;
  exactKeys(value, new Set(["calendarId", "year", "month", "day"]), new Set(["year"]), where);
  const result = { year: assertCanonicalDecimal(value.year, `${where}.year`) };
  const endpointCalendar = "calendarId" in value ? nonblank(value.calendarId, `${where}.calendarId`) : calendarId;
  if (endpointCalendar !== calendarId) throw new TypeError(`${where}.calendarId must match its range calendar`);
  if ("calendarId" in value) result.calendarId = endpointCalendar;
  if ("month" in value) result.month = assertCanonicalDecimal(value.month, `${where}.month`);
  if ("day" in value) {
    if (!("month" in value)) throw new TypeError(`${where}.day requires month`);
    result.day = assertCanonicalDecimal(value.day, `${where}.day`);
  }
  return copyExtensions(value, result);
}

function cloneDate(value, depth = 0) {
  if (!plainObject(value) || typeof value.kind !== "string") throw new TypeError("chronology value requires a kind");
  const kind = value.kind;
  if (kind === "civil" || kind === "era") {
    const id = kind === "civil" ? "calendarId" : "eraId";
    exactKeys(value, new Set(["kind", id, "year", "month", "day", "tagExtensions"]), new Set(["kind", id, "year"]), `${kind} value`);
    const result = { kind, [id]: nonblank(value[id], `${kind}.${id}`), year: assertCanonicalDecimal(value.year, `${kind}.year`) };
    if ("month" in value) result.month = assertCanonicalDecimal(value.month, `${kind}.month`);
    if ("day" in value) {
      if (!("month" in value)) throw new TypeError(`${kind}.day requires month`);
      result.day = assertCanonicalDecimal(value.day, `${kind}.day`);
    }
    return copyExtensions(value, result);
  }
  if (kind === "range") {
    exactKeys(value, new Set(["kind", "calendarId", "lower", "upper", "tagExtensions"]), new Set(["kind", "calendarId", "lower", "upper"]), "range value");
    const calendarId = nonblank(value.calendarId, "range.calendarId");
    return copyExtensions(value, { kind, calendarId, lower: cloneEndpoint(value.lower, calendarId, "range.lower"), upper: cloneEndpoint(value.upper, calendarId, "range.upper") });
  }
  if (kind === "approximate") {
    exactKeys(value, new Set(["kind", "displayValue", "bounds", "tagExtensions"]), new Set(["kind", "displayValue", "bounds"]), "approximate value");
    if (typeof value.displayValue !== "string") throw new TypeError("approximate.displayValue must be a string");
    if (!plainObject(value.bounds)) throw new TypeError("approximate.bounds must be an object");
    exactKeys(value.bounds, new Set(["calendarId", "lower", "upper"]), new Set(["lower", "upper"]), "approximate.bounds");
    const qualitative = value.bounds.lower === null && value.bounds.upper === null;
    if (qualitative && "calendarId" in value.bounds) throw new TypeError("qualitative approximate.bounds omits calendarId");
    if (!qualitative && !("calendarId" in value.bounds)) throw new TypeError("approximate.bounds.calendarId is required for a bound");
    const bounds = {
      lower: qualitative ? null : cloneEndpoint(value.bounds.lower, nonblank(value.bounds.calendarId, "approximate.bounds.calendarId"), "approximate.bounds.lower"),
      upper: qualitative ? null : cloneEndpoint(value.bounds.upper, nonblank(value.bounds.calendarId, "approximate.bounds.calendarId"), "approximate.bounds.upper"),
    };
    if (!qualitative) bounds.calendarId = value.bounds.calendarId;
    copyExtensions(value.bounds, bounds);
    return copyExtensions(value, { kind, displayValue: value.displayValue, bounds });
  }
  if (kind === "conflict") {
    if (depth >= 64) throw new TypeError("chronology conflict nesting exceeds 64");
    exactKeys(value, new Set(["kind", "claims", "tagExtensions"]), new Set(["kind", "claims"]), "conflict value");
    if (!Array.isArray(value.claims) || value.claims.length < 2 || value.claims.length > 64) throw new TypeError("conflict.claims must contain 2..64 values");
    return copyExtensions(value, { kind, claims: value.claims.map((item) => cloneDate(item, depth + 1)) });
  }
  if (kind === "relative") {
    exactKeys(value, new Set(["kind", "relation", "beforeId", "afterId", "tagExtensions"]), new Set(["kind", "relation"]), "relative value");
    if (typeof value.relation !== "string" || !("beforeId" in value || "afterId" in value)) throw new TypeError("relative value requires a relation and beforeId and/or afterId");
    const result = { kind, relation: value.relation };
    if ("beforeId" in value) result.beforeId = nonblank(value.beforeId, "relative.beforeId");
    if ("afterId" in value) result.afterId = nonblank(value.afterId, "relative.afterId");
    return copyExtensions(value, result);
  }
  if (kind === "duration") {
    exactKeys(value, new Set(["kind", "unit", "value", "tagExtensions"]), new Set(["kind", "unit", "value"]), "duration value");
    if (!["year", "month", "day"].includes(value.unit)) throw new TypeError("duration.unit is invalid");
    return copyExtensions(value, { kind, unit: value.unit, value: assertCanonicalDecimal(value.value, "duration.value") });
  }
  throw new TypeError(`unsupported chronology kind ${kind}`);
}

export function cloneChronologyValue(value) { return cloneDate(value); }
export function validateChronologyValue(value) { cloneDate(value); return true; }

export function cloneChronologyAnnotation(annotation) {
  exactKeys(annotation, new Set(["id", "temporaryId", "role", "display", "provenance", "value"]), new Set(["provenance", "value"]), "chronology annotation");
  const hasId = typeof annotation.id === "string";
  const hasTemporaryId = typeof annotation.temporaryId === "string";
  if (hasId === hasTemporaryId) throw new TypeError("chronology annotation requires exactly one id or temporaryId");
  if (!Array.isArray(annotation.provenance) || annotation.provenance.some((item) => typeof item !== "string")) throw new TypeError("chronology annotation provenance must be strings");
  const result = { ...(hasId ? { id: annotation.id } : { temporaryId: annotation.temporaryId }), provenance: [...annotation.provenance], value: cloneDate(annotation.value) };
  for (const key of ["role", "display"]) if (key in annotation) {
    if (typeof annotation[key] !== "string") throw new TypeError(`chronology annotation.${key} must be a string`);
    result[key] = annotation[key];
  }
  for (const key of Object.keys(annotation)) if (EXTENSION.test(key)) result[key] = cloneOpaque(annotation[key]);
  return result;
}

export function cloneChronologyAnnotations(annotations) {
  if (!Array.isArray(annotations)) throw new TypeError("chronology annotations must be an array");
  return annotations.map(cloneChronologyAnnotation);
}

function datePieces(value) {
  const year = value.year;
  const month = "month" in value ? `-${value.month}` : "";
  const day = "day" in value ? `-${value.day}` : "";
  return `${year}${month}${day}`;
}

export function chronologyValueLabel(value) {
  const item = cloneDate(value);
  if (item.kind === "civil") return `${item.calendarId}: ${datePieces(item)}`;
  if (item.kind === "era") return `${item.eraId}: ${datePieces(item)}`;
  if (item.kind === "range") return `${item.calendarId}: ${item.lower ? datePieces(item.lower) : "open"} to ${item.upper ? datePieces(item.upper) : "open"}`;
  if (item.kind === "approximate") return item.displayValue;
  if (item.kind === "conflict") return `Conflicting dates (${item.claims.length} claims)`;
  if (item.kind === "relative") return `Relative: ${item.relation}`;
  return `${item.value} ${item.unit}${item.value === "1" ? "" : "s"}`;
}

export function chronologyValueAriaLabel(value) {
  const item = cloneDate(value);
  if (item.kind === "range") return `Range in calendar ${item.calendarId}, from ${item.lower ? datePieces(item.lower) : "an open lower bound"} to ${item.upper ? datePieces(item.upper) : "an open upper bound"}`;
  if (item.kind === "approximate") return `Approximate date: ${item.displayValue}`;
  return chronologyValueLabel(item);
}

function requestValue(value) { return cloneDate(value); }
export function buildFormatRequest(value) { return { protocol: CHRONOLOGY_PROTOCOL, value: requestValue(value) }; }
export function buildStoryTimesRequest(value) { return { protocol: CHRONOLOGY_PROTOCOL, value: requestValue(value) }; }
export function buildConvertRequest(value, target) {
  if (!plainObject(target) || Object.keys(target).length !== 1 || !("calendarId" in target || "eraId" in target)) throw new TypeError("conversion target must contain exactly calendarId or eraId");
  const key = "calendarId" in target ? "calendarId" : "eraId";
  return { protocol: CHRONOLOGY_PROTOCOL, value: requestValue(value), target: { [key]: nonblank(target[key], `target.${key}`) } };
}
export function buildSearchRequest({ predicate, value, upper, eraFilter, limit } = {}) {
  if (!SEARCH_PREDICATES.includes(predicate)) throw new TypeError("search predicate is invalid");
  if ((predicate === "between") !== (upper !== undefined)) throw new TypeError("between requires upper and other predicates forbid it");
  const result = { protocol: CHRONOLOGY_PROTOCOL, predicate, value: requestValue(value) };
  if (upper !== undefined) result.upper = requestValue(upper);
  if (eraFilter !== undefined) {
    if (!plainObject(eraFilter) || Object.keys(eraFilter).length !== 2 || !ERA_MODES.includes(eraFilter.mode)) throw new TypeError("era filter is invalid");
    result.eraFilter = { eraId: nonblank(eraFilter.eraId, "eraFilter.eraId"), mode: eraFilter.mode };
  }
  if (limit !== undefined) {
    if (typeof limit !== "number" || limit !== limit || limit % 1 !== 0 || limit < 1 || limit > 10000) throw new TypeError("search limit must be an integer from 1 to 10000");
    result.limit = limit;
  }
  return result;
}

export function buildChronologyReplaceIntent({ expectedHead, record, annotations, summary, idempotencyKey } = {}) {
  if (typeof expectedHead !== "string" || !/^[0-9a-f]{40}$/.test(expectedHead)) throw new TypeError("expectedHead must be a 40-character lowercase SHA");
  const intent = { action: "chronology.replace", expectedHead, change: { records: [{ record: nonblank(record, "record"), annotations: cloneChronologyAnnotations(annotations) }] } };
  if (summary !== undefined) intent.summary = String(summary);
  if (idempotencyKey !== undefined) intent.idempotencyKey = String(idempotencyKey);
  return intent;
}

export function capabilityIsEnabled(catalog) {
  return Boolean(catalog && catalog.capability && catalog.capability.publicReads && catalog.capability.mode === "chronology-enabled");
}
