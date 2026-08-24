import { expect, test } from "@playwright/test";

import { ensureAdminSetup } from "../../fixtures/seed.js";
import {
  TUNNEL_SPEED_TEST_LABEL,
  deleteTestTunnels,
  dispatchSpeedTest,
  importTunnel,
  importTunnelReturningId,
} from "../../fixtures/tunnels.js";

/**
 * Admin-site per-tunnel speed test coverage (#691, follow-up to #239).
 *
 * The daemon under test is the real binary, but the compose stack runs it with
 * `[test] stub_tunnel_backends = true`, so the throughput tester, latency
 * prober and (no-op) WireGuard interface return deterministic numbers: the
 * tunnel keeps 90% of a 100 Mbps line at double the 10 ms direct latency. That
 * lets these specs assert the rendered comparison exactly, which a real
 * download through a live tunnel (neither of which exists in CI) never could.
 *
 * Runs in the `admin-site` project (seeded daemon + admin storageState).
 */

// A second label, owned only by the concurrency test, so its rapid-fire runs
// never collide with the card the UI test acts on.
const TUNNEL_SPEED_TEST_409_LABEL = "e2e-tunnel-speed-409";

test.beforeAll(async () => {
  await deleteTestTunnels(TUNNEL_SPEED_TEST_LABEL, TUNNEL_SPEED_TEST_409_LABEL);
});

test.afterAll(async () => {
  await deleteTestTunnels(TUNNEL_SPEED_TEST_LABEL, TUNNEL_SPEED_TEST_409_LABEL);
});

test("speed test: run from the card, see the direct/tunnel comparison and detail history", async ({
  page,
}) => {
  await importTunnel(TUNNEL_SPEED_TEST_LABEL);

  await page.goto("./tunnels");
  const card = page
    .getByTestId("tunnel-card")
    .filter({ hasText: TUNNEL_SPEED_TEST_LABEL });
  await expect(card).toBeVisible();

  // ── Run the test ───────────────────────────────────────────────────
  const runButton = card.getByTestId("tunnel-speed-test-button");
  await expect(runButton).toContainText("Speed test");
  await runButton.click();

  // ── The inline comparison renders the deterministic stub values ────
  const result = card.getByTestId("tunnel-speed-test-result");
  await expect(result).toBeVisible({ timeout: 20_000 });
  await expect(card.getByTestId("tunnel-speed-test-download")).toHaveText(
    "90.0 Mbps",
  );
  await expect(card.getByTestId("tunnel-speed-test-latency")).toHaveText(
    "20 ms",
  );
  // 90 Mbps tunnel over a 100 Mbps direct baseline → 90% retained.
  await expect(result).toContainText("90% kept");
  await expect(result).toContainText("Direct");
  await expect(result).toContainText("Tunnel");

  // ── Detail page shows the run in the history table ─────────────────
  await card.getByRole("link").click();
  await expect(page).toHaveURL(/\/admin\/tunnels\/[0-9a-f-]+$/);
  const history = page.getByTestId("tunnel-speed-test-history");
  await expect(history).toBeVisible();
  await expect(
    history.getByRole("heading", { name: "Speed test history" }),
  ).toBeVisible();
  // tunnel / direct throughput and latency, plus retained percentage.
  await expect(history).toContainText("90.0 / 100.0 Mbps");
  await expect(history).toContainText("20 / 10 ms");
  await expect(history).toContainText("90%");
});

test("speed test: a concurrent run on the same tunnel returns 409", async () => {
  const token = await ensureAdminSetup();
  const id = await importTunnelReturningId(TUNNEL_SPEED_TEST_409_LABEL);

  // Fire two runs at once. The service claims a shared in-flight slot
  // synchronously before dispatching the job and holds it for the whole run,
  // so exactly one request wins (202 Accepted) and the other is rejected
  // (409 Conflict).
  const statuses = await Promise.all([
    dispatchSpeedTest(token, id),
    dispatchSpeedTest(token, id),
  ]);

  expect(statuses.sort()).toEqual([202, 409]);
});
