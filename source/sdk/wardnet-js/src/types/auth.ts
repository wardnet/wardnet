/** Request body for POST /api/auth/login. */
export interface LoginRequest {
  username: string;
  password: string;
  /** When `true`, the session is set with a 30-day Max-Age. */
  rememberMe?: boolean;
}

/** Response body for POST /api/auth/login.
 *
 * `token` is the same opaque value written into the `wardnet_session` cookie;
 * non-browser clients can replay it via `Authorization: Bearer <token>`.
 * `expiresInSeconds` is the remaining lifetime from the time of the response.
 */
export interface LoginResponse {
  message: string;
  token: string;
  expiresInSeconds: number;
}

/** Response body for GET /api/users/me. */
export interface MeResponse {
  /**
   * The authenticated user's display name.
   *
   * Kept under the name `username` **additively** (ADR-0031 §8): callers
   * written before household identity read this field, and for a backfilled
   * local admin it is exactly the old `admins.username`. New code should
   * prefer `displayName`, which is the same value under an honest name.
   */
  username: string;
  /** The authenticated user's id. */
  id: string;
  /** Same value as `username`. */
  displayName: string;
  /** `null` for a local admin created by the wizard, which asks for no email. */
  email: string | null;
  /**
   * `admin` or `member`. Lets a UI hide admin-only surfaces without probing
   * endpoints for 403s — a convenience, never the authorization itself, which
   * the daemon enforces on every call regardless.
   */
  role: "admin" | "member";
}
