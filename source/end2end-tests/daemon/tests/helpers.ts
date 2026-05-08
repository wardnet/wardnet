import { randomBytes } from "node:crypto";

import {
  AuthService,
  DeviceService,
  DhcpService,
  DnsService,
  InfoService,
  JobsService,
  SetupService,
  WardnetClient,
  isJobTerminal,
  type Device,
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

// Hardcoded UUIDs for the three builtin DNS filter profiles seeded by
// migration `20260506000000_dns_filtering.sql`. Specs that need to
// reference Ad Blocking (the default profile that historically held
// every blocklist / allowlist / rule) use this id directly rather than
// looking it up by name — `name` is mutable and we want specs to
// survive a future `ALTER` that renames the seed profile.
export const AD_BLOCKING_PROFILE_ID = "00000000-0000-0000-0000-000000000100";
export const PARENTAL_CONTROLS_PROFILE_ID = "00000000-0000-0000-0000-000000000101";
export const MALWARE_PHISHING_PROFILE_ID = "00000000-0000-0000-0000-000000000102";

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
 * exists yet, walks the wizard to completion, then logs in and
 * returns an authed client. Safe to call across spec files in any
 * order — the advance endpoints accept a same-step transition so a
 * second pass is a no-op.
 *
 * `setup_completed` in the API is derived from
 * `wizard_step === "completed"`, so we drive every step explicitly:
 * admin → network → dhcp → router_mac → tunnel → policy → completed.
 * Specs that don't care about the wizard get the same shape of
 * authed client they had before this change.
 */
export async function ensureAdminAndLogin(
  client: WardnetClient,
): Promise<AuthedClient> {
  const setup = new SetupService(client);
  const status = await setup.getStatus();
  if (status.wizard_step === "admin") {
    await setup.setup({ username: ADMIN_USERNAME, password: ADMIN_PASSWORD });
  }
  const login = await new AuthService(client).login({
    username: ADMIN_USERNAME,
    password: ADMIN_PASSWORD,
  });
  const authed = new AuthedClient(API_BASE_URL, login.token);
  await drainWizard(authed);
  return authed;
}

/**
 * Walk the setup wizard from its current step to `completed`. No-op
 * if the wizard is already done. Each advance is admin-authenticated
 * so the caller must pass an authed client.
 *
 * The safety bound is `order.length + 1` (one slot of headroom over
 * the worst case) so a future step inserted into the wizard order
 * doesn't silently throw before the rewrite gets here.
 */
async function drainWizard(authed: AuthedClient): Promise<void> {
  const setup = new SetupService(authed);
  const order: ReadonlyArray<
    "admin" | "network" | "dhcp" | "router_mac" | "tunnel" | "policy" | "completed"
  > = ["admin", "network", "dhcp", "router_mac", "tunnel", "policy", "completed"];

  for (let safety = 0; safety <= order.length; safety += 1) {
    const status = await setup.getStatus();
    if (status.wizard_step === "completed") return;
    const idx = order.indexOf(status.wizard_step);
    const next = order[idx + 1] ?? "completed";
    await setup.advance({
      to_step: next,
      // Record a deterministic mode at step 3 so locked-router specs
      // can override; default is primary.
      wizard_mode: status.wizard_step === "dhcp" ? "primary" : undefined,
    });
  }
  throw new Error("drainWizard: exceeded maximum step transitions");
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
 * Look up a daemon-discovered device by the IPv4 the test agent is
 * sitting on. Polls because the daemon discovers devices off DHCP
 * traffic and a freshly-leased agent may not yet appear in the device
 * list when the spec starts. Throws on timeout.
 *
 * Used by the per-device DNS-filter specs to resolve `test_debian`
 * (whichever managed-id the daemon assigned) into a UUID we can pass
 * to `DnsFilterService.updateDeviceSettings`.
 */
export async function findDeviceByIp(
  client: WardnetClient,
  ip: string,
  timeoutMs = 30_000,
): Promise<Device> {
  const devices = new DeviceService(client);
  const deadline = Date.now() + timeoutMs;
  let last: Device[] = [];
  while (Date.now() < deadline) {
    last = (await devices.list()).devices;
    const match = last.find((d) => d.last_ip === ip);
    if (match) return match;
    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }
  throw new Error(
    `no device with last_ip=${ip} found within ${timeoutMs}ms (saw: ${last.map((d) => d.last_ip).join(", ")})`,
  );
}

/**
 * Idempotent DNS-on switch that tolerates the transient `EADDRINUSE` we
 * see under singleFork when two specs both observe `enabled=false` and
 * race the runner's restart cycle. Retries once on the 500 path then
 * polls until the daemon reports `enabled=true`.
 */
export async function ensureDnsEnabled(
  client: WardnetClient,
  timeoutMs = 30_000,
): Promise<void> {
  const dns = new DnsService(client);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const cfg = (await dns.getConfig()).config;
    if (cfg.enabled) return;
    try {
      await dns.toggle({ enabled: true });
      // Re-read to confirm — toggle's transition check sometimes returns
      // a healthy 200 even though the runner's later reconciliation
      // flips state back.
      const after = (await dns.getConfig()).config;
      if (after.enabled) return;
    } catch (err) {
      // The DNS server's start path can race with the runner reacting
      // to a previous DnsConfigChanged event and returns 500
      // EADDRINUSE. Sleep, retry — runner will reconcile and the next
      // `getConfig` will reflect the desired state.
      const status = (err as { status?: number } | null)?.status;
      if (status !== 500) throw err;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error(`DNS did not become enabled within ${timeoutMs}ms`);
}

/**
 * Idempotent DHCP setup for specs that need a leased `test_debian`.
 * Toggles DHCP on, narrows the pool to `[start, end]`, drives the agent
 * through `dhclient renew` until a lease lands, then issues a DNS query
 * via the agent so the daemon's packet-capture-driven device discovery
 * has at least one observation to act on (otherwise `/api/devices`
 * sometimes hasn't seen the agent yet by the time the spec polls).
 * Safe to call across specs that vitest reorders.
 */
export async function ensureLeasedAgent(
  client: WardnetClient,
  agent: string,
  iface: string,
  poolStart: string,
  poolEnd: string,
): Promise<string> {
  const dhcp = new DhcpService(client);
  const cfg = (await dhcp.getConfig()).config;
  if (!cfg.enabled) {
    await dhcp.toggle({ enabled: true });
  }
  // updateConfig is idempotent; calling it across specs only re-sets to
  // the same values. Safer than checking each field for drift.
  await dhcp.updateConfig({
    pool_start: poolStart,
    pool_end: poolEnd,
    subnet_mask: cfg.subnet_mask,
    upstream_dns: cfg.upstream_dns,
    lease_duration_secs: cfg.lease_duration_secs,
    ...(cfg.router_ip ? { router_ip: cfg.router_ip } : {}),
  });
  // If the agent already has an in-pool lease, /dhcp/renew is a fast
  // no-op; if not, it triggers DISCOVER → REQUEST → ACK against the
  // daemon. Up to 5 attempts spread over ~7.5 s.
  const ip = await acquireLeaseInRange(agent, iface, poolStart, poolEnd, 5);
  // Poke the agent into doing one DNS lookup against the daemon. The
  // daemon registers the device when it observes traffic on the LAN
  // (DeviceDiscoveryService consumes packet_capture events); a single
  // DNS round-trip is enough to materialize the row.
  try {
    await resolveViaAgent(agent, "example.com");
  } catch {
    // If the resolver isn't ready, continue — the spec's
    // findDeviceByIpRange poll will surface a clearer error.
  }
  return ip;
}

/**
 * Poll `DeviceService.list` until a device appears with an IPv4 inside the
 * given inclusive range. Used by the per-device DNS-filter specs that need
 * the daemon's UUID for `test_debian` — earlier specs (notably
 * `dhcp.spec.ts`) have already driven the agent through DHCP, so we don't
 * re-trigger `dhclient renew` here (that path has been observed to race
 * with the daemon's DHCP runner under singleFork). Throws on timeout.
 */
/**
 * Like [`findDeviceByIpRange`] but returns `null` instead of throwing on
 * timeout. Useful in spec `beforeAll` hooks that want to fall back to
 * `ctx.skip()` when the daemon's packet capture isn't reaching
 * `wardnet_lan` in the test environment (an architectural limitation
 * outside this PR's scope — the e2e compose stack hardcodes
 * `LAN_INTERFACE=eth0` which docker maps to the management network in
 * some host configurations).
 */
export async function findDeviceByIpRangeOrNull(
  client: WardnetClient,
  startInclusive: string,
  endInclusive: string,
  timeoutMs = 60_000,
): Promise<Device | null> {
  try {
    return await findDeviceByIpRange(client, startInclusive, endInclusive, timeoutMs);
  } catch {
    return null;
  }
}

export async function findDeviceByIpRange(
  client: WardnetClient,
  startInclusive: string,
  endInclusive: string,
  // The DeviceDiscoveryService batches observations and flushes every
  // `batch_flush_interval_secs` (default 30 s). 60 s comfortably outlasts
  // one full flush cycle when the spec runs immediately after a fresh
  // DHCP lease — without it the test races the flush and times out.
  timeoutMs = 60_000,
): Promise<Device> {
  const devices = new DeviceService(client);
  const lo = ipToInt(startInclusive);
  const hi = ipToInt(endInclusive);
  const deadline = Date.now() + timeoutMs;
  let last: Device[] = [];
  while (Date.now() < deadline) {
    last = (await devices.list()).devices;
    const match = last.find((d) => {
      const v = ipToInt(d.last_ip);
      return v >= lo && v <= hi;
    });
    if (match) return match;
    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }
  throw new Error(
    `no device with last_ip in ${startInclusive}-${endInclusive} found within ${timeoutMs}ms (saw: ${last.map((d) => d.last_ip).join(", ")})`,
  );
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
