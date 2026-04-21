/** Request body for POST /api/auth/login. */
export interface LoginRequest {
  username: string;
  password: string;
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
