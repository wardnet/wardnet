import type { WardnetClient } from "../client.js";
import type {
  AddInboundWgPeerRequest,
  AddInboundWgPeerResponse,
  InboundWgConfigRequest,
  InboundWgConfigResponse,
  ListInboundWgPeersResponse,
} from "../types/inbound-wg.js";

/**
 * Inbound (multi-peer) WireGuard remote-access grant management
 * (issues #809-#811). All operations are admin-only.
 */
export class InboundWgService {
  constructor(private readonly client: WardnetClient) {}

  /** Enable/disable the inbound WireGuard server and set its listen port. */
  async setConfig(body: InboundWgConfigRequest): Promise<InboundWgConfigResponse> {
    return this.client.request<InboundWgConfigResponse>("/inbound-wg/config", {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /** List every configured inbound peer (no private keys). */
  async listPeers(): Promise<ListInboundWgPeersResponse> {
    return this.client.request<ListInboundWgPeersResponse>("/inbound-wg/peers");
  }

  /**
   * Grant remote access to an already-managed device. The response carries
   * the peer's private key exactly once — it is never persisted server-side.
   */
  async addPeer(body: AddInboundWgPeerRequest): Promise<AddInboundWgPeerResponse> {
    return this.client.request<AddInboundWgPeerResponse>("/inbound-wg/peers", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Revoke a peer's remote-access credential. */
  async removePeer(id: string): Promise<void> {
    await this.client.request<void>(`/inbound-wg/peers/${id}`, {
      method: "DELETE",
    });
  }
}
