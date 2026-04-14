/**
 * System test environment configuration.
 *
 * Reads from environment variables with sensible defaults that match
 * wardnet-test.env. Override via env vars when running from a dev machine
 * against a remote Pi.
 */
export const env = {
  /** Pi's IP address on the test bridge network. */
  piHost: process.env.WARDNET_PI_IP ?? "10.232.1.10",

  /** Port wardnetd listens on. */
  apiPort: Number(process.env.WARDNET_API_PORT ?? "7411"),

  /** Port the test agent listens on. */
  agentPort: Number(process.env.WARDNET_AGENT_PORT ?? "3001"),

  /** IP of the test_alpine container on the bridge. */
  testAlpineIp: process.env.TEST_ALPINE_IP ?? "172.30.0.10",

  /** IP of the test_ubuntu container on the bridge. */
  testUbuntuIp: process.env.TEST_UBUNTU_IP ?? "172.30.0.11",

  /** Mock peer 1 internal WireGuard IP (only reachable through tunnel 1). */
  mockPeer1Internal: process.env.MOCK_PEER_1_INTERNAL ?? "10.99.1.1",

  /** Mock peer 2 internal WireGuard IP (only reachable through tunnel 2). */
  mockPeer2Internal: process.env.MOCK_PEER_2_INTERNAL ?? "10.99.2.1",

  /** Admin username for setup wizard. */
  adminUser: process.env.TEST_ADMIN_USER ?? "admin",

  /** Admin password for setup wizard. */
  adminPass: process.env.TEST_ADMIN_PASS ?? "testpassword123",

  /** Idle timeout in seconds (must match daemon config). */
  idleTimeoutSecs: Number(process.env.WARDNET_IDLE_TIMEOUT ?? "5"),

  /** Base URL for the wardnetd API. */
  get apiUrl(): string {
    return `http://${this.piHost}:${this.apiPort}/api`;
  },

  /** Base URL for the test agent. */
  get agentUrl(): string {
    return `http://${this.piHost}:${this.agentPort}`;
  },
} as const;
