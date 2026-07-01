import { expect, test } from "@playwright/test";

import {
  enableDnsAndQueryLog,
  resolveViaAgent,
  waitForQueryLog,
} from "../../fixtures/dns-logs.js";

/**
 * Admin-site DNS query-log coverage (A5, #620) for `/admin/dns/logs`.
 *
 * The query log is populated by real resolution, so both paths are seeded by
 * driving DNS queries through the `test_debian` LAN client against the daemon
 * (see fixtures/dns-logs.ts):
 *  - **live tail** — with the stream connected, a freshly driven query must
 *    appear in the table in real time (WebSocket coverage);
 *  - **history** — with live tail off, the paginated table renders persisted
 *    queries, narrows by the domain search, and honours the result filter.
 *
 * Runs in the `admin-site` project (seeded daemon + admin storageState).
 */

// Query logging (and the DNS server) must be on for resolves to be recorded.
test.beforeAll(async () => {
  await enableDnsAndQueryLog();
});

test("query log: a live query streams into the table", async ({ page }) => {
  // Unique label so the assertion can't match an unrelated pre-existing row.
  const label = `a5live${Date.now()}`;
  const domain = `${label}.example.org`;

  await page.goto("./dns/logs");
  await expect(page.getByTestId("page-title")).toHaveText("DNS query log");

  // Live tail is the default view; wait for the stream to connect (the label
  // drops its "(offline)" suffix) before driving a query into it.
  await expect(page.getByTestId("live-tail")).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(page.getByText("(offline)")).toHaveCount(0, { timeout: 15_000 });

  await resolveViaAgent(domain);

  await expect(page.getByRole("row").filter({ hasText: label })).toBeVisible({
    timeout: 15_000,
  });
});

test("query log: history table filters by domain and result", async ({
  page,
}) => {
  const label = `a5hist${Date.now()}`;
  const domain = `${label}.example.org`;

  // Seed a persisted entry and wait until the daemon has written it, so the
  // history query below isn't racing the log write path.
  await resolveViaAgent(domain);
  await waitForQueryLog(label);

  await page.goto("./dns/logs");

  // Switch to the historical (paginated) view.
  const liveTail = page.getByTestId("live-tail");
  await liveTail.click();
  await expect(liveTail).toHaveAttribute("aria-checked", "false");

  // Domain search narrows the history to our seeded query.
  await page.getByPlaceholder("Search domain…").fill(label);
  const row = page.getByRole("row").filter({ hasText: label });
  await expect(row).toBeVisible();

  // Result filter: the seeded query was never "blocked", so filtering by
  // Blocked hides it; resetting to Any brings it back. Deterministic
  // regardless of whether the upstream answered or NXDOMAIN'd.
  await page.getByTestId("dns-log-result-filter").click();
  await page.getByRole("option", { name: "Blocked", exact: true }).click();
  await expect(row).toHaveCount(0);

  await page.getByTestId("dns-log-result-filter").click();
  await page.getByRole("option", { name: "Any result", exact: true }).click();
  await expect(row).toBeVisible();
});
