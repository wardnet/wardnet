import { expect, test } from "@playwright/test";

import { TUNNEL_PROVIDER_LABEL, deleteTestTunnels } from "../../fixtures/tunnels.js";

/**
 * Admin-site VPN-provider wizard coverage (A7, #622).
 *
 * Drives the "Provider" tab of the add-tunnel panel end to end against the
 * NordVPN provider, backed by the `nordvpn_mock` compose service (#248) which
 * `compose.ui.yaml` wires into this stack. The mock is a firm invariant here:
 * these tests HARD-FAIL (not skip) if the provider can't be reached — the
 * runner gates on `nordvpn_mock` being healthy.
 *
 * NordVPN is token-only (`auth_methods: [Token]`), so the wizard shows the
 * access-token field directly with no auth-method selector. The mock's
 * `/v1/servers/recommendations` returns its server fixture regardless of
 * country, so the happy path can create a tunnel without selecting a
 * country/server — the daemon auto-selects the lowest-load server.
 *
 * Runs in the `admin-site` project (seeded daemon + admin storageState).
 */

/** The mock accepts exactly this token (NORDVPN_VALID_TOKEN default). */
const VALID_TOKEN = "valid-nordvpn-token";

test.beforeAll(async () => {
  await deleteTestTunnels(TUNNEL_PROVIDER_LABEL);
});

// Unlike tunnels.spec (which deletes everything it creates), the happy-path
// test here leaves a tunnel behind. Clean it up so it can't leak into other
// admin-site specs sharing this daemon.
test.afterAll(async () => {
  await deleteTestTunnels(TUNNEL_PROVIDER_LABEL);
});

test("provider: NordVPN wizard validates token and creates a tunnel", async ({
  page,
}) => {
  await page.goto("./tunnels");
  await page.getByTestId("tunnel-add").click();
  await page.getByTestId("tunnel-tab-provider").click();

  // ── Step 1: select the NordVPN provider ────────────────────────────
  await page.getByTestId("provider-select").click();
  await page.getByRole("option", { name: "NordVPN", exact: true }).click();

  // ── Step 2: validate the access token against the mock ──────────────
  await page.getByTestId("provider-token").fill(VALID_TOKEN);
  await page.getByTestId("provider-validate").click();

  // Validation success reveals the country/server + label step.
  const labelField = page.getByTestId("provider-label");
  await expect(labelField).toBeVisible();
  await labelField.fill(TUNNEL_PROVIDER_LABEL);

  // ── Step 3: create the tunnel (auto-select best server) ─────────────
  await page.getByTestId("provider-setup-submit").click();

  // ── It appears in the list attributed to our label ─────────────────
  await expect(
    page.getByTestId("tunnel-card").filter({ hasText: TUNNEL_PROVIDER_LABEL }),
  ).toBeVisible();
});

test("provider: NordVPN wizard rejects an invalid token", async ({ page }) => {
  await page.goto("./tunnels");
  await page.getByTestId("tunnel-add").click();
  await page.getByTestId("tunnel-tab-provider").click();

  await page.getByTestId("provider-select").click();
  await page.getByRole("option", { name: "NordVPN", exact: true }).click();

  await page.getByTestId("provider-token").fill("not-a-valid-token");
  await page.getByTestId("provider-validate").click();

  // The daemon reports the credentials as invalid (mock 401 → valid:false).
  await expect(page.getByText("credentials are invalid")).toBeVisible();
  // The country/server + setup step stays hidden while creds are invalid.
  await expect(page.getByTestId("provider-setup-submit")).toHaveCount(0);
});
