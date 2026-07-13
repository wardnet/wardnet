// Auto-update subsystem types. Mirror the Rust definitions in
// `source/daemon/crates/wardnet-common/src/update.rs` and `.../api.rs`.

export type UpdateChannel = "stable" | "beta";

export type UpdateHistoryStatus = "started" | "succeeded" | "failed" | "rolled_back";

/** Phase of an in-flight install. The tagged enum mirrors the Rust serde shape. */
export type InstallPhase =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "downloading"; bytes: number; total: number | null }
  | { phase: "verifying" }
  | { phase: "staging" }
  | { phase: "swapping" }
  | { phase: "restart_pending" }
  | { phase: "applied" }
  | { phase: "failed"; reason: string };

export interface Release {
  version: string;
  tarball_url: string;
  sha256_url: string;
  minisig_url: string | null;
  published_at: string | null;
  notes: string | null;
}

export interface UpdateHistoryEntry {
  id: number;
  from_version: string;
  to_version: string;
  phase: string;
  status: UpdateHistoryStatus;
  error: string | null;
  started_at: string;
  finished_at: string | null;
}

export interface InstallHandle {
  install_id: string;
  target_version: string;
}

export interface UpdateStatus {
  current_version: string;
  latest_version: string | null;
  update_available: boolean;
  auto_update_enabled: boolean;
  channel: UpdateChannel;
  last_check_at: string | null;
  last_install_at: string | null;
  install_phase: InstallPhase;
  /** Version an install is staging. Cleared once the daemon restarts onto it. */
  pending_version: string | null;
  /** Version the daemon finished applying across a restart, if any. */
  applied_version: string | null;
  /** When `applied_version` was observed running; dedupe key for the toast. */
  applied_at: string | null;
  rollback_available: boolean;
}

export interface UpdateStatusResponse {
  status: UpdateStatus;
}

export interface UpdateCheckResponse {
  status: UpdateStatus;
}

export interface InstallUpdateRequest {
  version?: string;
}

export interface InstallUpdateResponse {
  handle: InstallHandle;
  message: string;
}

export interface RollbackResponse {
  message: string;
}

export interface UpdateConfigRequest {
  auto_update_enabled?: boolean;
  channel?: UpdateChannel;
}

export interface UpdateConfigResponse {
  status: UpdateStatus;
}

export interface UpdateHistoryResponse {
  entries: UpdateHistoryEntry[];
}
