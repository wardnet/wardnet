import { afterEach, describe, expect, it, vi } from "vitest";
import { AuthService, WardnetClient } from "@wardnet/js";

/**
 * Exercises the real SDK `AuthService` against a stubbed `fetch`, so the
 * request line each method produces is pinned down — the store tests mock
 * the service away, which would let a wrong path or verb slip through.
 */
describe("AuthService", () => {
  const fetchMock = vi.fn();
  vi.stubGlobal("fetch", fetchMock);

  afterEach(() => {
    fetchMock.mockReset();
  });

  it("logout POSTs to /auth/logout and resolves on 204", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }));
    const service = new AuthService(new WardnetClient({ baseUrl: "/api" }));

    await expect(service.logout()).resolves.toBeUndefined();

    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("/api/auth/logout");
    expect(init.method).toBe("POST");
    // The session cookie is the credential — it must ride along.
    expect(init.credentials).toBe("include");
    // Sign-out flows gate navigation on this call, so it must be time-boxed
    // rather than riding the browser's connection timeout.
    expect(init.signal).toBeInstanceOf(AbortSignal);
  });

  it("logout rejects when the daemon reports an error", async () => {
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify({ error: "unauthorized" }), { status: 401 }),
    );
    const service = new AuthService(new WardnetClient({ baseUrl: "/api" }));

    await expect(service.logout()).rejects.toMatchObject({ status: 401 });
  });
});
