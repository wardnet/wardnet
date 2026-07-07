// wardnet_cloud_mock — a minimal, dependency-free HTTP server that stands in
// for the wardnet-cloud `tenants` (global) and `ddns` (regional) gateways the
// daemon's wardnet DDNS provider talks to. Lets the admin-app/user-app e2e
// projects walk the real enroll → register flow offline, so
// `Entitlement::is_entitled()` flips true and the premium-gated PWAs render.
// Mirrors ../../daemon/mocks/nordvpn/server.mjs in style.
//
// Serves BOTH the `tenants` and `ddns` gateways on one port: the daemon is
// pointed at this single mock for its global gateway, region gateway, AND
// region health-probe URL (see compose.ui.yaml's `[ddns_wardnet]` injection),
// since there is only one region in the e2e harness and nothing here needs
// them to be separate hosts.
//
// Paths are prefix-free `/v1/...` (cloud ADR-0015 / daemon commit bc46ae0,
// #793): the real gateway selects the target service from an `X-Mesh-Target`
// header it derives per service, not from a path prefix, so there is no
// `/tenants`/`/ddns` segment to match here. No auth/signature verification —
// the daemon's `PoP` signing (cloud/pop.rs) is a cloud-side concern; this mock
// only exists to make the daemon's own premium gate flip, so every request
// unconditionally succeeds.
//
// Endpoints:
//   GET    /health                        liveness probe (200 "ok")
//   GET    /readyz                        region readiness probe (200 "ok"; cloud ADR-0027)
//   POST   /v1/verification-codes         -> 204
//   POST   /v1/enroll                     -> 200 {tenant_id}
//   POST   /v1/token                      -> 200 {token}
//   GET    /v1/availability               -> 200 {available: true}
//   POST   /v1/networks                   -> 200 {id, slug, region, provisioning_state}
//   DELETE /v1/networks/:id/daemons/self  -> 204
//   PUT    /v1/ip                         -> 204 (best-effort background publish)
//   PUT    /v1/acme-challenge             -> 204 (best-effort background publish)
//   DELETE /v1/acme-challenge             -> 204

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

  try {
    if (req.method === "GET" && path === "/health") {
      res.writeHead(200, { "content-type": "text/plain" });
      res.end("ok");
      return;
    }

    if (req.method === "GET" && path === "/readyz") {
      res.writeHead(200, { "content-type": "text/plain" });
      res.end("ok");
      return;
    }

    if (req.method === "POST" && path === "/v1/verification-codes") {
      res.writeHead(204);
      res.end();
      return;
    }

    if (req.method === "POST" && path === "/v1/enroll") {
      sendJson(res, 200, { tenant_id: `e2e-tenant-${randomUUID()}` });
      return;
    }

    if (req.method === "POST" && path === "/v1/token") {
      sendJson(res, 200, { token: `e2e-mock-jwt-${randomUUID()}` });
      return;
    }

    if (req.method === "GET" && path === "/v1/availability") {
      sendJson(res, 200, { available: true });
      return;
    }

    if (req.method === "POST" && path === "/v1/networks") {
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
      /^\/v1\/networks\/[^/]+\/daemons\/self$/.test(path)
    ) {
      res.writeHead(204);
      res.end();
      return;
    }

    if (req.method === "PUT" && path === "/v1/ip") {
      res.writeHead(204);
      res.end();
      return;
    }

    if (
      (req.method === "PUT" || req.method === "DELETE") &&
      path === "/v1/acme-challenge"
    ) {
      res.writeHead(204);
      res.end();
      return;
    }

    sendJson(res, 404, {
      errors: { message: `no mock route for ${req.method} ${path}` },
    });
  } catch (err) {
    console.error(`wardnet_cloud_mock: error handling ${req.method} ${path}:`, err);
    sendJson(res, 500, { errors: { message: "internal mock error" } });
  }
});

server.listen(PORT, () => {
  console.log(`wardnet_cloud_mock listening on :${PORT}`);
});
