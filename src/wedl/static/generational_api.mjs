export const GENERATIONAL_PROTOCOL = "wedl-generational/v1";
export const PAGE_SIZE = 20;
export const READ_ITEMS = 50;

export class GenerationalReadError extends Error {
  constructor(message, state = "invalid", code = "") {
    super(message);
    this.name = "GenerationalReadError";
    this.state = state;
    this.code = code;
  }
}

const decimal = /^(?:0|-?[1-9][0-9]*)$/;
const MIN = -(1n << 63n);
const MAX = (1n << 63n) - 1n;

export function exactPoint(timeline, tick, order) {
  if (typeof timeline !== "string" || !timeline || typeof tick !== "string" ||
      typeof order !== "string" || !decimal.test(tick) || !decimal.test(order)) {
    throw new GenerationalReadError("Enter exact signed decimal tick and order strings.");
  }
  if ([tick, order].some((value) => BigInt(value) < MIN || BigInt(value) > MAX)) {
    throw new GenerationalReadError("Tick and order must fit the signed 64-bit StoryTime range.");
  }
  return { timeline, tick, order };
}

export function createGenerationalApi(fetcher = globalThis.fetch) {
  let token = "";
  let revision = "";
  let capabilities = [];
  let timelines = [];
  let scopeMode = "author-as-of", viewpoint = "", scopeEpoch = 0;

  async function transport(path, body, signal) {
    const options = { signal, headers: token ? { "X-Wedl-Token": token } : {} };
    if (body) {
      options.method = "POST";
      options.headers["Content-Type"] = "application/json";
      options.body = JSON.stringify(body);
    }
    const response = await fetcher(path, options);
    let payload;
    try { payload = await response.json(); }
    catch { throw new GenerationalReadError("The generational response could not be read.", "unavailable"); }
    if (signal?.aborted) throw new DOMException("Read cancelled.", "AbortError");
    return { payload, ok: response.ok };
  }

  function closed(payload, ok) {
    if (!ok && !["unknown", "invalid", "unavailable", "limit"].includes(payload?.state) ||
        !["available", "unknown", "invalid", "unavailable", "limit"].includes(payload?.state)) {
      throw new GenerationalReadError("The generational service is unavailable.", "unavailable");
    }
    return payload;
  }

  function envelope(operation, point, mode = "author-as-of") {
    if (!revision || !capabilities.length) throw new GenerationalReadError("Refresh the generational session.", "stale_revision");
    if (!timelines.includes(point.timeline)) throw new GenerationalReadError("Choose a declared timeline.");
    return { protocol: GENERATIONAL_PROTOCOL, operation, revision, capabilities: [...capabilities],
      mode, timeline: point.timeline, ...(mode !== "author-all-time" ? { at: point } : {}) };
  }

  return {
    get revision() { return revision; },
    get capabilities() { return [...capabilities]; },
    get timelines() { return [...timelines]; },
    get mode() { return scopeMode; },
    setScope(mode, selectedViewpoint = "") {
      if (!["author-as-of", "character"].includes(mode) || typeof selectedViewpoint !== "string" ||
          mode === "character" && (!selectedViewpoint.trim() || selectedViewpoint.length > 256)) {
        throw new GenerationalReadError("Choose an author view or enter a character viewpoint.");
      }
      scopeMode = mode; viewpoint = mode === "character" ? selectedViewpoint : ""; scopeEpoch += 1;
    },
    invalidate() { revision = ""; capabilities = []; timelines = []; },
    async boot(signal) {
      const session = await transport("/api/session", undefined, signal);
      token = typeof session.payload?.token === "string" ? session.payload.token : "";
      if (!session.ok || !token) throw new GenerationalReadError("The author session could not be opened.", "unavailable");
      return this.refresh(signal);
    },
    async refresh(signal) {
      const { payload, ok } = await transport("/api/generational/bootstrap", undefined, signal);
      if (!ok || payload?.protocol !== GENERATIONAL_PROTOCOL || payload?.operation !== "bootstrap" ||
          payload?.state !== "available" || !/^[0-9a-f]{40}$/.test(payload.revision) ||
          !Array.isArray(payload.capabilities) || !payload.capabilities.includes("generational-core-v1") ||
          !Array.isArray(payload.timelines)) {
        const hadEnvelope = Boolean(revision);
        this.invalidate();
        throw new GenerationalReadError("Generational evidence is unavailable in this world.",
          hadEnvelope ? "stale_revision" : "unavailable");
      }
      const drifted = Boolean(revision && (revision !== payload.revision ||
        JSON.stringify(capabilities) !== JSON.stringify(payload.capabilities) ||
        JSON.stringify(timelines) !== JSON.stringify(payload.timelines)));
      revision = payload.revision;
      capabilities = [...payload.capabilities];
      timelines = [...payload.timelines];
      if (drifted) throw new GenerationalReadError("The world changed. Choose the horizon again.", "stale_revision");
      return { revision, capabilities: [...capabilities], timelines: [...timelines] };
    },
    async read(operation, point, fields = {}, signal, mode = scopeMode) {
      if (scopeMode === "character" && mode !== "character") throw new GenerationalReadError("Author-only history is unavailable in character view.");
      const epoch = scopeEpoch;
      const body = { ...envelope(operation, point, mode), ...fields };
      const path = `/api/generational/${operation}` + (mode === "character" ? `?viewpoint=${encodeURIComponent(viewpoint)}` : "");
      const { payload, ok } = await transport(path, body, signal);
      if (epoch !== scopeEpoch) throw new DOMException("Viewpoint changed.", "AbortError");
      if (payload?.protocol !== GENERATIONAL_PROTOCOL || payload?.operation !== operation || payload?.revision !== revision) {
        this.invalidate();
        throw new GenerationalReadError("The world changed. Refresh the session.", "stale_revision");
      }
      const value = closed(payload, ok);
      if (value.state === "unavailable" && !ok) {
        this.invalidate();
        throw new GenerationalReadError("The world changed or its cache is unavailable. Refresh the session.", "stale_revision");
      }
      return value;
    },
    async discover(point, kind, text, cursor, signal) {
      return this.read("discover", point, { kind, text, items: PAGE_SIZE, cursor }, signal);
    },
    async labels(point, ids, signal) {
      if (!ids.length || ids.length > 100 || new Set(ids).size !== ids.length) throw new GenerationalReadError("Invalid label batch.");
      return this.read("labels", point, { ids, items: 100 }, signal);
    },
  };
}
