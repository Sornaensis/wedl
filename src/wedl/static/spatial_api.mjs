export const EXPLORER_PROTOCOL = "wedl-spatial-explorer/v1";
export const SPATIAL_PROTOCOL = "wedl-spatial/v1";
export const PAGE_LIMIT = 50;

export class SpatialReadError extends Error {
  constructor(message, state = "error", code = "") {
    super(message);
    this.name = "SpatialReadError";
    this.state = state;
    this.code = code;
  }
}

export function createSpatialApi(fetcher = globalThis.fetch) {
  let token = "";
  let revision = "";
  let capabilities = [];

  async function request(path, body, signal) {
    const options = { signal, headers: token ? { "X-Wedl-Token": token } : {} };
    if (body !== undefined) {
      options.method = "POST";
      options.headers["Content-Type"] = "application/json";
      options.body = JSON.stringify(body);
    }
    const response = await fetcher(path, options);
    let payload;
    try { payload = await response.json(); }
    catch { throw new SpatialReadError(`Spatial read failed (${response.status}).`); }
    if (signal?.aborted) throw new DOMException("Spatial read cancelled.", "AbortError");
    if (!response.ok || payload?.state && payload.state !== "ok" || payload?.outcome && payload.outcome !== "ok") {
      throw new SpatialReadError(payload?.detail || payload?.message || `Spatial read failed (${response.status}).`, payload?.state || payload?.outcome || "error", payload?.code || "");
    }
    return payload;
  }

  function confirm(payload) {
    if (payload?.revision !== revision || JSON.stringify(payload?.capabilities) !== JSON.stringify(capabilities)) {
      revision = "";
      capabilities = [];
      throw new SpatialReadError("The world changed. Reload the spatial catalog and try again.", "stale_revision");
    }
    return payload.result;
  }

  return {
    get revision() { return revision; },
    get capabilities() { return [...capabilities]; },
    invalidate() { revision = ""; capabilities = []; },
    async catalog(signal, cursor = null) {
      const params = new URLSearchParams({ limit: String(PAGE_LIMIT) });
      if (cursor !== null) {
        params.set("revision", revision);
        for (const capability of capabilities) params.append("capabilities", capability);
        params.set("cursor", cursor);
      }
      const payload = await request(`/api/spatial/explorer/catalog?${params}`, undefined, signal);
      if (payload.protocol !== EXPLORER_PROTOCOL || payload.operation !== "catalog" || payload.state !== "ok" || !Array.isArray(payload.capabilities)) {
        throw new SpatialReadError("The spatial catalog has an unexpected format.");
      }
      if (cursor !== null) return confirm(payload);
      revision = payload.revision;
      capabilities = [...payload.capabilities];
      return payload.result;
    },
    async explorer(operation, fields, signal, cursor = null) {
      if (!revision) throw new SpatialReadError("Load the spatial catalog first.", "stale_revision");
      const body = { protocol: EXPLORER_PROTOCOL, revision, capabilities, limit: PAGE_LIMIT, cursor, ...fields };
      const payload = await request(`/api/spatial/explorer/${operation}`, body, signal);
      if (payload.protocol !== EXPLORER_PROTOCOL || payload.operation !== operation) throw new SpatialReadError("Unexpected spatial response.");
      return confirm(payload);
    },
    async path(fromLocationId, toLocationId, metric, signal) {
      if (!revision) throw new SpatialReadError("Load the spatial catalog first.", "stale_revision");
      const payload = await request("/api/spatial/path", { protocol: SPATIAL_PROTOCOL, revision, capabilities,
        limit: PAGE_LIMIT, cursor: null, fromLocationId, toLocationId, metric }, signal);
      if (payload.protocol !== SPATIAL_PROTOCOL || payload.operation !== "path") throw new SpatialReadError("Unexpected path response.");
      return confirm(payload);
    },
    async session(signal) {
      const payload = await request("/api/session", undefined, signal);
      token = typeof payload?.token === "string" ? payload.token : "";
    },
  };
}
