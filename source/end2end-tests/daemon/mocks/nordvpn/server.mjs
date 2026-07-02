// nordvpn_mock — a minimal, dependency-free HTTP server that simulates the
// subset of the NordVPN API the wardnet daemon's VPN provider calls, backed by
// the static fixtures in fixtures/nordvpn/. See issue #248 (E2E Stage 10).
//
// Endpoints (all under the daemon's configured base URL):
//   GET /health                          liveness probe (200 "ok")
//   GET /v1/servers/countries            -> countries.json
//   GET /v1/servers/recommendations      -> servers.json
//   GET /v1/servers                      -> servers.json (optionally hostname-filtered)
//   GET /v1/users/services               Basic-auth gate: 200 if valid, 401 otherwise
//   GET /v1/users/services/credentials   Basic-auth gate: credentials.json / 401
//
// The daemon authenticates with HTTP Basic `token:<TOKEN>` (username is the
// literal "token", password is the user's NordVPN token). We accept a single
// valid token, configurable via NORDVPN_VALID_TOKEN.

import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const PORT = Number(process.env.PORT ?? 8080);
const FIXTURES_DIR = process.env.FIXTURES_DIR ?? "/fixtures";
const VALID_TOKEN = process.env.NORDVPN_VALID_TOKEN ?? "valid-nordvpn-token";

// Read a fixture fresh on each request so a mounted-volume edit takes effect
// without an image rebuild (mirrors the busybox blocklist_server pattern).
function fixture(name) {
  return readFileSync(join(FIXTURES_DIR, name), "utf8");
}

function sendJson(res, status, body) {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(typeof body === "string" ? body : JSON.stringify(body));
}

// Returns true when the request carries HTTP Basic auth whose password equals
// the valid token. The daemon sends `Authorization: Basic base64("token:<t>")`.
function authOk(req) {
  const header = req.headers["authorization"] ?? "";
  const [scheme, encoded] = header.split(" ");
  if (scheme !== "Basic" || !encoded) return false;
  let decoded;
  try {
    decoded = Buffer.from(encoded, "base64").toString("utf8");
  } catch {
    return false;
  }
  const sep = decoded.indexOf(":");
  const password = sep === -1 ? "" : decoded.slice(sep + 1);
  return password === VALID_TOKEN;
}

const server = createServer((req, res) => {
  const url = new URL(req.url, "http://mock");
  const path = url.pathname;

  try {
    if (path === "/health") {
      res.writeHead(200, { "content-type": "text/plain" });
      res.end("ok");
      return;
    }

    if (path === "/v1/servers/countries") {
      sendJson(res, 200, fixture("countries.json"));
      return;
    }

    if (path === "/v1/servers/recommendations") {
      sendJson(res, 200, fixture("servers.json"));
      return;
    }

    if (path === "/v1/servers") {
      // Optional `filters[hostname]` narrowing — the daemon uses this path for
      // dedicated-IP lookups. With our single-server fixture we filter to match
      // real API semantics but otherwise return the list unchanged.
      const wanted = url.searchParams.get("filters[hostname]");
      const servers = JSON.parse(fixture("servers.json"));
      const body = wanted
        ? servers.filter((s) => s.hostname === wanted)
        : servers;
      sendJson(res, 200, body);
      return;
    }

    if (path === "/v1/users/services") {
      if (!authOk(req)) {
        sendJson(res, 401, { errors: { message: "Not found or invalid token" } });
        return;
      }
      // Real API returns the user's service list; the daemon only checks the
      // status code, so an empty array is sufficient.
      sendJson(res, 200, []);
      return;
    }

    if (path === "/v1/users/services/credentials") {
      if (!authOk(req)) {
        sendJson(res, 401, { errors: { message: "Not found or invalid token" } });
        return;
      }
      sendJson(res, 200, fixture("credentials.json"));
      return;
    }

    sendJson(res, 404, { errors: { message: `no mock route for ${path}` } });
  } catch (err) {
    sendJson(res, 500, { errors: { message: String(err?.message ?? err) } });
  }
});

server.listen(PORT, () => {
  console.log(`nordvpn_mock listening on :${PORT} (fixtures=${FIXTURES_DIR})`);
});
