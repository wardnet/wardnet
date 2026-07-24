import { describe, expect, it } from "vitest";
import { WardnetClient } from "../src/client.js";
import { apiClient } from "../src/internal/client.js";

/**
 * Record the transport calls the internal client makes. The typed client is an
 * implementation detail, so these tests pin its runtime behaviour — path
 * substitution, query serialization, and body encoding — independent of the
 * per-endpoint types the generated schema drives.
 */
function recordingClient(): {
  client: WardnetClient;
  calls: Array<{ path: string; init?: RequestInit }>;
} {
  const calls: Array<{ path: string; init?: RequestInit }> = [];
  const client = new WardnetClient();
  // Stub the transport: every service call funnels through `request`, so
  // capturing it is enough to assert what would go on the wire.
  client.request = async <T>(path: string, init?: RequestInit): Promise<T> => {
    calls.push({ path, init });
    return undefined as T;
  };
  return { client, calls };
}

describe("internal ApiClient", () => {
  it("substitutes path parameters", async () => {
    const { client, calls } = recordingClient();
    await apiClient(client).get("/devices/{id}", { path: { id: "abc-123" } });
    expect(calls[0].path).toBe("/devices/abc-123");
  });

  it("url-encodes path parameter values", async () => {
    const { client, calls } = recordingClient();
    await apiClient(client).get("/devices/{id}", { path: { id: "a/b c" } });
    expect(calls[0].path).toBe("/devices/a%2Fb%20c");
  });

  it("throws when a required path parameter is missing", async () => {
    const { client } = recordingClient();
    // Cast around the types on purpose — the guard is a runtime backstop.
    await expect(
      apiClient(client).get("/devices/{id}", { path: {} as { id: string } }),
    ).rejects.toThrow(/Missing path parameter "id"/);
  });

  it("serializes scalar query parameters and drops null/undefined", async () => {
    const { client, calls } = recordingClient();
    await apiClient(client).get("/dns/log", {
      query: { limit: 5, domain: "example.com", result: undefined },
    });
    expect(calls[0].path).toBe("/dns/log?limit=5&domain=example.com");
  });

  it("serializes array query parameters as repeated keys", async () => {
    const { client, calls } = recordingClient();
    await apiClient(client).get("/stats", {
      query: { from: "1", to: "2", bucket: "minute", metrics: ["dns", "block"] },
    });
    expect(calls[0].path).toBe("/stats?from=1&to=2&bucket=minute&metrics=dns&metrics=block");
  });

  it("JSON-encodes the request body and sets the method", async () => {
    const { client, calls } = recordingClient();
    await apiClient(client).post("/auth/login", {
      body: { username: "admin", password: "pw", remember_me: true },
    });
    expect(calls[0].init?.method).toBe("POST");
    expect(calls[0].init?.body).toBe(
      JSON.stringify({ username: "admin", password: "pw", remember_me: true }),
    );
  });

  it("omits the body entirely for bodyless calls", async () => {
    const { client, calls } = recordingClient();
    await apiClient(client).post("/auth/refresh");
    expect(calls[0].init?.body).toBeUndefined();
  });

  it("forwards an abort signal", async () => {
    const { client, calls } = recordingClient();
    const controller = new AbortController();
    await apiClient(client).post("/auth/logout", { signal: controller.signal });
    expect(calls[0].init?.signal).toBe(controller.signal);
  });
});
