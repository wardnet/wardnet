import { describe, it, expect, beforeAll } from "vitest";
import { InfoService, TunnelService, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  ensureAdminAndLogin,
  waitForReady,
} from "./helpers.js";

describe("daemon smoke", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });

  beforeAll(async () => {
    await waitForReady(client);
  }, 120_000);

  it("serves /api/info before setup", async () => {
    const info = await new InfoService(client).getInfo();
    expect(info.version).toBeTruthy();
  });

  it("completes the setup wizard and lists zero tunnels", async () => {
    const authed = await ensureAdminAndLogin(client);
    const { tunnels } = await new TunnelService(authed).list();
    expect(tunnels).toEqual([]);
  });
});
