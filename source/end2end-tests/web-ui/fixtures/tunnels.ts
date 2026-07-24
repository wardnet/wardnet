/**
 * Tunnel seeding + cleanup helpers for the admin-site Tunnels & Providers
 * specs (A7, #622).
 *
 * `tunnels.spec.ts` creates a tunnel through the UI (real coverage of the
 * `.conf` paste-import flow) and `providers.spec.ts` creates one through the
 * NordVPN wizard, so the only seeding done here is *cleanup* plus a
 * deterministic API import for the list-page delete case. Tunnel labels are
 * NOT unique in the daemon schema (unlike filter-profile names), so a re-run
 * against the persisted state volume wouldn't collide — but leftover
 * `e2e-tunnel-*` rows would accumulate and make list assertions ambiguous.
 * `deleteTestTunnels` removes every row matching the given labels up front.
 *
 * Uses the same plain-`fetch` `api`/`ensureAdminSetup` path as the other
 * fixtures (see seed.ts for why this harness avoids the source-only SDK).
 */

import { API_BASE_URL, api, ensureAdminSetup } from "./seed";

/** Label the import→detail→delete lifecycle test creates through the UI. */
export const TUNNEL_IMPORT_LABEL = "e2e-tunnel-import";

/** Label the admin-site speed-test spec owns (comparison + detail history). */
export const TUNNEL_SPEED_TEST_LABEL = "e2e-tunnel-speed";

/** Label the admin-app speed-test spec owns. Distinct from the admin-site
 *  label so the two specs never see each other's card in the shared daemon. */
export const TUNNEL_SPEED_TEST_APP_LABEL = "e2e-tunnel-speed-app";

/**
 * Label the list-page delete test seeds via the API then deletes through the
 * ConfirmDialog. Distinct from `TUNNEL_IMPORT_LABEL` so the two tunnel tests
 * never see each other's rows in the shared daemon.
 */
export const TUNNEL_LIST_DELETE_LABEL = "e2e-tunnel-list-delete";

/** Label the NordVPN provider wizard test's tunnel is created under. */
export const TUNNEL_PROVIDER_LABEL = "e2e-tunnel-nordvpn";

/**
 * A deterministic, syntactically-valid WireGuard config. The daemon's parser
 * (`wardnet-common/src/wireguard_config.rs`) only requires `[Interface]
 * PrivateKey` and `[Peer] PublicKey` and does not validate the key material,
 * so these throwaway base64 blobs are fine — import only parses and persists
 * the definition (status `Down`, no interface brought up), so it has no
 * WireGuard kernel dependency in the compose stack.
 */
export const TUNNEL_CONFIG = [
  "[Interface]",
  "PrivateKey = SHFlsItPbjj4u4nNZbR8Ej2cTSDDTNeWiR+ej8a4tEM=",
  "Address = 10.99.0.2/32",
  "DNS = 1.1.1.1",
  "",
  "[Peer]",
  "PublicKey = HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=",
  "Endpoint = 198.51.100.1:51820",
  "AllowedIPs = 0.0.0.0/0",
  "",
].join("\n");

export interface TunnelSummary {
  id: string;
  label: string;
}

export interface ListTunnelsResponse {
  tunnels: TunnelSummary[];
}

/**
 * Delete every tunnel whose label matches one of `labels`, so a spec starts
 * from a clean slate. Idempotent: a missing tunnel (already deleted) is a
 * no-op. Matching is by label because these specs own their labels and never
 * know the daemon-assigned id up front; all matches are removed since labels
 * are not unique.
 */
export async function deleteTestTunnels(...labels: string[]): Promise<void> {
  const token = await ensureAdminSetup();
  const wanted = new Set(labels);
  const { tunnels } = await api<ListTunnelsResponse>("/tunnels", { token });
  for (const t of tunnels) {
    if (wanted.has(t.label)) {
      await api(`/tunnels/${t.id}`, { method: "DELETE", token });
    }
  }
}

/**
 * Import a tunnel via the API so a spec has an existing row to act on (used by
 * the list-page delete test). Always POSTs a fresh row (the caller is expected
 * to clean up its label in `beforeAll`); for an idempotent skip-if-present
 * variant see `seedTunnel` in ./devices. Returns the label so the spec can
 * locate the card by its accessible text.
 */
export async function importTunnel(
  label: string,
  countryCode = "de",
): Promise<string> {
  const token = await ensureAdminSetup();
  await api("/tunnels", {
    method: "POST",
    token,
    body: JSON.stringify({
      label,
      country_code: countryCode,
      config: TUNNEL_CONFIG,
    }),
  });
  return label;
}

/**
 * Import a tunnel and return its daemon-assigned id. The speed-test specs need
 * the id to drive `POST /tunnels/{id}/speed-test` directly (the concurrency
 * 409 case and history pre-seeding). Labels are not unique, so callers are
 * expected to `deleteTestTunnels(label)` first; this returns the id of the
 * (then only) matching row.
 */
export async function importTunnelReturningId(
  label: string,
  countryCode = "de",
): Promise<string> {
  await importTunnel(label, countryCode);
  const token = await ensureAdminSetup();
  const { tunnels } = await api<ListTunnelsResponse>("/tunnels", { token });
  const found = tunnels.find((t) => t.label === label);
  if (!found) {
    throw new Error(`imported tunnel '${label}' not found after create`);
  }
  return found.id;
}

interface JobDispatched {
  job_id: string;
}

interface JobState {
  status: "PENDING" | "RUNNING" | "SUCCEED" | "TERMINATED_WITH_ERRORS";
}

/**
 * Dispatch a speed test and return the raw HTTP status without throwing, so a
 * spec can assert the 409 a concurrent run produces. (`api()` throws on
 * non-2xx, which would mask the status.)
 */
export async function dispatchSpeedTest(
  token: string,
  tunnelId: string,
): Promise<number> {
  const res = await fetch(`${API_BASE_URL}/tunnels/${tunnelId}/speed-test`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
  });
  await res.text(); // Drain the body so the socket is released.
  return res.status;
}

/**
 * Run a speed test to completion via the API: dispatch the job, then poll it
 * until terminal. Used to pre-seed history rows (e.g. the admin-app expand
 * case needs a prior run so the latest result has something to expand into).
 * Throws if the job errors or does not finish in time.
 */
export async function runSpeedTest(
  token: string,
  tunnelId: string,
): Promise<void> {
  const { job_id } = await api<JobDispatched>(
    `/tunnels/${tunnelId}/speed-test`,
    { method: "POST", token },
  );

  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const job = await api<JobState>(`/jobs/${job_id}`, { token });
    // Any status other than the two in-flight ones is terminal; surface it
    // directly rather than polling to the deadline on an unexpected state.
    if (job.status === "PENDING" || job.status === "RUNNING") {
      await new Promise((resolve) => setTimeout(resolve, 300));
      continue;
    }
    if (job.status === "SUCCEED") return;
    throw new Error(`speed test job ${job_id} ended in ${job.status}`);
  }
  throw new Error(`speed test job ${job_id} did not complete within 30s`);
}
