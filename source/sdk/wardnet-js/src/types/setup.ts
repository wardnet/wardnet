/**
 * Linear stage in the first-run setup wizard.
 *
 * The wizard advances `admin → network → dhcp → router_mac → tunnel →
 * policy → secure_access → completed`. `setup_completed` (in
 * `SetupStatusResponse` and the `SetupGuard` redirect logic) is derived from
 * `wizard_step === "completed"`.
 */
export type WizardStep =
  | "admin"
  | "network"
  | "dhcp"
  | "router_mac"
  | "tunnel"
  | "policy"
  | "secure_access"
  | "completed";

/**
 * Branch chosen at step 3 of the wizard.
 *
 * `primary` runs Wardnet's built-in DHCP server. `locked_router` keeps
 * the upstream ISP router as the LAN's DHCP server and configures
 * opted-in devices statically.
 */
export type WizardMode = "primary" | "locked_router";

/** Response for GET /api/setup/status. */
export interface SetupStatusResponse {
  /** Derived: `wizard_step === "completed"`. */
  setup_completed: boolean;
  wizard_step: WizardStep;
  wizard_mode: WizardMode | null;
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

/** Request body for POST /api/setup/advance. */
export interface AdvanceWizardRequest {
  to_step: WizardStep;
  /** Set when transitioning into `dhcp` so the daemon knows which branch. */
  wizard_mode?: WizardMode | null;
}

/** Response for POST /api/setup/advance. */
export interface AdvanceWizardResponse {
  wizard_step: WizardStep;
  wizard_mode: WizardMode | null;
}
