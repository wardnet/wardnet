import type { WardnetClient } from "../client.js";
import type { LoginRequest, LoginResponse } from "../types/auth.js";

/** Authentication service for the Wardnet daemon. */
export class AuthService {
  constructor(private readonly client: WardnetClient) {}

  /** Log in as admin. The daemon sets a session cookie in the response. */
  async login(body: LoginRequest): Promise<LoginResponse> {
    return this.client.request<LoginResponse>("/auth/login", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /**
   * Extend the current session's expiry by 30 days (sliding-window refresh).
   *
   * Called by the admin-app on every open. Requires a valid session cookie.
   * No body — the daemon reads the token from the request cookie.
   */
  async refresh(): Promise<void> {
    await this.client.request<void>("/auth/refresh", { method: "POST" });
  }
}
