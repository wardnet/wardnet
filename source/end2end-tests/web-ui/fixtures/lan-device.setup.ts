import { request, test as setup } from "@playwright/test";

import {
  API_BASE_URL,
  LAN_PROXY_IP,
  UI_LAN_BASE_URL,
  api,
  ensureAdminSetup,
  waitForReady,
} from "./seed.js";

/**
 * LAN-device bootstrap for the user-app project (C1, #626) — runs before
 * the user-app specs via Playwright `dependencies`.
 *
 * The user-app is device-keyed: the daemon classifies `GET /api/devices/me`
 * by the request's source IP (`middleware.rs::resolve_auth_context` — raw
 * TCP peer, no `X-Forwarded-For` trust). Requests reach the daemon through
 * the LAN-side TLS proxy (`tls_proxy_lan`), so the daemon sees the proxy's
 * fixed LAN IP (`LAN_PROXY_IP`). For `devices/me` to resolve non-null, that
 * IP must be a *discovered* device.
 *
 * There is no create-device API — the daemon only materializes a device row
 * when packet-capture discovery observes LAN traffic for a MAC in-subnet
 * (`device/discovery.rs`). So instead of spoofing an ARP frame at the
 * proxy's IP (which it actively uses — that would risk ARP conflicts), we
 * make the proxy itself talk to the daemon: a throwaway `GET /api/info`
 * through the LAN proxy forces a proxy→daemon connection over wardnet_lan,
 * which discovery observes (the proxy's real MAC at `LAN_PROXY_IP`). We
 * re-poke and poll `/api/devices` until that device appears.
 *
 * The proxy request uses a Playwright request context with
 * `ignoreHTTPSErrors` so the self-signed `tls_proxy_lan` cert is trusted
 * (Node's bare `fetch`, used elsewhere here against plain HTTP, can't).
 */

interface SeededDevice {
  id: string;
  mac: string;
  last_ip: string;
}

interface ListDevicesResponse {
  devices: SeededDevice[];
}

setup("seed LAN-proxy device", async () => {
  await waitForReady();
  const token = await ensureAdminSetup();

  // Trust the self-signed LAN proxy so the warm-up request goes through.
  const proxy = await request.newContext({ ignoreHTTPSErrors: true });
  try {
    const found = async (): Promise<SeededDevice | undefined> => {
      const { devices } = await api<ListDevicesResponse>("/devices", { token });
      return devices.find((d) => d.last_ip === LAN_PROXY_IP);
    };

    const existing = await found();
    if (existing) return;

    const timeoutMs = 60_000;
    const deadline = Date.now() + timeoutMs;
    let lastErr: unknown;
    while (Date.now() < deadline) {
      try {
        // Forces tls_proxy_lan → daemon traffic over wardnet_lan, which
        // packet-capture discovery observes. The response itself is
        // irrelevant; we only need the connection to happen.
        await proxy.get(`${UI_LAN_BASE_URL}/api/info`);
        if (await found()) return;
      } catch (err) {
        lastErr = err;
      }
      await new Promise((resolve) => setTimeout(resolve, 3_000));
    }
    throw new Error(
      `LAN proxy device ${LAN_PROXY_IP} was not discovered within ${timeoutMs}ms ` +
        `(${API_BASE_URL}) — packet capture may not be reaching wardnet_lan in ` +
        `this environment${lastErr ? `: ${String(lastErr)}` : ""}`,
    );
  } finally {
    await proxy.dispose();
  }
});
