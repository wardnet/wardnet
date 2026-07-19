import type { WardnetClient } from "../client.js";
import type { LoginRequest, LoginResponse, MeResponse } from "../types/auth.js";

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

  /**
   * End the current session.
   *
   * The daemon deletes the session server-side (the token can never
   * authenticate again) and clears the `wardnet_session` cookie in the
   * response. No body — the daemon reads the token from the request cookie.
   *
   * Bounded by a 5s timeout: sign-out flows gate navigation on this call, and
   * an unreachable gateway (router mid-reboot, phone off the home LAN) would
   * otherwise leave the UI hanging on the browser's connection timeout.
   */
  async logout(): Promise<void> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 5000);
    try {
      await this.client.request<void>("/auth/logout", {
        method: "POST",
        signal: controller.signal,
      });
    } finally {
      clearTimeout(timer);
    }
  }

  /** Return the authenticated admin's identity. */
  async me(): Promise<MeResponse> {
    return this.client.request<MeResponse>("/users/me");
  }
}
