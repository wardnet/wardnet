// wardnet_cloud_mock — a minimal, dependency-free HTTP server that stands in
// for the wardnet-cloud `tenants` (global) and `ddns` (regional) gateways the
// daemon's wardnet DDNS provider talks to. Lets the admin-app/user-app e2e
// projects walk the real enroll → register flow offline, so
// `Entitlement::is_entitled()` flips true and the premium-gated PWAs render.
// Mirrors ../../daemon/mocks/nordvpn/server.mjs in style.
//
// Serves BOTH the `tenants` and `ddns` path prefixes on one port: the daemon
// is pointed at this single mock for its global gateway, region gateway, AND
// region health-probe URL (see compose.ui.yaml's `[ddns_wardnet]` injection),
// since there is only one region in the e2e harness and nothing here needs
// them to be separate hosts.
//
// No auth/signature verification — the daemon's `PoP` signing (cloud/pop.rs)
// is a cloud-side concern; this mock only exists to make the daemon's own
// premium gate flip, so every request unconditionally succeeds.
//
// Endpoints:
//   GET    /health                              liveness probe (200 "ok")
//   GET    /ddns/v1/health                       region health probe (200 "ok")
//   POST   /tenants/v1/verification-codes        -> 204
//   POST   /tenants/v1/enroll                    -> 200 {tenant_id}
//   POST   /tenants/v1/token                     -> 200 {token}
//   GET    /tenants/v1/availability              -> 200 {available: true}
//   POST   /tenants/v1/networks                  -> 200 {id, slug, region, provisioning_state}
//   DELETE /tenants/v1/networks/:id/daemons/self -> 204
//   PUT    /ddns/v1/ip                           -> 204 (best-effort background publish)
//   PUT    /ddns/v1/acme-challenge               -> 204 (best-effort background publish)
//   DELETE /ddns/v1/acme-challenge               -> 204

import { createServer } from "node:http";
import { randomUUID } from "node:crypto";

const PORT = Number(process.env.PORT ?? 8080);

function sendJson(res, status, body) {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}

function readJsonBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on("data", (chunk) => chunks.push(chunk));
    req.on("end", () => {
      if (chunks.length === 0) {
        resolve({});
        return;
      }
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString("utf8")));
      } catch (err) {
        reject(err);
      }
    });
    req.on("error", reject);
  });
}

const server = createServer(async (req, res) => {
  const url = new URL(req.url, "http://mock");
  const path = url.pathname;
  console.log(
    `wardnet_cloud_mock: raw request line: ${req.method} ${req.url} ` +
      `(host header: ${req.headers.host}, from: ${req.socket.remoteAddress}:${req.socket.remotePort}, ` +
      `connection: ${req.headers.connection}, all headers: ${JSON.stringify(req.headers)})`,
  );

  try {
    if (req.method === "GET" && path === "/health") {
      res.writeHead(200, { "content-type": "text/plain" });
      res.end("ok");
      return;
    }

    if (req.method === "GET" && path === "/ddns/v1/health") {
      res.writeHead(200, { "content-type": "text/plain" });
      res.end("ok");
      return;
    }

    if (req.method === "POST" && path === "/tenants/v1/verification-codes") {
      res.writeHead(204);
      res.end();
      return;
    }

    if (req.method === "POST" && path === "/tenants/v1/enroll") {
      sendJson(res, 200, { tenant_id: `e2e-tenant-${randomUUID()}` });
      return;
    }

    if (req.method === "POST" && path === "/tenants/v1/token") {
      sendJson(res, 200, { token: `e2e-mock-jwt-${randomUUID()}` });
      return;
    }

    if (req.method === "GET" && path === "/tenants/v1/availability") {
      sendJson(res, 200, { available: true });
      return;
    }

    if (req.method === "POST" && path === "/tenants/v1/networks") {
      const body = await readJsonBody(req);
      sendJson(res, 200, {
        id: randomUUID(),
        slug: body.slug ?? "e2e-network",
        region: body.region ?? "use1",
        provisioning_state: "active",
      });
      return;
    }

    if (
      req.method === "DELETE" &&
      /^\/tenants\/v1\/networks\/[^/]+\/daemons\/self$/.test(path)
    ) {
      res.writeHead(204);
      res.end();
      return;
    }

    if (req.method === "PUT" && path === "/ddns/v1/ip") {
      res.writeHead(204);
      res.end();
      return;
    }

    if (
      (req.method === "PUT" || req.method === "DELETE") &&
      path === "/ddns/v1/acme-challenge"
    ) {
      res.writeHead(204);
      res.end();
      return;
    }

    sendJson(res, 404, { errors: { message: `no mock route for ${req.method} ${path}` } });
  } catch (err) {
    console.error(`wardnet_cloud_mock: error handling ${req.method} ${path}:`, err);
    sendJson(res, 500, { errors: { message: "internal mock error" } });
  }
});

server.listen(PORT, () => {
  console.log(`wardnet_cloud_mock listening on :${PORT}`);
});
