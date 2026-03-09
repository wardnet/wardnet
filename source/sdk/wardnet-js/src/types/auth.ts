/** Request body for POST /api/auth/login. */
export interface LoginRequest {
  username: string;
  password: string;
}

/** Response body for POST /api/auth/login. */
export interface LoginResponse {
  message: string;
}
