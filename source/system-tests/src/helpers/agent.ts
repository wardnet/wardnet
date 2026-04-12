/**
 * Client for the wardnet-test-agent running on the Pi.
 *
 * Provides typed access to kernel state (ip rules, nftables, WireGuard)
 * and container operations (podman exec) over HTTP.
 */

// -- Response types matching the test agent's models.rs ----------------------

export interface IpRule {
  priority: number;
  from: string;
  table: string;
}

export interface IpRulesResponse {
  rules: IpRule[];
  raw: string;
}

export interface NftRulesResponse {
  raw: string;
  tables: string[];
  has_masquerade_for: string[];
}

export interface WgPeer {
  public_key: string;
  endpoint?: string;
  allowed_ips: string[];
  latest_handshake?: string;
  transfer_rx: number;
  transfer_tx: number;
}

export interface WgShowResponse {
  interface: string;
  exists: boolean;
  public_key?: string;
  listening_port?: number;
  peers?: WgPeer[];
}

export interface LinkShowResponse {
  name: string;
  exists: boolean;
  up: boolean;
  mtu?: number;
}

export interface ContainerExecResponse {
  exit_code: number;
  stdout: string;
  stderr: string;
}

// -- Test agent client -------------------------------------------------------

export class TestAgent {
  constructor(private readonly baseUrl: string) {}

  /** Check that the test agent is reachable. */
  async health(): Promise<{ status: string }> {
    return this.get("/health");
  }

  /** Get parsed ip rule list from the Pi. */
  async ipRules(): Promise<IpRulesResponse> {
    return this.get("/ip-rules");
  }

  /** Get nftables ruleset with parsed table and masquerade info. */
  async nftRules(): Promise<NftRulesResponse> {
    return this.get("/nft-rules");
  }

  /** Get WireGuard interface state. Returns { exists: false } if missing. */
  async wgShow(iface: string): Promise<WgShowResponse> {
    return this.get(`/wg/${encodeURIComponent(iface)}`);
  }

  /** Get network link state. Returns { exists: false } if missing. */
  async linkShow(iface: string): Promise<LinkShowResponse> {
    return this.get(`/link/${encodeURIComponent(iface)}`);
  }

  /** Read a generated fixture file from the Pi (e.g. "tunnel-1.conf"). */
  async readFixture(name: string): Promise<string> {
    const res = await fetch(
      `${this.baseUrl}/fixtures/${encodeURIComponent(name)}`,
    );
    if (!res.ok) {
      throw new Error(`Failed to read fixture "${name}": ${res.status}`);
    }
    return res.text();
  }

  /** Execute a command inside a container on the Pi. */
  async containerExec(
    container: string,
    command: string[],
  ): Promise<ContainerExecResponse> {
    return this.post("/container/exec", { container, command });
  }

  /** Ping from a container. Returns true if ping succeeds. */
  async ping(
    container: string,
    target: string,
    count = 1,
    timeoutSecs = 2,
  ): Promise<boolean> {
    const result = await this.containerExec(container, [
      "ping",
      "-c",
      String(count),
      "-W",
      String(timeoutSecs),
      target,
    ]);
    return result.exit_code === 0;
  }

  private async get<T>(path: string): Promise<T> {
    const res = await fetch(`${this.baseUrl}${path}`);
    if (!res.ok) {
      const body = await res.text().catch(() => res.statusText);
      throw new Error(`Agent GET ${path}: ${res.status} ${body}`);
    }
    return (await res.json()) as T;
  }

  private async post<T>(path: string, body: unknown): Promise<T> {
    const res = await fetch(`${this.baseUrl}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      const text = await res.text().catch(() => res.statusText);
      throw new Error(`Agent POST ${path}: ${res.status} ${text}`);
    }
    return (await res.json()) as T;
  }
}
