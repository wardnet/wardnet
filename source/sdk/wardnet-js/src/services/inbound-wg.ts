import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  AddInboundWgPeerRequest,
  AddInboundWgPeerResponse,
  InboundWgConfigRequest,
  InboundWgConfigResponse,
  InboundWgPeerSummary,
  ListInboundWgPeersResponse,
  SetInboundWgPeerEnabledRequest,
} from "../types/inbound-wg.js";

/**
 * Inbound (multi-peer) WireGuard remote-access grant management
 * (issues #809-#811). All operations are admin-only.
 */
export class InboundWgService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** Read the current server config without mutating anything. */
  async getConfig(): Promise<InboundWgConfigResponse> {
    return this.api.get("/inbound-wg/config");
  }

  /** Enable/disable the inbound WireGuard server and set its listen port. */
  async setConfig(body: InboundWgConfigRequest): Promise<InboundWgConfigResponse> {
    return this.api.put("/inbound-wg/config", { body });
  }

  /** List every configured inbound peer (no private keys). */
  async listPeers(): Promise<ListInboundWgPeersResponse> {
    return this.api.get("/inbound-wg/peers");
  }

  /**
   * Grant remote access to an already-managed device. The response carries
   * the peer's private key exactly once — it is never persisted server-side.
   */
  async addPeer(body: AddInboundWgPeerRequest): Promise<AddInboundWgPeerResponse> {
    return this.api.post("/inbound-wg/peers", { body });
  }

  /** Revoke a peer's remote-access credential. */
  async removePeer(id: string): Promise<void> {
    await this.api.del("/inbound-wg/peers/{id}", { path: { id } });
  }

  /**
   * Pause or resume a peer without deleting its credential. Distinct from
   * {@link removePeer}, which revokes permanently and requires a fresh
   * keypair (and QR scan) to re-grant.
   */
  async setPeerEnabled(
    id: string,
    body: SetInboundWgPeerEnabledRequest,
  ): Promise<InboundWgPeerSummary> {
    return this.api.patch("/inbound-wg/peers/{id}", { path: { id }, body });
  }
}
