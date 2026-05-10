/**
 * GARP failover e2e — issue #213.
 *
 * Drives the full passive-learning + claim/farewell pipeline against
 * the running daemon container:
 *
 * 1. Configure `dhcp_router_ip = 10.91.0.5` via the wizard so the
 *    daemon has a router IP to broadcast about.
 * 2. **Claim phase**: capture ARP traffic on `test_debian` while the
 *    daemon restarts via `POST /api/system/restart`. Assert ≥2
 *    gratuitous-reply frames sent from the daemon's LAN-iface MAC,
 *    with the configured `sender_ip` and broadcast destination.
 * 3. **Drive passive learning**: `POST /arp/send` from `test_debian`
 *    with `sender_mac = aa:bb:cc:dd:ee:01, sender_ip = 10.91.0.5` so
 *    the daemon's discovery loop populates `garp_router_mac`.
 * 4. **Farewell phase**: capture ARP traffic and trigger another
 *    restart. Assert ≥2 gratuitous-reply frames whose sender MAC
 *    equals the synthetic upstream-router MAC seeded in step 3 — the
 *    proof that the daemon read `garp_router_mac` and broadcast the
 *    farewell with the upstream router's MAC.
 */

import { describe, it, expect, beforeAll } from "vitest";
import {
  DhcpService,
  NetworkService,
  SystemService,
  WardnetClient,
} from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  agentPost,
  ensureAdminAndLogin,
  waitForReady,
} from "./helpers.js";

// `wardnetd` resolves to the daemon container on `wardnet_mgmt` and
// hosts the test-agent on :3001 in server mode (per safe-power.spec.ts).
const DAEMON_AGENT = "http://wardnetd:3001";

// IP advertised in the GARP frames. Pick a value comfortably outside
// docker IPAM (.2-.15) and the daemon's DHCP pool start (.100+).
const ROUTER_IP = "10.91.0.5";

// Synthetic upstream-router MAC used to seed `garp_router_mac` via the
// passive-learning pipeline. Locally-administered (second nibble = 2).
const SYNTHETIC_ROUTER_MAC = "aa:bb:cc:dd:ee:01";

// ARP opcode 2 = Reply. Gratuitous announcements use opcode 2 because
// every common stack updates its cache more reliably on reply than on
// request.
const ARP_REPLY_OPCODE = 2;

interface ArpSendBody {
  opcode?: number;
  sender_mac: string;
  sender_ip: string;
  target_mac?: string;
  target_ip: string;
  count?: number;
  interval_ms?: number;
  interface?: string;
}

interface ArpCaptureBody {
  duration_ms: number;
  interface?: string;
  filter?: { sender_ip?: string; sender_mac?: string; opcode?: number };
}

interface CapturedArpFrame {
  opcode: number;
  sender_ip: string;
  sender_mac: string;
  target_ip: string;
  target_mac: string;
  timestamp_ms: number;
  eth_source: string;
  eth_destination: string;
}

interface LinkShowResponse {
  name: string;
  exists: boolean;
  up: boolean;
  mtu?: number;
  mac?: string;
}

/**
 * Open an /arp/capture window and run `during` while it's running.
 * Returns the captured frames once the window closes. Coalesces the
 * window setup so each test reads as a linear "do this while
 * capturing" rather than juggling promises.
 *
 * The window is intentionally generous: HTTP round-trip +
 * `spawn_blocking` scheduling + `pnet`'s `datalink::channel` open can
 * take 1-2s on a cold runner before the agent is actually listening,
 * so we wait 2s before triggering anything in `during` and run the
 * capture for 15s total. The full daemon restart cycle (farewell +
 * exit + systemd respawn + claim) fits comfortably inside the
 * remaining window.
 */
async function captureWhile(
  durationMs: number,
  filter: ArpCaptureBody["filter"],
  during: () => Promise<void>,
): Promise<CapturedArpFrame[]> {
  const capturePromise = agentPost<CapturedArpFrame[]>(
    TEST_DEBIAN_AGENT,
    "/arp/capture",
    {
      duration_ms: durationMs,
      interface: "eth0",
      filter,
    } satisfies ArpCaptureBody,
  );
  await new Promise((r) => setTimeout(r, 2_000));
  await during();
  return capturePromise;
}

/** Restart the daemon via the SystemService SDK helper. Returns once
 *  the daemon is back to /api/info readiness. */
async function restartDaemon(authed: AuthedClient): Promise<void> {
  await new SystemService(authed).restart();
  // request_restart cancels the shutdown_token after a ~500ms grace,
  // then the process exits and systemd respawns. The full cycle —
  // farewell + exit + relaunch + claim — fits in ~5s on this stack.
  await waitForReady(new WardnetClient({ baseUrl: API_BASE_URL }), 30_000);
}

/** Set `dhcp_router_ip = ROUTER_IP` while leaving every other DHCP
 *  field at whatever the wizard left behind. */
async function configureRouterIp(authed: AuthedClient): Promise<void> {
  const dhcp = new DhcpService(authed);
  const cfg = (await dhcp.getConfig()).config;
  await dhcp.updateConfig({
    pool_start: cfg.pool_start,
    pool_end: cfg.pool_end,
    subnet_mask: cfg.subnet_mask,
    upstream_dns: cfg.upstream_dns,
    lease_duration_secs: cfg.lease_duration_secs,
    router_ip: ROUTER_IP,
  });
}

/** Resolves the daemon's wardnet_lan MAC.
 *
 * Asks the daemon which interface holds the LAN IP (Docker doesn't
 * guarantee a stable eth0/eth1/eth2 mapping when the container
 * attaches to multiple networks — the compose entrypoint already
 * pins the daemon's `lan_interface` to whichever NIC carries
 * 10.91.0.1; this lookup mirrors that), then pulls the MAC for
 * that interface from the daemon's test-agent. */
async function getDaemonLanMac(authed: AuthedClient): Promise<string> {
  const status = await new NetworkService(authed).getStatus();
  const res = await fetch(`${DAEMON_AGENT}/link/${status.interface}`);
  if (!res.ok) {
    throw new Error(
      `failed to fetch daemon link state for ${status.interface}: ${res.status} ${await res.text()}`,
    );
  }
  const link = (await res.json()) as LinkShowResponse;
  if (!link.mac) {
    throw new Error(
      `daemon ${status.interface} has no MAC in /link response: ${JSON.stringify(link)}`,
    );
  }
  return link.mac.toLowerCase();
}

describe("GARP failover (issue #213)", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  let admin: AuthedClient;
  let daemonMac: string;

  beforeAll(async () => {
    await waitForReady(client);
    admin = await ensureAdminAndLogin(client);
    await configureRouterIp(admin);
    daemonMac = await getDaemonLanMac(admin);
  }, 120_000);

  it("broadcasts the claim GARP on startup with the daemon's LAN MAC", async () => {
    // configureRouterIp in beforeAll cleared garp_router_mac, so the
    // pre-restart farewell hits the no-op matrix and only the
    // post-restart claim should land in the capture window.
    //
    // Capture without an agent-side filter so the failure message
    // shows what (if anything) the agent saw on the wire — narrowing
    // the diagnostic from "no claim pulses" to "no traffic at all"
    // vs "traffic but wrong sender_ip".
    const frames = await captureWhile(15_000, undefined, async () => {
      await restartDaemon(admin);
    });

    const arpReplies = frames.filter(
      (f) => f.opcode === ARP_REPLY_OPCODE && f.sender_ip === ROUTER_IP,
    );
    const claim = arpReplies.filter(
      (f) => f.sender_mac.toLowerCase() === daemonMac,
    );

    expect(
      claim.length,
      `expected ≥2 claim pulses; saw ${frames.length} total ARP frames, ${arpReplies.length} matching sender_ip=${ROUTER_IP} opcode=2. macs: ${JSON.stringify(frames.map((f) => f.sender_mac).slice(0, 20))}`,
    ).toBeGreaterThanOrEqual(2);

    // No-op matrix sanity: with garp_router_mac cleared, no farewell
    // pulses should appear. Anything from a sender_mac other than the
    // daemon's own MAC would mean the no-op path didn't trigger.
    const nonDaemonReplies = arpReplies.filter(
      (f) => f.sender_mac.toLowerCase() !== daemonMac,
    );
    expect(
      nonDaemonReplies,
      "no farewell pulses expected when garp_router_mac is empty",
    ).toEqual([]);

    for (const frame of claim) {
      expect(frame.opcode).toBe(ARP_REPLY_OPCODE);
      expect(frame.sender_ip).toBe(ROUTER_IP);
      expect(frame.target_ip).toBe(ROUTER_IP);
      expect(frame.eth_destination.toLowerCase()).toBe("ff:ff:ff:ff:ff:ff");
      expect(frame.eth_source.toLowerCase()).toBe(daemonMac);
    }

    // The 2-pulse / 500 ms gap is part of the GarpOps contract.
    if (claim.length >= 2) {
      const sorted = [...claim].sort((a, b) => a.timestamp_ms - b.timestamp_ms);
      const gap = sorted[1].timestamp_ms - sorted[0].timestamp_ms;
      expect(gap).toBeGreaterThanOrEqual(400);
      expect(gap).toBeLessThanOrEqual(1_000);
    }
  }, 90_000);

  it("learns the upstream router MAC from passive ARP and uses it on farewell", async () => {
    // Step 1: emit a single gratuitous-reply from the synthetic
    // router MAC. The daemon's discovery loop sees it on eth0,
    // matches obs.ip == dhcp_router_ip, and persists garp_router_mac.
    await agentPost<unknown>(TEST_DEBIAN_AGENT, "/arp/send", {
      opcode: ARP_REPLY_OPCODE,
      sender_mac: SYNTHETIC_ROUTER_MAC,
      sender_ip: ROUTER_IP,
      target_ip: ROUTER_IP,
      count: 2,
      interval_ms: 200,
      interface: "eth0",
    } satisfies ArpSendBody);

    // Discovery flushes batched observations on
    // `batch_flush_interval_secs`; default in fixtures is short. Give
    // it a comfortable margin before triggering a restart.
    await new Promise((r) => setTimeout(r, 3_000));

    // Step 2: capture window covering the next restart cycle. The
    // farewell goes out from the synthetic router MAC; the
    // immediately-following claim goes out from the daemon's MAC.
    const frames = await captureWhile(15_000, undefined, async () => {
      await restartDaemon(admin);
    });

    const arpReplies = frames.filter(
      (f) => f.opcode === ARP_REPLY_OPCODE && f.sender_ip === ROUTER_IP,
    );
    const farewell = arpReplies.filter(
      (f) => f.sender_mac.toLowerCase() === SYNTHETIC_ROUTER_MAC,
    );

    expect(
      farewell.length,
      `expected ≥2 farewell pulses with sender_mac=${SYNTHETIC_ROUTER_MAC}; ${frames.length} total frames, ${arpReplies.length} matching reply+ROUTER_IP. macs: ${JSON.stringify(arpReplies.map((f) => f.sender_mac).slice(0, 20))}`,
    ).toBeGreaterThanOrEqual(2);

    for (const frame of farewell) {
      expect(frame.opcode).toBe(ARP_REPLY_OPCODE);
      expect(frame.sender_ip).toBe(ROUTER_IP);
      expect(frame.target_ip).toBe(ROUTER_IP);
      expect(frame.eth_destination.toLowerCase()).toBe("ff:ff:ff:ff:ff:ff");
      // Decision 4: Ethernet src must match the ARP sender-HW so
      // L2 switch CAM tables flip in lockstep with client ARP caches.
      expect(frame.eth_source.toLowerCase()).toBe(SYNTHETIC_ROUTER_MAC);
    }

    if (farewell.length >= 2) {
      const sorted = [...farewell].sort((a, b) => a.timestamp_ms - b.timestamp_ms);
      const gap = sorted[1].timestamp_ms - sorted[0].timestamp_ms;
      expect(gap).toBeGreaterThanOrEqual(400);
      expect(gap).toBeLessThanOrEqual(1_000);
    }

    // Sanity: the post-restart claim also fired from the daemon's MAC.
    const claimAfterFarewell = arpReplies.filter(
      (f) => f.sender_mac.toLowerCase() === daemonMac,
    );
    expect(claimAfterFarewell.length).toBeGreaterThanOrEqual(1);
  }, 120_000);
});
