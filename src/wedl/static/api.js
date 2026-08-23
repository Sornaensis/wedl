export function createApiClient() {
  let token = "";

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
    async get(path, options = {}) {
      const headers = {
        ...(options.headers || {}),
        ...(token ? { "X-Wedl-Token": token } : {}),
      };
      const response = await fetch(path, { ...options, headers });

      if (!response.ok) {
        const detail = errorDetail(await response.text());
        throw new Error(detail || `Request failed with status ${response.status}.`);
      }

      return response.json();
    },
  };
}
