import { expect, test } from "@playwright/test";

import { ensureAdminSetup } from "../../fixtures/seed.js";
import {
  TUNNEL_SPEED_TEST_APP_LABEL,
  deleteTestTunnels,
  importTunnelReturningId,
  runSpeedTest,
} from "../../fixtures/tunnels.js";

/**
 * Admin-app (mobile PWA) per-tunnel speed test coverage (#691).
 *
 * The compose daemon runs with `[test] stub_tunnel_backends = true`, so the
 * speed test returns deterministic numbers (90 of 100 Mbps kept, 20 vs 10 ms).
 * The mobile card has no tunnel-detail route, so its last-runs history is a
 * tap-to-expand panel under the latest result rather than a table.
 *
 * Runs in the `admin-app` project (seeded daemon + admin storageState).
 */

test.beforeAll(async () => {
  await deleteTestTunnels(TUNNEL_SPEED_TEST_APP_LABEL);
});

test.afterAll(async () => {
  await deleteTestTunnels(TUNNEL_SPEED_TEST_APP_LABEL);
});

test("speed test: run from the card, then tap the latest result to expand history", async ({
  page,
}) => {
  const token = await ensureAdminSetup();
  const id = await importTunnelReturningId(TUNNEL_SPEED_TEST_APP_LABEL);
  // Seed two prior runs so the latest result has earlier runs to expand into
  // (the panel only reveals history when more than one result exists).
  await runSpeedTest(token, id);
  await runSpeedTest(token, id);

  await page.goto("./tunnels");
  const card = page
    .getByTestId("tunnel-card")
    .filter({ hasText: TUNNEL_SPEED_TEST_APP_LABEL });
  await expect(card).toBeVisible();

  // ── Run a test from the card ───────────────────────────────────────
  // The card only fetches its history once a run is requested, so this click
  // both exercises the button and surfaces the seeded results.
  await card.getByTestId("tunnel-speed-test-button").click();

  // ── The latest result renders the deterministic stub values ────────
  const result = card.getByTestId("tunnel-speed-test-result");
  await expect(result).toBeVisible({ timeout: 20_000 });
  await expect(result).toContainText("90.0 / 100.0 Mbps");
  await expect(result).toContainText("20 / 10 ms");
  await expect(result).toContainText("90% kept");

  // ── History is collapsed until the result is tapped ────────────────
  await expect(card.getByTestId("tunnel-speed-test-history")).toBeHidden();
  await result.getByRole("button").click();
  const history = card.getByTestId("tunnel-speed-test-history");
  await expect(history).toBeVisible();
  await expect(history).toContainText("90.0 / 100.0 Mbps");
});
