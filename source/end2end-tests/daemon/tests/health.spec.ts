import { randomBytes } from "node:crypto";

import { describe, it, expect, beforeAll } from "vitest";
import {
  WardnetClient,
  AuthService,
  InfoService,
  SetupService,
  TunnelService,
} from "@wardnet/js";

// The daemon container publishes its API on the wardnet_mgmt bridge.
// Compose's DNS resolves the service name to the container's mgmt IP.
const API_BASE_URL = process.env.WARDNET_API_BASE_URL ?? "http://wardnetd:7411/api";

// Setup-wizard credentials. Generated per-run rather than checked in
// so a leaked log line can't be replayed against a real instance.
// `randomBytes` (vs `Math.random`) keeps CodeQL's
// js/insecure-randomness rule happy -- the credential is test-only
// and never leaves the compose stack, but the rule fires on shape,
// not reachability.
const ADMIN_USERNAME = "admin";
const ADMIN_PASSWORD = `e2e-${randomBytes(6).toString("hex")}`;

/**
 * WardnetClient that re-attaches the bearer token returned by the login
 * endpoint to every subsequent request. Node's fetch has no cookie jar,
 * so the session cookie the daemon sets is invisible to follow-up calls;
 * `Authorization: Bearer <token>` is the documented non-browser path.
 */
class AuthedClient extends WardnetClient {
  constructor(
    baseUrl: string,
    private readonly token: string,
  ) {
    super({ baseUrl });
  }

  override async request<T>(path: string, init?: RequestInit): Promise<T> {
    const headers = new Headers(init?.headers);
    headers.set("Content-Type", "application/json");
    headers.set("Authorization", `Bearer ${this.token}`);
    return super.request<T>(path, { ...init, headers });
  }
}

async function waitForReady(client: WardnetClient, timeoutMs = 90_000): Promise<void> {
  const info = new InfoService(client);
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;
  while (Date.now() < deadline) {
    try {
      await info.getInfo();
      return;
    } catch (err) {
      lastError = err;
      await new Promise((resolve) => setTimeout(resolve, 1_000));
    }
  }
  throw new Error(
    `daemon did not become ready within ${timeoutMs}ms: ${String(lastError)}`,
  );
}

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
    const setup = new SetupService(client);

    const status = await setup.getStatus();
    if (!status.setup_completed) {
      await setup.setup({ username: ADMIN_USERNAME, password: ADMIN_PASSWORD });
    }

    const login = await new AuthService(client).login({
      username: ADMIN_USERNAME,
      password: ADMIN_PASSWORD,
    });
    expect(login.token).toBeTruthy();

    const authed = new AuthedClient(API_BASE_URL, login.token);
    const { tunnels } = await new TunnelService(authed).list();
    expect(tunnels).toEqual([]);
  });
});
