import { describe, expect, it } from "vitest";
import { WardnetClient } from "../src/client.js";
import { UserService } from "../src/services/users.js";
import { DeviceService } from "../src/services/devices.js";

/**
 * Pin what `UserService` puts on the wire (ADR-0031).
 *
 * The service's whole job is mapping a friendly surface onto the daemon's
 * paths and snake_case bodies, so these assert the transport call rather than
 * re-testing the daemon. `request` is the single funnel every call goes
 * through — capturing it is enough to see exactly what would be sent.
 */
function recording(response: unknown = {}): {
  client: WardnetClient;
  calls: Array<{ path: string; method: string; body?: unknown }>;
} {
  const calls: Array<{ path: string; method: string; body?: unknown }> = [];
  const client = new WardnetClient();
  client.request = async <T>(path: string, init?: RequestInit): Promise<T> => {
    calls.push({
      path,
      method: init?.method ?? "GET",
      body: init?.body ? JSON.parse(init.body as string) : undefined,
    });
    return response as T;
  };
  return { client, calls };
}

describe("UserService", () => {
  it("unwraps the directory list", async () => {
    const { client } = recording({ users: [{ id: "u1", display_name: "Ana" }] });
    const users = await new UserService(client).list();
    expect(users).toHaveLength(1);
    expect(users[0].display_name).toBe("Ana");
  });

  it("sends null for an omitted email rather than an empty string", async () => {
    // The column is uniquely indexed, so a run of empty strings would collide
    // on the second user — "no email" has to be null.
    const { client, calls } = recording({});
    await new UserService(client).create({
      display_name: "Cleo",
      role: "member",
    });
    expect(calls[0]).toMatchObject({ path: "/users", method: "POST" });
    expect(calls[0].body).toEqual({
      display_name: "Cleo",
      email: null,
      role: "member",
    });
  });

  it("replaces both profile fields, clearing an omitted email", async () => {
    const { client, calls } = recording({});
    await new UserService(client).updateProfile("u1", {
      display_name: "Renamed",
    });
    expect(calls[0].method).toBe("PATCH");
    expect(calls[0].body).toEqual({ display_name: "Renamed", email: null });
  });

  it("routes the enable, role, delete and unlink calls", async () => {
    const { client, calls } = recording({});
    const svc = new UserService(client);
    await svc.setEnabled("u1", false);
    await svc.setRole("u1", "admin");
    await svc.delete("u1");
    await svc.unlinkOauth("u1", "google");

    expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
      "PUT /users/u1/enabled",
      "PUT /users/u1/role",
      "DELETE /users/u1",
      "DELETE /users/u1/credentials/google",
    ]);
  });

  it("maps the camelCase password arguments onto the snake_case wire", async () => {
    const { client, calls } = recording({});
    await new UserService(client).changeOwnPassword("old-one", "new-one");
    expect(calls[0].path).toBe("/users/me/password");
    expect(calls[0].body).toEqual({
      current_password: "old-one",
      new_password: "new-one",
    });
  });

  it("unwraps credentials and enrolments", async () => {
    const { client } = recording({
      credentials: [{ id: "c1", kind: "password" }],
    });
    expect(await new UserService(client).listCredentials("u1")).toHaveLength(1);

    const { client: c2 } = recording({ enrolments: [{ id: "e1" }, { id: "e2" }] });
    expect(await new UserService(c2).listEnrolments("u1")).toHaveLength(2);
  });

  it("returns the one-time invitation intact", async () => {
    // Present exactly once in this response; the service must not drop it.
    const { client, calls } = recording({
      token: "one-time",
      expires_at: "2026-08-04T00:00:00Z",
      user_id: "u1",
    });
    const invite = await new UserService(client).issueEnrolment("u1");
    expect(calls[0]).toMatchObject({
      path: "/users/u1/enrolments",
      method: "POST",
    });
    expect(invite.token).toBe("one-time");
  });

  it("scopes a revoke to the owning user", async () => {
    // The path asserts an ownership relation the daemon now checks; sending a
    // bare enrolment id would let a wrong user id revoke the right invitation.
    const { client, calls } = recording({});
    await new UserService(client).revokeEnrolment("u1", "e9");
    expect(calls[0].path).toBe("/users/u1/enrolments/e9");
  });

  it("redeems an invitation unauthenticated", async () => {
    const { client, calls } = recording({ display_name: "Bruno" });
    const user = await new UserService(client).redeemEnrolment("tok", "pw");
    expect(calls[0]).toMatchObject({
      path: "/auth/enrolments/redeem",
      method: "POST",
    });
    expect(calls[0].body).toEqual({ token: "tok", password: "pw" });
    expect(user.display_name).toBe("Bruno");
  });

  it("reads the public sign-in contract", async () => {
    const { client, calls } = recording({ password: true, providers: [] });
    const methods = await new UserService(client).availableMethods();
    expect(calls[0].path).toBe("/auth/methods");
    expect(methods.password).toBe(true);
  });

  it("reads the admin provider list from its own endpoint", async () => {
    // Distinct from `availableMethods`: that one is unauthenticated and
    // carries neither client id nor redirect URI.
    const { client, calls } = recording({
      providers: [{ provider: "google", client_id: "id" }],
    });
    const providers = await new UserService(client).listOauthProviders();
    expect(calls[0].path).toBe("/auth/providers");
    expect(providers[0].client_id).toBe("id");
  });

  it("carries the caller's intent into the ceremony and returns the URL", async () => {
    const { client, calls } = recording({ url: "https://accounts.google.com/x" });
    const url = await new UserService(client).startOauth("google", {
      returnTo: "admin_app",
      rememberMe: true,
    });
    expect(calls[0].path).toBe("/auth/oauth/google/start?return_to=admin_app&remember_me=true");
    expect(url).toBe("https://accounts.google.com/x");
  });

  it("defaults the ceremony to the desktop admin and a short session", async () => {
    // `remember_me` defaults to false: it gates session refresh, so the safe
    // default is the short-lived session.
    const { client, calls } = recording({ url: "https://accounts.google.com/x" });
    await new UserService(client).startOauth("google");
    expect(calls[0].path).toContain("return_to=admin");
    expect(calls[0].path).toContain("remember_me=false");
  });

  it("sends null for an omitted client secret so the stored one survives", async () => {
    const { client, calls } = recording({});
    await new UserService(client).configureOauthProvider("google", {
      client_id: "the-id",
      enabled: true,
    });
    expect(calls[0].body).toEqual({
      client_id: "the-id",
      client_secret: null,
      enabled: true,
    });
  });

  it("clears a provider", async () => {
    const { client, calls } = recording({});
    await new UserService(client).clearOauthProvider("github");
    expect(calls[0]).toMatchObject({
      path: "/auth/providers/github",
      method: "DELETE",
    });
  });
});

describe("DeviceService.setOwner", () => {
  it("assigns an owner", async () => {
    const { client, calls } = recording({ device: {}, current_rule: null });
    await new DeviceService(client).setOwner("d1", "u1");
    expect(calls[0]).toMatchObject({
      path: "/devices/d1/owner",
      method: "PUT",
    });
    expect(calls[0].body).toEqual({ owner_user_id: "u1" });
  });

  it("clears an owner with an explicit null", async () => {
    const { client, calls } = recording({ device: {}, current_rule: null });
    await new DeviceService(client).setOwner("d1", null);
    expect(calls[0].body).toEqual({ owner_user_id: null });
  });
});
