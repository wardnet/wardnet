import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  AdvanceWizardRequest,
  AdvanceWizardResponse,
  SetupRequest,
  SetupResponse,
  SetupStatusResponse,
} from "../types/setup.js";

/** Setup wizard service: status, first-admin creation, and step advance. */
export class SetupService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** Read the current wizard state. Unauthenticated. */
  async getStatus(): Promise<SetupStatusResponse> {
    return this.api.get("/setup/status");
  }

  /** Create the first admin account (only works if setup not yet completed). */
  async setup(body: SetupRequest): Promise<SetupResponse> {
    return this.api.post("/setup", { body });
  }

  /** Advance the wizard to a new step. Admin-authenticated. */
  async advance(body: AdvanceWizardRequest): Promise<AdvanceWizardResponse> {
    return this.api.post("/setup/advance", { body });
  }
}
