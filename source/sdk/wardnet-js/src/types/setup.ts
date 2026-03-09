/** Response for GET /api/setup/status. */
export interface SetupStatusResponse {
  setup_completed: boolean;
}

/** Request body for POST /api/setup. */
export interface SetupRequest {
  username: string;
  password: string;
}

/** Response for POST /api/setup. */
export interface SetupResponse {
  message: string;
}
