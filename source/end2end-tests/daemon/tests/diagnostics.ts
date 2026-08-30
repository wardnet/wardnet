/**
 * On-failure daemon state dump.
 *
 * A failing e2e assertion tells you the daemon did the wrong thing, not why.
 * Reproducing locally is not an option — the systemd-in-container stack only
 * boots on CI — so the run that fails is usually the only chance to see the
 * state that produced it, and by the time the artefacts are read the stack is
 * long gone.
 *
 * Registered globally (see `vitest.config.ts` -> `setupFiles`), so every spec
 * gets this for free rather than each one growing its own bespoke logging.
 *
 * Everything here is best-effort and individually caught: a diagnostic that
 * throws would replace the real assertion error with its own, which is worse
 * than no diagnostic at all.
 */

import { afterEach } from "vitest";
import {
  DeviceService,
  DnsService,
  NetworkZonesService,
  WardnetClient,
} from "@wardnet/js";

import {
  API_BASE_URL,
  DAEMON_AGENT,
  agentGet,
  ensureAdminAndLogin,
  type AuthedClient,
  type DaemonRoutesResponse,
  type DaemonIpRulesResponse,
} from "./helpers.js";

/** How many recent query-log rows to show. Enough for one spec's traffic. */
const QUERY_LOG_ROWS = 30;

let cached: AuthedClient | undefined;

async function client(): Promise<AuthedClient> {
  if (!cached) {
    cached = await ensureAdminAndLogin(
      new WardnetClient({ baseUrl: API_BASE_URL }),
    );
  }
  return cached;
}

async function section(title: string, body: () => Promise<string>) {
  try {
    return `--- ${title} ---\n${await body()}`;
  } catch (e) {
    return `--- ${title} ---\n(unavailable: ${String(e)})`;
  }
}

/**
 * Devices as the daemon currently sees them.
 *
 * `last_ip` is the field per-device DNS filtering and the zone host routes key
 * on, and the e2e clients hold BOTH a docker-IPAM address and their DHCP lease
 * — so a device whose `last_ip` is its docker address is attributed
 * differently from one on its lease, and that alone explains a whole class of
 * "the rule did not apply" failures.
 */
async function devices(): Promise<string> {
  const list = (await new DeviceService(await client()).list()).devices;
  return list
    .map(
      (d) =>
        `  ${d.id.slice(0, 8)}  ip=${d.last_ip.padEnd(15)} mac=${d.mac}  zone=${(d.zone_id ?? "-").slice(0, 8)}`,
    )
    .join("\n");
}

async function zones(): Promise<string> {
  const list = (await new NetworkZonesService(await client()).list()).zones;
  return list
    .map(
      (z) =>
        `  ${z.id.slice(0, 8)}  ${z.name.padEnd(22)} member_isolation=${z.member_isolation} subnet=${z.subnet?.cidr ?? "-"}`,
    )
    .join("\n");
}

/** Recent resolutions, with the device each was attributed to. */
async function queryLog(): Promise<string> {
  const rows = (
    await new DnsService(await client()).listQueryLog({
      limit: QUERY_LOG_ROWS,
    })
  ).entries;
  return rows
    .map(
      (r) =>
        `  ${r.timestamp}  ${r.client_ip.padEnd(15)} dev=${(r.device_id ?? "-").slice(0, 8).padEnd(8)} ${r.result.padEnd(12)} ${r.domain}`,
    )
    .join("\n");
}

/** Kernel routing state the daemon owns. */
async function kernel(): Promise<string> {
  const [routes, rules] = await Promise.all([
    agentGet<DaemonRoutesResponse>(DAEMON_AGENT, "/routes"),
    agentGet<DaemonIpRulesResponse>(DAEMON_AGENT, "/ip-rules"),
  ]);
  const r = routes.routes
    .map(
      (x) =>
        `  ${x.dst.padEnd(18)} dev=${x.dev ?? "-"} src=${x.src ?? "-"} table=${x.table ?? "main"}`,
    )
    .join("\n");
  const ip = rules.rules
    .map((x) => `  ${x.priority}: from ${x.from} lookup ${x.table}`)
    .join("\n");
  return `routes:\n${r}\nip rules:\n${ip}`;
}

/** Gather everything. Never throws. */
export async function daemonSnapshot(label: string): Promise<string> {
  const parts = await Promise.all([
    section("devices (last_ip drives filter + host-route keying)", devices),
    section("zones", zones),
    section(`dns query log (last ${QUERY_LOG_ROWS})`, queryLog),
    section("kernel", kernel),
  ]);
  return [`===== daemon snapshot: ${label} =====`, ...parts, "=====".repeat(6)]
    .join("\n");
}

afterEach(async (ctx) => {
  if (ctx.task.result?.state !== "fail") return;
  try {
    // eslint-disable-next-line no-console
    console.error(await daemonSnapshot(ctx.task.name));
  } catch {
    // A broken diagnostic must never mask the assertion that triggered it.
  }
}, 60_000);
