import { randomBytes } from "node:crypto";

import {
  AuthService,
  InfoService,
  JobsService,
  SetupService,
  WardnetClient,
  isJobTerminal,
  type Job,
} from "@wardnet/js";

// Compose service names resolve to the corresponding container's IP on
// each shared bridge. The test runner sits on both wardnet_mgmt (where
// it reaches the daemon API) and wardnet_lan (where the test-agent
// HTTP servers listen on :3001).
export const API_BASE_URL =
  process.env.WARDNET_API_BASE_URL ?? "http://wardnetd:7411/api";
export const TEST_DEBIAN_AGENT =
  process.env.WARDNET_TEST_DEBIAN_AGENT ?? "http://test_debian:3001";
export const TEST_UBUNTU_AGENT =
  process.env.WARDNET_TEST_UBUNTU_AGENT ?? "http://test_ubuntu:3001";

// Setup-wizard credentials. Generated per-process so a leaked log line
// can't be replayed against a real instance. `randomBytes` (vs
// `Math.random`) keeps CodeQL's js/insecure-randomness rule happy --
// the credential is test-only and never leaves the compose stack, but
// the rule fires on shape, not reachability.
export const ADMIN_USERNAME = "admin";
export const ADMIN_PASSWORD = `e2e-${randomBytes(6).toString("hex")}`;

/**
 * `WardnetClient` that re-attaches the bearer token returned by login
 * to every subsequent request. Node's fetch has no cookie jar, so the
 * session cookie the daemon sets is invisible to follow-up calls;
 * `Authorization: Bearer <token>` is the documented non-browser path.
 */
export class AuthedClient extends WardnetClient {
  constructor(
    baseUrl: string,
    private readonly token: string,
  ) {
    super({ baseUrl });
  }

  override async request<T>(path: string, init?: RequestInit): Promise<T> {
    const headers = new Headers(init?.headers);
    headers.set("Content-Type", "application/json");
    headers.set("Authorization", `Bearer ${this.token}`);
    return super.request<T>(path, { ...init, headers });
  }
}

/** Polls `/api/info` until the daemon responds, or throws. */
export async function waitForReady(
  client: WardnetClient,
  timeoutMs = 90_000,
): Promise<void> {
  const info = new InfoService(client);
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;
  while (Date.now() < deadline) {
    try {
      await info.getInfo();
      return;
    } catch (err) {
      lastError = err;
      await new Promise((resolve) => setTimeout(resolve, 1_000));
    }
  }
  throw new Error(
    `daemon did not become ready within ${timeoutMs}ms: ${String(lastError)}`,
  );
}

/**
 * Idempotent admin bootstrap. Runs the setup wizard if no admin
 * exists yet, then logs in and returns an authed client. Safe to call
 * across spec files in any order — the wizard endpoint is a no-op
 * once `setup_completed` flips.
 */
export async function ensureAdminAndLogin(
  client: WardnetClient,
): Promise<AuthedClient> {
  const setup = new SetupService(client);
  const status = await setup.getStatus();
  if (!status.setup_completed) {
    await setup.setup({ username: ADMIN_USERNAME, password: ADMIN_PASSWORD });
  }
  const login = await new AuthService(client).login({
    username: ADMIN_USERNAME,
    password: ADMIN_PASSWORD,
  });
  return new AuthedClient(API_BASE_URL, login.token);
}

/** Shape returned by `wardnet-test-agent client serve`'s /interfaces. */
export interface AgentInterface {
  name: string;
  up: boolean;
  mac: string | null;
  mtu: number;
  addrs: Array<{ family: string; local: string; prefixlen: number }>;
}

export interface AgentInterfacesResponse {
  interfaces: AgentInterface[];
}

export interface AgentDhcpRenewResponse {
  interface: string;
  client: string;
  release_success: boolean;
  renew_success: boolean;
  stdout: string;
  stderr: string;
}

/** GET against a test-agent serve URL. Throws on non-2xx. */
export async function agentGet<T>(
  baseUrl: string,
  path: string,
): Promise<T> {
  const res = await fetch(`${baseUrl}${path}`);
  if (!res.ok) {
    throw new Error(
      `agent GET ${baseUrl}${path} failed: ${res.status} ${await res.text()}`,
    );
  }
  return (await res.json()) as T;
}

/** POST JSON to a test-agent serve URL. Throws on non-2xx. */
export async function agentPost<T>(
  baseUrl: string,
  path: string,
  body: unknown,
): Promise<T> {
  const res = await fetch(`${baseUrl}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    throw new Error(
      `agent POST ${baseUrl}${path} failed: ${res.status} ${await res.text()}`,
    );
  }
  return (await res.json()) as T;
}

/** First IPv4 address on the named interface, or undefined. */
export function ipv4Of(
  ifaces: AgentInterfacesResponse,
  name: string,
): string | undefined {
  return ifaces.interfaces
    .find((i) => i.name === name)
    ?.addrs.find((a) => a.family === "inet")?.local;
}

/**
 * Returns the IPv4 on `name` whose value sits within `[start, end]`,
 * or undefined if none. The e2e clients keep both their docker-IPAM
 * address and the daemon-issued lease on eth0 simultaneously, so a
 * "first inet wins" pick can return the wrong one.
 */
export function ipv4InRange(
  ifaces: AgentInterfacesResponse,
  name: string,
  startInclusive: string,
  endInclusive: string,
): string | undefined {
  const lo = ipToInt(startInclusive);
  const hi = ipToInt(endInclusive);
  return ifaces.interfaces
    .find((i) => i.name === name)
    ?.addrs.filter((a) => a.family === "inet")
    .map((a) => a.local)
    .find((ip) => {
      const v = ipToInt(ip);
      return v >= lo && v <= hi;
    });
}

/** MAC of the named interface, or undefined. Lowercased for compares. */
export function macOf(
  ifaces: AgentInterfacesResponse,
  name: string,
): string | undefined {
  return ifaces.interfaces.find((i) => i.name === name)?.mac?.toLowerCase();
}

/**
 * Drives the test-agent's /dhcp/renew until an address in `[poolStart,
 * poolEnd]` lands on `iface`, or fails after `attempts`. The daemon's
 * DHCP server starts disabled, so beforeAll() must call dhcp.toggle()
 * before this. Retries because dhclient races with the daemon's
 * DHCP-runner spawn after toggle on a cold stack.
 */
export async function acquireLeaseInRange(
  agent: string,
  iface: string,
  poolStart: string,
  poolEnd: string,
  attempts = 5,
): Promise<string> {
  let lastErr: unknown;
  for (let i = 0; i < attempts; i++) {
    try {
      const renew = await agentPost<AgentDhcpRenewResponse>(
        agent,
        "/dhcp/renew",
        { interface: iface },
      );
      if (renew.renew_success) {
        const ifaces = await agentGet<AgentInterfacesResponse>(
          agent,
          `/interfaces?name=${iface}`,
        );
        const ip = ipv4InRange(ifaces, iface, poolStart, poolEnd);
        if (ip) {
          return ip;
        }
      }
      lastErr = new Error(
        `attempt ${i + 1}: renew_success=${renew.renew_success}, no in-pool IP yet — stderr: ${renew.stderr}`,
      );
    } catch (err) {
      lastErr = err;
    }
    await new Promise((resolve) => setTimeout(resolve, 1_500));
  }
  throw new Error(
    `could not acquire lease in ${poolStart}-${poolEnd} on ${agent}/${iface}: ${String(lastErr)}`,
  );
}

/**
 * Convert a dotted-quad IPv4 to a 32-bit integer for range comparisons.
 * Bitwise ops in JS would coerce to signed 32-bit; multiply-and-add
 * keeps the value safely positive.
 */
export function ipToInt(ip: string): number {
  return ip
    .split(".")
    .map(Number)
    .reduce((acc, n) => acc * 256 + n, 0);
}

export interface DnsResolveResponse {
  name: string;
  server?: string;
  addrs: string[];
}

export interface ResolveOptions {
  server?: string;
  record?: "A" | "AAAA" | "TXT" | "CNAME";
  timeout?: number;
}

/**
 * Drive a LAN-client agent's `/dns/resolve` probe. Defaults to
 * querying the wardnetd LAN-side DNS at 10.91.0.1 over A records.
 */
export async function resolveViaAgent(
  agent: string,
  name: string,
  opts: ResolveOptions = {},
): Promise<DnsResolveResponse> {
  const params = new URLSearchParams({ name });
  params.set("server", opts.server ?? "10.91.0.1");
  if (opts.record) params.set("record", opts.record);
  if (opts.timeout !== undefined) params.set("timeout", String(opts.timeout));
  return agentGet<DnsResolveResponse>(agent, `/dns/resolve?${params}`);
}

/**
 * Poll `JobsService.get` until the job reaches a terminal state, or
 * throw on timeout. Returns the final `Job` so callers can assert on
 * `status === "SUCCEED"` and surface `error` on failure paths.
 */
export async function waitForJob(
  jobs: JobsService,
  id: string,
  timeoutMs = 30_000,
  pollIntervalMs = 500,
): Promise<Job> {
  const deadline = Date.now() + timeoutMs;
  let last: Job | undefined;
  while (Date.now() < deadline) {
    last = await jobs.get(id);
    if (isJobTerminal(last.status)) {
      return last;
    }
    await new Promise((resolve) => setTimeout(resolve, pollIntervalMs));
  }
  throw new Error(
    `job ${id} did not reach a terminal state within ${timeoutMs}ms (last status=${last?.status})`,
  );
}
