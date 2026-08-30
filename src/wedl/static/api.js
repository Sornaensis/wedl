export class ApiError extends Error {
  constructor(message, { status = 0, body = null, code = "", details = {} } = {}) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.body = body;
    this.code = code;
    this.details = details;
  }
}

export function createApiClient() {
  let token = "";

  function errorPayload(body) {
    try {
      const payload = JSON.parse(body);
      const message = typeof payload.message === "string" && payload.message
        ? payload.message
        : typeof payload.detail === "string" && payload.detail
          ? payload.detail
          : payload.detail != null ? JSON.stringify(payload.detail) : "";
      return { payload, message };
    } catch {
      return { payload: null, message: body };
    }
  }

  function errorDetail(body) {
    try {
      const payload = JSON.parse(body);
      for (const key of ["message", "detail"]) {
        if (typeof payload[key] === "string" && payload[key]) return payload[key];
        if (payload[key] != null) return JSON.stringify(payload[key]);
      }
    } catch {
      // A plain-text response remains useful below.
    }

    return body;
  }

  return {
    setToken(value) {
      token = typeof value === "string" ? value : "";
    },
    sessionToken() { return token; },
    async request(path, { method = "GET", body, confirmationToken = "", headers: suppliedHeaders = {}, ...options } = {}) {
      const headers = {
        ...suppliedHeaders,
        ...(token ? { "X-Wedl-Token": token } : {}),
        ...(confirmationToken ? { "X-Wedl-Confirmation": confirmationToken } : {}),
      };
      const hasBody = body !== undefined;
      if (hasBody && !Object.keys(headers).some((key) => key.toLowerCase() === "content-type")) headers["Content-Type"] = "application/json";
      const response = await fetch(path, { ...options, method, headers, ...(hasBody ? { body: typeof body === "string" ? body : JSON.stringify(body) } : {}) });

      if (!response.ok) {
        const raw = await response.text(); const parsed = errorPayload(raw);
        throw new ApiError(parsed.message || errorDetail(raw) || `Request failed with status ${response.status}.`, {
          status: response.status, body: parsed.payload, code: parsed.payload && typeof parsed.payload.code === "string" ? parsed.payload.code : "", details: parsed.payload && plainDetails(parsed.payload.details),
        });
      }

      return response.json();
    },
    get(path, options = {}) { return this.request(path, { ...options, method: "GET" }); },
    post(path, body, options = {}) { return this.request(path, { ...options, method: "POST", body }); },
  };
}

function plainDetails(value) { return value && typeof value === "object" && !Array.isArray(value) ? value : {}; }
