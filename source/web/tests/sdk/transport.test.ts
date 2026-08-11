import { afterEach, describe, expect, it, vi, type Mock } from "vitest";
import {
  BackupService,
  WardnetClient,
  AnomalyService,
  type Anomaly,
  type ListAnomaliesResponse,
  type UpdateTunnelDnsOverrideRequest,
  type UpdateTunnelDnsOverrideResponse,
} from "@wardnet/js";

/**
 * `fetch` call signature. Typing the mocks with it (rather than adding unused
 * parameters to their implementations) makes `mock.calls` a `[url, init?]`
 * tuple so the assertions below can read the outgoing URL and headers.
 */
type FetchFn = (url: string, init?: RequestInit) => Promise<Response>;

/**
 * Client that attaches a marker header through the `buildHeaders` seam — the
 * same override point the e2e `AuthedClient` uses for bearer auth. Every SDK
 * transport path (JSON `request`, plus backup's raw octet-stream/multipart
 * calls) must route through this seam, or a consumer's auth silently drops
 * for exactly the endpoints that bypass `request`.
 */
class HeaderStampedClient extends WardnetClient {
  protected override buildHeaders(init?: RequestInit): Record<string, string> {
    return { ...super.buildHeaders(init), "X-Test-Auth": "Bearer test-token" };
  }
}

/** Minimal `Response` stand-in covering only what the SDK reads. */
function fakeResponse(init: {
  ok?: boolean;
  status?: number;
  json?: unknown;
  blob?: Blob;
}): Response {
  return {
    ok: init.ok ?? true,
    status: init.status ?? 200,
    statusText: "OK",
    headers: new Headers(),
    json: async () => init.json,
    blob: async () => init.blob ?? new Blob([]),
  } as unknown as Response;
}

function lastFetchHeaders(mock: Mock<FetchFn>): Headers {
  const calls = mock.mock.calls;
  const init = calls[calls.length - 1]?.[1];
  return new Headers(init?.headers);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("WardnetClient transport seam", () => {
  it("routes BackupService.export through buildHeaders (auth header preserved)", async () => {
    const fetchMock = vi.fn<FetchFn>(async () =>
      fakeResponse({ blob: new Blob(["bundle-bytes"]) }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const backup = new BackupService(
      new HeaderStampedClient({ baseUrl: "/api" }),
    );
    const blob = await backup.export({ passphrase: "correct horse battery" });

    expect(blob).toBeInstanceOf(Blob);
    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/backup/export");
    const headers = lastFetchHeaders(fetchMock);
    expect(headers.get("X-Test-Auth")).toBe("Bearer test-token");
    // JSON body still declares its content type.
    expect(headers.get("Content-Type")).toBe("application/json");
  });

  it("routes BackupService.previewImport through buildHeaders without forcing a content type", async () => {
    const fetchMock = vi.fn<FetchFn>(async () =>
      fakeResponse({
        json: {
          manifest: {},
          compatible: true,
          files_to_replace: [],
          preview_token: "tok-123",
        },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const backup = new BackupService(
      new HeaderStampedClient({ baseUrl: "/api" }),
    );
    const preview = await backup.previewImport(
      new Blob(["bundle"]),
      "correct horse battery",
    );

    expect(preview.preview_token).toBe("tok-123");
    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/backup/import/preview");
    const headers = lastFetchHeaders(fetchMock);
    expect(headers.get("X-Test-Auth")).toBe("Bearer test-token");
    // Must stay unset so the platform can inject the multipart boundary.
    expect(headers.get("Content-Type")).toBeNull();
  });
});

describe("AnomalyService.list", () => {
  it("GETs /anomalies and returns the typed payload", async () => {
    const payload: ListAnomaliesResponse = {
      anomalies: [
        {
          id: "00000000-0000-0000-0000-000000000001",
          type: "blocklist_refresh_failing",
          severity: "error",
          component: "dns",
          subject_id: "bl-1",
          message: 'Blocklist "HaGeZi" has failed to refresh 7 times',
          hint: "check the URL is still reachable",
          details: { profile_id: "p-1" },
          opened_at: "2026-08-01T00:00:00Z",
          last_seen_at: "2026-08-01T06:00:00Z",
          occurrences: 7,
          resolved_at: null,
        },
      ],
    };
    const fetchMock = vi.fn<FetchFn>(async () =>
      fakeResponse({ json: payload }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const anomalies = new AnomalyService(
      new WardnetClient({ baseUrl: "/api" }),
    );
    const result = await anomalies.list();

    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/anomalies");
    const first: Anomaly = result.anomalies[0];
    expect(first.type).toBe("blocklist_refresh_failing");
    expect(first.subject_id).toBe("bl-1");
  });

  it("passes the status, subject and limit filters through", async () => {
    const fetchMock = vi.fn<FetchFn>(async () =>
      fakeResponse({ json: { anomalies: [] } }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const anomalies = new AnomalyService(
      new WardnetClient({ baseUrl: "/api" }),
    );
    await anomalies.list({ status: "all", subject_id: "tun-1", limit: 5 });

    const [url] = fetchMock.mock.calls[0];
    expect(String(url)).toContain("status=all");
    expect(String(url)).toContain("subject_id=tun-1");
    expect(String(url)).toContain("limit=5");
  });
});

describe("AnomalyService.reevaluate", () => {
  it("POSTs /anomalies/reevaluate and returns the summary", async () => {
    const fetchMock = vi.fn<FetchFn>(async () =>
      fakeResponse({ json: { evaluated: 4, resolved: 1 } }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const anomalies = new AnomalyService(
      new WardnetClient({ baseUrl: "/api" }),
    );
    const result = await anomalies.reevaluate();

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/anomalies/reevaluate");
    expect(init?.method).toBe("POST");
    expect(result.resolved).toBe(1);
  });
});

describe("header normalization", () => {
  it("preserves headers passed as a Headers instance", async () => {
    const fetchMock = vi.fn<FetchFn>(async () => fakeResponse({ json: {} }));
    vi.stubGlobal("fetch", fetchMock);

    const client = new WardnetClient({ baseUrl: "/api" });
    await client.request("/thing", {
      headers: new Headers({ "X-Custom": "yes" }),
    });

    const headers = lastFetchHeaders(fetchMock);
    expect(headers.get("X-Custom")).toBe("yes");
    // The JSON default is still applied on top of the caller's headers.
    expect(headers.get("Content-Type")).toBe("application/json");
  });

  it("preserves headers passed as an entries array", async () => {
    const fetchMock = vi.fn<FetchFn>(async () => fakeResponse({ json: {} }));
    vi.stubGlobal("fetch", fetchMock);

    const client = new WardnetClient({ baseUrl: "/api" });
    await client.request("/thing", { headers: [["X-Custom", "yes"]] });

    const headers = lastFetchHeaders(fetchMock);
    expect(headers.get("X-Custom")).toBe("yes");
  });
});

// Compile-time guard: these DTOs must remain importable from the package
// entry. If the re-exports in the SDK's index.ts regress, this file stops
// compiling and `tsc --noEmit` (and the test run) fails.
describe("public type surface", () => {
  it("exposes the tunnel DNS-override DTOs", () => {
    const req: UpdateTunnelDnsOverrideRequest = { override_default_dns: true };
    // Reference the response type at the type level only — it wraps a Tunnel,
    // whose full shape we don't need to reconstruct here.
    const readTunnel = (res: UpdateTunnelDnsOverrideResponse): unknown =>
      res.tunnel;
    expect(req.override_default_dns).toBe(true);
    expect(typeof readTunnel).toBe("function");
  });
});
