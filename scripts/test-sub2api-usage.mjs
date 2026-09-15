#!/usr/bin/env node

/**
 * Read-only Sub2API usage verification.
 *
 * Fill CONFIG below, or provide the same values through environment variables:
 *   SUB2API_BASE_URL
 *   SUB2API_API_KEY
 *   SUB2API_AUTH_TOKEN
 *   SUB2API_REFRESH_TOKEN
 *   SUB2API_TOKEN_EXPIRES_AT
 *
 * This script only calls authentication, user-panel, and usage endpoints. It
 * never calls a model endpoint, writes to the database, or prints credentials.
 */

const CONFIG = {
  baseUrl: "https://www.rayinai.com",
  apiKey: "",
  authToken: "",
  refreshToken: "",
  tokenExpiresAt: "",
};

const baseUrl = (process.env.SUB2API_BASE_URL || CONFIG.baseUrl).trim().replace(/\/$/, "");
const apiKey = (process.env.SUB2API_API_KEY || CONFIG.apiKey).trim();
let authToken = (process.env.SUB2API_AUTH_TOKEN || CONFIG.authToken).trim();
let refreshToken = (process.env.SUB2API_REFRESH_TOKEN || CONFIG.refreshToken).trim();
const tokenExpiresAtRaw = (process.env.SUB2API_TOKEN_EXPIRES_AT || CONFIG.tokenExpiresAt).trim();

let criticalFailures = 0;
let refreshAttempted = false;

const KNOWN_FIELDS = [
  "cost",
  "actual_cost",
  "total_cost",
  "input_cost",
  "output_cost",
  "average_duration_ms",
  "duration_ms",
  "first_token_ms",
  "request_id",
];

function fail(message, critical = true) {
  console.error(`${critical ? "FAIL" : "WARN"}: ${message}`);
  if (critical) {
    criticalFailures += 1;
    process.exitCode = 1;
  }
}

function pass(message) {
  console.log(`PASS: ${message}`);
}

function note(message) {
  console.log(`INFO: ${message}`);
}

function unwrap(payload) {
  if (payload && typeof payload === "object" && "code" in payload && "data" in payload) {
    return payload.data;
  }
  return payload;
}

function responseItems(payload) {
  const data = unwrap(payload);
  if (Array.isArray(data)) return data;
  if (data && Array.isArray(data.items)) return data.items;
  if (data && Array.isArray(data.records)) return data.records;
  return [];
}

function firstDefined(object, names) {
  for (const name of names) {
    if (object && object[name] !== undefined && object[name] !== null) {
      return object[name];
    }
  }
  return null;
}

function readField(object, names) {
  for (const name of names) {
    if (object && Object.prototype.hasOwnProperty.call(object, name)) {
      return { present: true, value: object[name] };
    }
  }
  return { present: false, value: null };
}

function numericValue(value) {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function formatCost(value) {
  const number = numericValue(value);
  return number === null ? "missing" : number.toFixed(10).replace(/0+$/, "").replace(/\.$/, "");
}

function formatCostField(field) {
  if (!field.present) return "missing";
  return field.value === null ? "null" : formatCost(field.value);
}

function formatFieldValue(name, value) {
  if (value === null) return "null";
  if (name === "request_id") return value === "" ? "empty" : "present";
  if (name.endsWith("_cost") || name === "cost") return formatCost(value);
  return String(value);
}

function parseTokenExpiry(raw) {
  if (!raw) return null;
  const numeric = Number(raw);
  if (Number.isFinite(numeric)) {
    // Accept both Unix seconds and Unix milliseconds. The documented value is
    // milliseconds, but Sub2API clients sometimes expose seconds.
    return numeric < 1_000_000_000_000 ? numeric * 1000 : numeric;
  }
  const parsed = Date.parse(raw);
  return Number.isNaN(parsed) ? null : parsed;
}

const tokenExpiresAt = parseTokenExpiry(tokenExpiresAtRaw);

function redact(text) {
  let result = String(text ?? "");
  for (const secret of [authToken, refreshToken]) {
    if (secret) result = result.split(secret).join("[redacted]");
  }
  return result.slice(0, 240);
}

function payloadError(payload) {
  if (!payload || typeof payload !== "object") return "";
  if ("_nonJson" in payload) return ": non-JSON response";
  const code = payload.code === undefined ? "" : ` (${payload.code})`;
  const message = payload.message ? `: ${redact(payload.message)}` : "";
  return `${code}${message}`;
}

async function readJson(response) {
  const text = await response.text();
  if (!text.trim()) return null;
  try {
    return JSON.parse(text);
  } catch {
    return { _nonJson: text.slice(0, 200) };
  }
}

function isSuccessful(response, payload) {
  if (!response.ok) return false;
  if (payload && typeof payload === "object" && "_nonJson" in payload) return false;
  if (payload && typeof payload === "object" && "code" in payload) {
    return payload.code === 0 || payload.code === "0";
  }
  return true;
}

function dataKeys(payload) {
  const data = unwrap(payload);
  if (Array.isArray(data)) return ["<array>"];
  if (data && typeof data === "object") return Object.keys(data);
  return [];
}

function arrayCounts(payload) {
  const data = unwrap(payload);
  if (Array.isArray(data)) return { data: data.length };
  if (!data || typeof data !== "object") return {};
  return Object.fromEntries(
    Object.entries(data)
      .filter(([, value]) => Array.isArray(value))
      .map(([key, value]) => [key, value.length]),
  );
}

function walkKnownFields(value, found = new Map(), depth = 0) {
  if (depth > 6 || value === null || value === undefined || typeof value !== "object") return found;
  if (Array.isArray(value)) {
    for (const item of value.slice(0, 100)) walkKnownFields(item, found, depth + 1);
    return found;
  }
  for (const [key, child] of Object.entries(value)) {
    if (KNOWN_FIELDS.includes(key) && !found.has(key)) {
      found.set(key, child);
    }
    walkKnownFields(child, found, depth + 1);
  }
  return found;
}

function reportKnownFields(label, payload) {
  const found = walkKnownFields(payload);
  const parts = KNOWN_FIELDS.map((name) => {
    if (!found.has(name)) return `${name}=missing`;
    return `${name}=${formatFieldValue(name, found.get(name))}`;
  });
  note(`${label} fields: ${parts.join(" ")}`);
}

function reportResponse(label, response, payload) {
  const data = unwrap(payload);
  const keys = dataKeys(payload);
  const items = responseItems(payload);
  const count = Array.isArray(data) ? data.length : items.length;
  const countText = count > 0 || Array.isArray(data) || (data && typeof data === "object" && ("items" in data || "records" in data))
    ? `; item_count=${count}`
    : "";
  const total = firstDefined(data, ["total", "total_count"]);
  const totalText = total === null ? "" : `; total=${total}`;
  const topLevel = payload && typeof payload === "object" ? Object.keys(payload) : [];
  const counts = arrayCounts(payload);
  const arrayCountText = Object.keys(counts).length > 0
    ? `; array_counts=${Object.entries(counts).map(([key, value]) => `${key}:${value}`).join(",")}`
    : "";
  note(`${label} top_level_keys=${topLevel.join(",") || "none"} data_keys=${keys.join(",") || "none"}${countText}${totalText}${arrayCountText}`);
  reportKnownFields(label, payload);
}

function requestOptionsWithAuth(options = {}) {
  const headers = new Headers(options.headers || {});
  headers.set("Accept", "application/json");
  if (authToken) headers.set("Authorization", `Bearer ${authToken}`);
  return { ...options, headers };
}

function requestOptionsWithApiKey(options = {}) {
  const headers = new Headers(options.headers || {});
  headers.set("Accept", "application/json");
  if (apiKey) headers.set("Authorization", `Bearer ${apiKey}`);
  return { ...options, headers };
}

async function request(path, options = {}) {
  return fetch(`${baseUrl}${path}`, requestOptionsWithAuth(options));
}

async function requestWithApiKey(path, options = {}) {
  return fetch(`${baseUrl}${path}`, requestOptionsWithApiKey(options));
}

async function refreshAccessToken() {
  if (refreshAttempted) return false;
  refreshAttempted = true;
  if (!refreshToken) {
    fail("access token is expired/near expiry and refresh_token is empty");
    return false;
  }

  let response;
  let payload;
  try {
    response = await fetch(`${baseUrl}/api/v1/auth/refresh`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
      },
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
    payload = await readJson(response);
  } catch (error) {
    fail(`POST /api/v1/auth/refresh failed: ${error instanceof Error ? error.message : String(error)}`);
    return false;
  }

  const data = unwrap(payload);
  const nextToken = data && typeof data.access_token === "string" ? data.access_token.trim() : "";
  const nextRefreshToken = data && typeof data.refresh_token === "string" ? data.refresh_token.trim() : "";
  if (!isSuccessful(response, payload) || !nextToken) {
    fail(`POST /api/v1/auth/refresh returned HTTP ${response.status}${payloadError(payload)}`);
    return false;
  }

  authToken = nextToken;
  if (nextRefreshToken) refreshToken = nextRefreshToken;
  pass(`POST /api/v1/auth/refresh returned HTTP ${response.status}`);
  if (data && data.expires_in !== undefined) note(`new access token lifetime: ${data.expires_in} seconds`);
  return true;
}

/**
 * Call one endpoint, retrying exactly once after a 401 when refresh_token is
 * available. The response body is parsed but only safe structural summaries
 * are printed.
 */
async function callEndpoint(method, path, options = {}, { critical = false, retry401 = true } = {}) {
  const label = `${method} ${path}`;
  let response;
  let payload;
  try {
    response = await request(path, { ...options, method });
    payload = await readJson(response);
  } catch (error) {
    fail(`${label} failed: ${error instanceof Error ? error.message : String(error)}`, critical);
    return { ok: false, response: null, payload: null, data: null };
  }

  if (response.status === 401 && retry401 && refreshToken && !refreshAttempted) {
    note(`${label} returned 401; attempting one refresh and retry`);
    if (await refreshAccessToken()) {
      try {
        response = await request(path, { ...options, method });
        payload = await readJson(response);
      } catch (error) {
        fail(`${label} retry failed: ${error instanceof Error ? error.message : String(error)}`, critical);
        return { ok: false, response: null, payload: null, data: null };
      }
    }
  }

  const ok = isSuccessful(response, payload);
  if (!ok) {
    fail(`${label} returned HTTP ${response.status}${payloadError(payload)}`, critical);
    return { ok: false, response, payload, data: unwrap(payload) };
  }

  pass(`${label} returned HTTP ${response.status}`);
  reportResponse(label, response, payload);
  return { ok: true, response, payload, data: unwrap(payload) };
}

async function callApiKeyEndpoint(method, path, options = {}, { critical = false } = {}) {
  const label = `${method} ${path} (API Key)`;
  if (!apiKey) {
    fail(`${label} was skipped because apiKey is empty`, critical);
    return { ok: false, response: null, payload: null, data: null };
  }
  let response;
  let payload;
  try {
    response = await requestWithApiKey(path, { ...options, method });
    payload = await readJson(response);
  } catch (error) {
    fail(`${label} failed: ${error instanceof Error ? error.message : String(error)}`, critical);
    return { ok: false, response: null, payload: null, data: null };
  }
  const ok = isSuccessful(response, payload);
  if (!ok) {
    fail(`${label} returned HTTP ${response.status}${payloadError(payload)}`, critical);
    return { ok: false, response, payload, data: unwrap(payload) };
  }
  pass(`${label} returned HTTP ${response.status}`);
  reportResponse(label, response, payload);
  return { ok: true, response, payload, data: unwrap(payload) };
}

function usageSample(items) {
  if (!items.length) return null;
  const sample = items[0];
  const idField = readField(sample, ["id", "usage_id"]);
  const requestIdField = readField(sample, ["request_id", "requestId"]);
  const actualCostField = readField(sample, ["actual_cost", "actualCost"]);
  const totalCostField = readField(sample, ["total_cost", "totalCost"]);
  const durationMsField = readField(sample, ["duration_ms", "durationMs"]);
  const firstTokenMsField = readField(sample, ["first_token_ms", "firstTokenMs"]);
  const id = idField.present ? idField.value : null;
  const requestId = requestIdField.present ? requestIdField.value : null;
  console.log(`SAMPLE: id=${id ?? "missing"} request_id=${requestId ? "present" : requestIdField.present ? "null/empty" : "missing"}`);
  console.log(`SAMPLE: actual_cost=${formatCostField(actualCostField)} total_cost=${formatCostField(totalCostField)} duration_ms=${durationMsField.present ? durationMsField.value ?? "null" : "missing"} first_token_ms=${firstTokenMsField.present ? firstTokenMsField.value ?? "null" : "missing"}`);
  for (const [label, field] of [
    ["request_id", requestIdField],
    ["actual_cost", actualCostField],
    ["total_cost", totalCostField],
    ["duration_ms", durationMsField],
    ["first_token_ms", firstTokenMsField],
  ]) {
    if (!field.present) note(`sample field ${label} is missing`);
    else pass(`sample field ${label} is available`);
  }
  return { id, requestId };
}

function extractIds(items) {
  return items
    .map((item) => firstDefined(item, ["id", "api_key_id"]))
    .filter((value) => value !== null && /^\d+$/.test(String(value)))
    .map((value) => Number(value))
    .filter((value) => Number.isSafeInteger(value) && value > 0);
}

async function main() {
  if (!baseUrl) {
    fail("baseUrl is empty; fill CONFIG.baseUrl or SUB2API_BASE_URL");
    return;
  }
  if (!authToken) {
    fail("authToken is empty; fill CONFIG.authToken or SUB2API_AUTH_TOKEN");
    return;
  }
  if (!apiKey) {
    fail("apiKey is empty; fill CONFIG.apiKey or SUB2API_API_KEY to verify GET /v1/usage");
    return;
  }
  if (tokenExpiresAtRaw && tokenExpiresAt === null) {
    fail("tokenExpiresAt must be a Unix timestamp (seconds or milliseconds) or an ISO date");
    return;
  }

  console.log(`Testing ${baseUrl}`);
  if (tokenExpiresAt !== null) {
    const remainingSeconds = Math.floor((tokenExpiresAt - Date.now()) / 1000);
    note(`configured access token remaining lifetime: ${remainingSeconds} seconds`);
    if (remainingSeconds <= 60) {
      note("access token is expired or near expiry; attempting refresh");
      if (!(await refreshAccessToken())) return;
    }
  }

  await callEndpoint("GET", "/api/v1/auth/me");
  await callEndpoint("GET", "/api/v1/user/profile");

  const keysResult = await callEndpoint("GET", "/api/v1/keys?page=1&page_size=100");
  const keyItems = keysResult.ok ? responseItems(keysResult.payload) : [];
  const apiKeyIds = [...new Set(extractIds(keyItems))];
  note(`API keys available for owned-key checks: ${apiKeyIds.length}`);

  const usageResult = await callEndpoint(
    "GET",
    "/api/v1/usage?page=1&page_size=5",
    {},
    { critical: true },
  );
  await callApiKeyEndpoint("GET", "/v1/usage", {}, { critical: true });
  const usageItems = usageResult.ok ? responseItems(usageResult.payload) : [];
  let sample = null;
  if (usageResult.ok) {
    sample = usageSample(usageItems);
    if (!usageItems.length) note("no usage records were returned, so single-record verification was skipped");
  }

  const today = new Date().toISOString().slice(0, 10);
  await callEndpoint(
    "GET",
    `/api/v1/usage?page=1&page_size=5&start_date=${today}&end_date=${today}`,
  );

  if (sample && sample.id !== null) {
    const detailResult = await callEndpoint(
      "GET",
      `/api/v1/usage/${encodeURIComponent(String(sample.id))}`,
      {},
      { critical: true },
    );
    if (detailResult.ok) {
      const detail = detailResult.data || {};
      const detailRequestIdField = readField(detail, ["request_id", "requestId"]);
      const detailRequestId = detailRequestIdField.present ? detailRequestIdField.value : null;
      if (sample.requestId && detailRequestId && sample.requestId !== detailRequestId) {
        fail("usage list/detail request_id values do not match", true);
      } else if (sample.requestId && detailRequestId) {
        pass("usage list/detail request_id values match");
      }
      const detailActualCost = readField(detail, ["actual_cost", "actualCost"]);
      const detailTotalCost = readField(detail, ["total_cost", "totalCost"]);
      const detailDurationMs = readField(detail, ["duration_ms", "durationMs"]);
      const detailFirstTokenMs = readField(detail, ["first_token_ms", "firstTokenMs"]);
      console.log(`DETAIL: actual_cost=${formatCostField(detailActualCost)} total_cost=${formatCostField(detailTotalCost)} duration_ms=${detailDurationMs.present ? detailDurationMs.value ?? "null" : "missing"} first_token_ms=${detailFirstTokenMs.present ? detailFirstTokenMs.value ?? "null" : "missing"}`);
    }
  }

  await callEndpoint("GET", "/api/v1/usage/stats?period=month");
  await callEndpoint("GET", "/api/v1/usage/dashboard/stats");
  await callEndpoint("GET", "/api/v1/usage/dashboard/trend?period=month&granularity=day");
  await callEndpoint("GET", "/api/v1/usage/dashboard/models?period=month");
  await callEndpoint("GET", "/api/v1/usage/dashboard/snapshot-v2?period=month&granularity=day&include_trend=true&include_model_stats=true&include_group_stats=true");

  if (apiKeyIds.length > 0) {
    await callEndpoint(
      "POST",
      "/api/v1/usage/dashboard/api-keys-usage",
      {
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ api_key_ids: apiKeyIds }),
      },
    );
    for (const apiKeyId of apiKeyIds) {
      await callEndpoint("GET", `/api/v1/user/api-keys/${apiKeyId}/usage/daily?days=30`);
    }
  } else {
    note("no owned API keys were returned; skipped API-key batch and per-key daily usage checks");
  }

  if (criticalFailures > 0) {
    fail(`${criticalFailures} critical verification check(s) failed`, false);
    process.exitCode = 1;
  } else {
    pass("all critical usage checks completed");
  }
}

main().catch((error) => {
  fail(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
