import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  BackupService,
  DhcpService,
  DnsFilterService,
  SystemService,
  WardnetApiError,
  WardnetClient,
  type LocalSnapshot,
} from "@wardnet/js";

import {
  AD_BLOCKING_PROFILE_ID,
  API_BASE_URL,
  AuthedClient,
  DAEMON_AGENT,
  daemonPid,
  ensureAdminAndLogin,
  waitForDaemonRestart,
  waitForReady,
} from "./helpers.js";

// Seeds the round-trip carries. Both are namespaced to this spec so a
// re-run against a persistent `wardnet_state` volume — or a collision with
// dhcp-reservations.spec.ts, which owns .160 — is impossible.
//
// `.170` sits in the .151–.199 reserved-static band from the e2e plan:
// outside the daemon's dynamic pool (.100–.150), so the reservation can't
// race a concurrent lease. The MAC is locally-administered (`02:` prefix) and
// belongs to no container in the topology, so no DHCP traffic ever exercises
// it — this reservation exists purely as a database row to lose and recover.
const RESERVED_IP = "10.91.0.170";
const RESERVED_MAC = "02:e2:e2:00:02:49";
const RESERVATION_HOSTNAME = "e2e-backup-roundtrip";

const BLOCKLIST_NAME = "e2e-backup-roundtrip";
const BLOCKLIST_URL = "http://10.91.0.200/dns-blocklists.txt";
// Annual cron + disabled: this blocklist is a row to back up, not a filter to
// exercise. Neither the dns_filter runner nor a create-time refresh may fetch
// it, or the spec would be timing-coupled to a download job it doesn't care
// about (dns-blocklists.spec.ts owns that coverage).
const CRON_NEVER = "0 0 1 1 *";

// Server-side minimum is 12 characters (`MIN_PASSPHRASE_LEN`).
const PASSPHRASE = "e2e-backup-roundtrip-passphrase";
const SHORT_PASSPHRASE = "tooshort";

/**
 * Backup export → destroy → preview → apply → restart round-trip
 * (issue #249, E2E Stage 11).
 *
 * Order-sensitive: `applyImport` replaces the live database with the bundle's
 * copy, so every row written between export and apply is rolled back. That is
 * safe under vitest's `fileParallelism: false` + `isolate: false` model —
 * nothing else is running — and the window is kept as narrow as possible: the
 * only writes in it are this spec's own deletions.
 *
 * The restart is not optional. The running SQLite pool holds an open fd to the
 * file `apply_import` renamed to `.bak-<ts>`, so until the daemon restarts it
 * is still reading and writing the *pre-restore* database. Asserting recovery
 * before the restart would pass for the wrong reason.
 */
describe("backup round-trip (issue #249)", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  let admin: AuthedClient;
  let backup: BackupService;
  let dhcp: DhcpService;
  let dnsFilter: DnsFilterService;

  let bundle: Blob;
  let appliedSnapshots: LocalSnapshot[] = [];

  beforeAll(async () => {
    await waitForReady(client);
    admin = await ensureAdminAndLogin(client);
    backup = new BackupService(admin);
    dhcp = new DhcpService(admin);
    dnsFilter = new DnsFilterService(admin);

    // Clear anything a previous run on the same volume left behind, so the
    // seeding step below starts from a known-absent state.
    await removeSeeds();
  }, 120_000);

  afterAll(async () => {
    // The restore brings the seeds back, so tear them down again — otherwise a
    // second run against the same volume would find them already present and
    // the "reappeared" assertion would be vacuous. Best-effort: a failure here
    // must not mask the real one.
    try {
      await removeSeeds();
    } catch {
      // ignore
    }
  });

  /** Delete this spec's reservation + blocklist if present. Idempotent. */
  async function removeSeeds(): Promise<void> {
    const reservations = await dhcp.listReservations();
    for (const r of reservations.reservations) {
      if (
        r.mac_address.toLowerCase() === RESERVED_MAC ||
        r.ip_address === RESERVED_IP
      ) {
        await dhcp.deleteReservation(r.id);
      }
    }
    const blocklists = await dnsFilter.listBlocklists(AD_BLOCKING_PROFILE_ID);
    for (const b of blocklists.blocklists) {
      if (b.name === BLOCKLIST_NAME) {
        await dnsFilter.deleteBlocklist(AD_BLOCKING_PROFILE_ID, b.id);
      }
    }
  }

  /** True when both seeds are present in the live database. */
  async function seedsPresent(): Promise<{
    reservation: boolean;
    blocklist: boolean;
  }> {
    const reservations = await dhcp.listReservations();
    const blocklists = await dnsFilter.listBlocklists(AD_BLOCKING_PROFILE_ID);
    return {
      reservation: reservations.reservations.some(
        (r) => r.mac_address.toLowerCase() === RESERVED_MAC,
      ),
      blocklist: blocklists.blocklists.some((b) => b.name === BLOCKLIST_NAME),
    };
  }

  it("reports an idle subsystem before any operation", async () => {
    const { status } = await backup.status();
    expect(status.state).toBe("idle");
  });

  it("rejects an export whose passphrase is under the 12-character minimum", async () => {
    const err = await backup
      .export({ passphrase: SHORT_PASSPHRASE })
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(WardnetApiError);
    expect((err as WardnetApiError).status).toBe(400);
  });

  it("seeds a unique reservation and blocklist", async () => {
    const reservation = await dhcp.createReservation({
      mac_address: RESERVED_MAC,
      ip_address: RESERVED_IP,
      hostname: RESERVATION_HOSTNAME,
      description: "seed row for the #249 backup round-trip spec",
    });
    expect(reservation.reservation.ip_address).toBe(RESERVED_IP);

    const blocklist = await dnsFilter.createBlocklist(AD_BLOCKING_PROFILE_ID, {
      name: BLOCKLIST_NAME,
      url: BLOCKLIST_URL,
      cron_schedule: CRON_NEVER,
      enabled: false,
    });
    expect(blocklist.blocklist.name).toBe(BLOCKLIST_NAME);

    expect(await seedsPresent()).toEqual({ reservation: true, blocklist: true });
  });

  it("exports an encrypted bundle containing those seeds", async () => {
    bundle = await backup.export({ passphrase: PASSPHRASE });

    // age's armour-free binary output — assert it is substantial and opaque
    // rather than sniffing for a magic header the archiver is free to change.
    expect(bundle.size).toBeGreaterThan(1_024);

    // The plaintext must never appear in the bundle: if the reserved MAC is
    // greppable in the bytes, the age layer silently didn't happen.
    const bytes = Buffer.from(await bundle.arrayBuffer());
    expect(bytes.includes(Buffer.from(RESERVED_MAC, "utf8"))).toBe(false);
  });

  it("refuses to preview the bundle with the wrong passphrase", async () => {
    const err = await backup
      .previewImport(bundle, `${PASSPHRASE}-wrong`)
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(WardnetApiError);
    expect((err as WardnetApiError).status).toBe(400);
  });

  it("loses both seeds when they are deleted", async () => {
    await removeSeeds();
    expect(await seedsPresent()).toEqual({
      reservation: false,
      blocklist: false,
    });
  });

  it("previews the bundle as compatible and returns a token", async () => {
    const preview = await backup.previewImport(bundle, PASSPHRASE);

    expect(preview.compatible).toBe(true);
    expect(preview.incompatibility_reason).toBeUndefined();
    expect(preview.preview_token).not.toBe("");
    // The database is always replaced; the config and secret store come along
    // when present. Asserting non-empty rather than an exact list keeps this
    // from breaking when the bundle gains a component.
    expect(preview.files_to_replace.length).toBeGreaterThan(0);

    // The manifest is the bundle's own provenance record — it must describe
    // *this* daemon, since this daemon produced it moments ago.
    expect(preview.manifest.wardnet_version).not.toBe("");
    expect(preview.manifest.host_id).not.toBe("");
    expect(preview.manifest.schema_version).toBeGreaterThan(0);
    expect(preview.manifest.bundle_format_version).toBeGreaterThan(0);
    expect(Number.isNaN(Date.parse(preview.manifest.created_at))).toBe(false);

    // Nothing on disk changed yet — preview is pure inspection.
    expect(await seedsPresent()).toEqual({
      reservation: false,
      blocklist: false,
    });
  });

  it("rejects a preview token that was never issued", async () => {
    const err = await backup
      .applyImport({ preview_token: "00000000-0000-0000-0000-000000000000" })
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(WardnetApiError);
    expect((err as WardnetApiError).status).toBe(400);
  });

  it(
    "applies the restore, then recovers both seeds after a daemon restart",
    async () => {
      // Fresh token: the one from the preview test above is still inside its
      // 5-minute TTL, but taking a new one keeps this test independent of how
      // long the intervening assertions took on a slow runner.
      const preview = await backup.previewImport(bundle, PASSPHRASE);
      const applied = await backup.applyImport({
        preview_token: preview.preview_token,
      });

      expect(applied.manifest.host_id).toBe(preview.manifest.host_id);
      // The pre-restore state is retained as `.bak-<ts>` siblings so an
      // operator has a manual way back.
      expect(applied.snapshots.length).toBeGreaterThan(0);
      appliedSnapshots = applied.snapshots;
      for (const snapshot of applied.snapshots) {
        expect(["database", "config", "keys"]).toContain(snapshot.kind);
        expect(snapshot.path).toContain(".bak-");
        expect(snapshot.size_bytes).toBeGreaterThan(0);
      }

      // Restart so the pool reopens the restored file. Until it does, the
      // daemon is still serving the renamed pre-restore database.
      const before = await daemonPid(DAEMON_AGENT);
      expect(before.running).toBe(true);
      await new SystemService(admin).restart();
      await waitForDaemonRestart(DAEMON_AGENT, before.pid, 90_000);
      await waitForReady(client, 90_000);

      // Re-authenticate against the restored database. The admin row predates
      // the export so the credentials are unchanged, but the pre-restart
      // session token belonged to a connection pool that is now gone.
      admin = await ensureAdminAndLogin(client);
      backup = new BackupService(admin);
      dhcp = new DhcpService(admin);
      dnsFilter = new DnsFilterService(admin);

      expect(await seedsPresent()).toEqual({
        reservation: true,
        blocklist: true,
      });

      // …and they came back with their contents intact, not just their keys.
      const reservations = await dhcp.listReservations();
      const restored = reservations.reservations.find(
        (r) => r.mac_address.toLowerCase() === RESERVED_MAC,
      );
      expect(restored?.ip_address).toBe(RESERVED_IP);
      expect(restored?.hostname).toBe(RESERVATION_HOSTNAME);

      const blocklists = await dnsFilter.listBlocklists(AD_BLOCKING_PROFILE_ID);
      const restoredList = blocklists.blocklists.find(
        (b) => b.name === BLOCKLIST_NAME,
      );
      expect(restoredList?.url).toBe(BLOCKLIST_URL);
      expect(restoredList?.enabled).toBe(false);
    },
    240_000,
  );

  it("lists the pre-restore snapshots that live beside the database", async () => {
    const { snapshots } = await backup.listSnapshots();
    expect(snapshots.length).toBeGreaterThan(0);
    // The database is always snapshotted, so the listing can never be empty
    // after an apply.
    expect(snapshots.map((s) => s.kind)).toContain("database");

    // `list_snapshots` enumerates exactly one directory — the database's
    // parent (`snapshot_dir()`). The database and secrets snapshots are
    // written there and must all show up; nothing this run created can have
    // been pruned, since the cleanup runner only sweeps siblings older than
    // 24 hours.
    //
    // The *config* snapshot is deliberately excluded: `apply_import` writes it
    // next to the live config (`/etc/wardnet/wardnet.toml.bak-<ts>`), which is
    // a different directory, so `GET /api/backup/snapshots` never reports it
    // and `cleanup_old_snapshots` never prunes it. That asymmetry is a daemon
    // bug, not a property worth asserting — this spec pins the behaviour that
    // exists so the gap is visible here rather than silently passing.
    const listedPaths = snapshots.map((s) => s.path);
    const beside = appliedSnapshots.filter((s) => s.kind !== "config");
    expect(beside.length).toBeGreaterThan(0);
    for (const applied of beside) {
      expect(listedPaths).toContain(applied.path);
    }
  });
});
