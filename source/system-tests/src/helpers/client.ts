import { WardnetClient } from "@wardnet/js";

/**
 * Test-specific API client that extends the SDK's WardnetClient with
 * cookie management for Node.js (where `credentials: "include"` is a no-op).
 *
 * After login, the session cookie is automatically captured and sent
 * with all subsequent requests.
 */
export class TestApiClient extends WardnetClient {
  private cookies: string[] = [];

  constructor(baseUrl: string) {
    super({ baseUrl });
  }

  override async request<T>(path: string, init?: RequestInit): Promise<T> {
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
    };

    if (this.cookies.length > 0) {
      headers["Cookie"] = this.cookies.join("; ");
    }

    const res = await fetch(`${this.baseUrl}${path}`, {
      ...init,
      headers: {
        ...headers,
        ...((init?.headers as Record<string, string> | undefined) ?? {}),
      },
    });

    // Capture set-cookie headers from the response.
    const setCookies = res.headers.getSetCookie?.() ?? [];
    for (const cookie of setCookies) {
      const name = cookie.split("=")[0];
      this.cookies = this.cookies.filter((c) => !c.startsWith(`${name}=`));
      this.cookies.push(cookie.split(";")[0]);
    }

    if (!res.ok) {
      const body = await res
        .json()
        .catch(() => ({ error: res.statusText }));
      throw new Error(
        `API ${res.status} ${init?.method ?? "GET"} ${path}: ${JSON.stringify(body)}`,
      );
    }

    return (await res.json()) as T;
  }
}
