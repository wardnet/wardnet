import type { WardnetClient } from "../client.js";
import type {
  AdvanceWizardRequest,
  AdvanceWizardResponse,
  SetupRequest,
  SetupResponse,
  SetupStatusResponse,
} from "../types/setup.js";

/** Setup wizard service: status, first-admin creation, and step advance. */
export class SetupService {
  constructor(private readonly client: WardnetClient) {}

  /** Read the current wizard state. Unauthenticated. */
  async getStatus(): Promise<SetupStatusResponse> {
    return this.client.request<SetupStatusResponse>("/setup/status");
  }

  /** Create the first admin account (only works if setup not yet completed). */
  async setup(body: SetupRequest): Promise<SetupResponse> {
    return this.client.request<SetupResponse>("/setup", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Advance the wizard to a new step. Admin-authenticated. */
  async advance(body: AdvanceWizardRequest): Promise<AdvanceWizardResponse> {
    return this.client.request<AdvanceWizardResponse>("/setup/advance", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }
}
