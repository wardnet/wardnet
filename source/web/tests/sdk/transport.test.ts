import { afterEach, describe, expect, it, vi } from "vitest";
import {
  BackupService,
  SystemService,
  WardnetClient,
  type SystemDiagnostic,
  type RecentErrorsResponse,
  type UpdateTunnelDnsOverrideRequest,
  type UpdateTunnelDnsOverrideResponse,
} from "@wardnet/js";

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

function lastFetchHeaders(mock: ReturnType<typeof vi.fn>): Headers {
  const init = mock.mock.calls.at(-1)?.[1] as RequestInit | undefined;
  return new Headers(init?.headers);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("WardnetClient transport seam", () => {
  it("routes BackupService.export through buildHeaders (auth header preserved)", async () => {
    const fetchMock = vi.fn(async () =>
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
    const fetchMock = vi.fn(async () =>
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

describe("SystemService.getRecentErrors", () => {
  it("GETs /system/errors and returns the typed payload", async () => {
    const payload: RecentErrorsResponse = {
      errors: [
        {
          timestamp: "2026-07-12T00:00:00Z",
          code: "dns_upstream_timeout",
          severity: "error",
          component: "dns",
          message: "upstream timeout",
          hint: "check the configured upstream resolvers",
        },
      ],
    };
    const fetchMock = vi.fn(async () => fakeResponse({ json: payload }));
    vi.stubGlobal("fetch", fetchMock);

    const system = new SystemService(new WardnetClient({ baseUrl: "/api" }));
    const result = await system.getRecentErrors();

    const [url] = fetchMock.mock.calls[0];
    expect(url).toBe("/api/system/errors");
    const first: SystemDiagnostic = result.errors[0];
    expect(first.message).toBe("upstream timeout");
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
