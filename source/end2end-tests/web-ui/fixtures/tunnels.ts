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

import { api, ensureAdminSetup } from "./seed";

/** Label the import→detail→delete lifecycle test creates through the UI. */
export const TUNNEL_IMPORT_LABEL = "e2e-tunnel-import";

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
