import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { UpdateService, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  DAEMON_AGENT,
  agentGet,
  agentPost,
  daemonPid,
  ensureAdminAndLogin,
  pollUntil,
  waitForDaemonRestart,
  waitForReady,
} from "./helpers.js";

/**
 * Auto-update binary swap, end-to-end (issue #319).
 *
 * Covers the transition unit tests cannot reach: the daemon downloads a signed
 * release, stages it, exits, systemd restarts the unit, the privileged
 * `wardnet-postupgrade-runner` re-verifies the tarball as root and renames the
 * new `wardnetd` into `/usr/local/bin/`, and the daemon comes back up on it.
 * The bug fixed in #318 (closing #309) shipped because nothing exercised that
 * chain; #249's `update-status.spec.ts` stops at "a release exists".
 *
 * ## What makes the version transition real
 *
 * `Dockerfile.test` compiles `wardnetd` twice from one source tree: the live
 * install at the image's default version, and a second build at
 * {@link RELEASE_VERSION} that is packaged, signed with the image's ephemeral
 * minisign key, and published by the `update_release_server` compose service.
 * A real previous release cannot be used — both trust anchors (the daemon's
 * `EMBEDDED_PUBLIC_KEY` and the runner's embedded key) are baked in at compile
 * time, so a genuinely-released binary only ever trusts the real release key,
 * which we cannot sign with. See
 * `docs/adr/0027-e2e-auto-update-version-skew.md`.
 *
 * ## Why one file
 *
 * All three scenarios mutate `/usr/local/bin/wardnetd`, which every other spec
 * in the suite runs against. Vitest orders *files* by size on a cold cache and
 * by recorded duration afterwards, so splitting these would leave the ordering
 * — and therefore the binary state each one inherits — to chance. Inside a
 * file, order is guaranteed. Same reasoning as `system-restart.spec.ts`.
 *
 * ## Why nothing here restarts the daemon by hand
 *
 * It does not need to. `run_install` and `rollback` both end in
 * `shutdown_token.cancel()`, so the daemon takes *itself* down and
 * `Restart=always` brings it back — that is the production mechanism, and
 * exercising it is the point. The swap only lands if systemd re-runs
 * `wardnet-postupgrade.service` (`Type=oneshot`, `RemainAfterExit=no`,
 * `RequiredBy=wardnetd.service`) as part of that automatic restart. If it does
 * not, these assertions fail — and they fail because auto-update is broken on
 * real hardware, where the symptom would be a silent `binary swap reverted`
 * log and a `restart_pending` history row on every box.
 */

/** Version compiled into the second `wardnetd` build and advertised by
 * `fixtures/update-manifests/beta.json`. Both must agree: the daemon derives
 * the asset filename as `wardnetd-<version>-<arch>.tar.gz`. */
const RELEASE_VERSION = "9999.12.31";

interface FileDigest {
  path: string;
  exists: boolean;
  size_bytes: number | null;
  sha256: string | null;
}

interface UpdateBinaries {
  live: FileDigest;
  old: FileDigest;
  pending_tarball: FileDigest;
  pending_signature: FileDigest;
}

interface UnitStatus {
  unit: string;
  active_state: string;
  result: string;
  exec_main_status: string;
}

interface PostupgradeState {
  applied: Array<{ id: string; applied_at: string }>;
  failed: Array<{ id: string; error: string; at: string }>;
  last_verification_failure?: { error: string; at: string } | null;
  last_runner_failure?: { stage: string; error: string; at: string } | null;
}

const binaries = () => agentGet<UpdateBinaries>(DAEMON_AGENT, "/update/binaries");

const restartUnit = (unit: string) =>
  agentPost<{ unit: string; exit_code: number; stdout: string; stderr: string }>(
    DAEMON_AGENT,
    `/systemd/restart?unit=${encodeURIComponent(unit)}`,
    {},
  );

const unitStatus = (unit: string) =>
  agentGet<UnitStatus>(DAEMON_AGENT, `/systemd/status?unit=${encodeURIComponent(unit)}`);

describe("auto-update binary swap (issue #319)", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  let admin: AuthedClient;
  let update: UpdateService;

  /** Digest of the binary the container booted with. Every scenario is
   * measured against this, and the `afterAll` restores it. */
  let originalBinarySha = "";
  let originalVersion = "";

  beforeAll(async () => {
    await waitForReady(client);
    admin = await ensureAdminAndLogin(client);
    update = new UpdateService(admin);

    const { status } = await update.status();
    originalVersion = status.current_version;

    const before = await binaries();
    expect(before.live.exists).toBe(true);
    originalBinarySha = before.live.sha256 ?? "";
    expect(originalBinarySha).not.toBe("");

    // A re-run against a persistent volume could inherit a half-finished
    // install from a previous run. Start from a known-clean staging dir.
    await agentPost(DAEMON_AGENT, "/update/clear-staged", {});
  }, 180_000);

  afterAll(async () => {
    // Unconditional and idempotent. On the happy path the rollback scenario
    // has already consumed the `.old` binary, so this is a no-op. It exists
    // for the partial-failure case: exiting this file with the swapped-in
    // 9999.12.31 binary live would flip `update_available` to false and break
    // `update-status.spec.ts` in whatever slot vitest scheduled it — a failure
    // attributed to the wrong file entirely.
    try {
      await agentPost(DAEMON_AGENT, "/update/clear-staged", {});
      const restored = await agentPost<{ restored: boolean }>(
        DAEMON_AGENT,
        "/update/restore-original",
        {},
      );
      if (restored.restored) {
        await restartUnit("wardnetd.service");
      }
      await waitForReady(client, 120_000);
      await update.updateConfig({ channel: "stable", auto_update_enabled: false });
    } catch {
      // Best-effort: a throw here would mask whichever assertion actually
      // failed above.
    }
  }, 180_000);

  describe("swap", () => {
    it(
      "installs the fixture release and comes back up on the new binary",
      async () => {
        const switched = await update.updateConfig({ channel: "beta" });
        expect(switched.status.channel).toBe("beta");

        const checked = await update.check();
        expect(checked.status.latest_version).toBe(RELEASE_VERSION);
        expect(checked.status.update_available).toBe(true);

        const before = await daemonPid(DAEMON_AGENT);
        expect(before.running).toBe(true);

        // Returns as soon as the install is claimed. The pipeline runs in a
        // spawned task and ends by cancelling the shutdown token, so the
        // daemon going away *is* the success signal.
        const { handle } = await update.install();
        expect(handle.target_version).toBe(RELEASE_VERSION);

        // Download + sha256 + minisign verify + stage, then exit, then
        // RestartSec=2s, then the post-upgrade runner's own verify and the
        // rename, then a full daemon startup. Generous so a slow CI runner
        // does not read as a failure to swap.
        await waitForDaemonRestart(DAEMON_AGENT, before.pid, 180_000);
        await waitForReady(client, 180_000);

        const after = await binaries();
        expect(after.live.exists).toBe(true);
        expect(after.live.sha256).not.toBe(originalBinarySha);

        // The outgoing binary must be preserved — `wardnetd-rollback.service`
        // and the rollback scenario below both depend on it.
        expect(after.old.exists).toBe(true);
        expect(after.old.sha256).toBe(originalBinarySha);

        // `perform_swap` removes the staged pair once the rename lands, so a
        // leftover here would mean the next boot re-applies the same update.
        expect(after.pending_tarball.exists).toBe(false);
        expect(after.pending_signature.exists).toBe(false);
      },
      300_000,
    );

    it("reports the new version and settles the pending marker", async () => {
      const { status } = await update.status();

      // The assertion that separates a real swap from a silent revert: the
      // daemon answering here is the *new* build.
      expect(status.current_version).toBe(RELEASE_VERSION);
      expect(status.pending_version).toBeNull();
      expect(status.install_phase).toEqual({ phase: "applied" });
      expect(status.applied_version).toBe(RELEASE_VERSION);
      expect(status.applied_at).not.toBeNull();

      // Nothing newer than what we are now running is on the channel.
      expect(status.update_available).toBe(false);
      expect(status.rollback_available).toBe(true);
    });

    it("records the install as succeeded, with no reverted-swap row", async () => {
      const { entries } = await update.history(10);

      const install = entries.find((e) => e.to_version === RELEASE_VERSION);
      expect(install).toBeDefined();
      expect(install?.status).toBe("succeeded");
      expect(install?.from_version).toBe(originalVersion);

      // `do_reconcile_pending_install` writes a `restart_pending` / failed row
      // when the daemon comes back as something other than what it staged.
      // That row is the exact signature of the swap silently not happening,
      // so its absence is the assertion — not just the presence of a success.
      const reverted = entries.filter(
        (e) => e.phase === "restart_pending" && e.status === "failed",
      );
      expect(reverted).toEqual([]);
    });

    it("re-ran the post-upgrade runner cleanly on the restart", async () => {
      // The swap happens inside the runner, so a clean state file after the
      // restart is what confirms the privileged step ran and completed rather
      // than being skipped.
      const state = await agentGet<PostupgradeState>(
        DAEMON_AGENT,
        "/postupgrade/state",
      );
      expect(state.failed).toEqual([]);
      expect(state.last_verification_failure ?? null).toBeNull();
    });
  });

  describe("rollback", () => {
    it(
      "restores the previous binary through the runner's rollback path",
      async () => {
        const before = await daemonPid(DAEMON_AGENT);

        // Writes `wardnetd-rollback.request` and cancels the shutdown token.
        // The rename itself is the runner's job on the way back up — the
        // daemon runs unprivileged and cannot write /usr/local/bin.
        const { message } = await update.rollback();
        expect(message).toContain("rollback");

        await waitForDaemonRestart(DAEMON_AGENT, before.pid, 180_000);
        await waitForReady(client, 180_000);

        const after = await binaries();
        expect(after.live.sha256).toBe(originalBinarySha);
        // `perform_rollback` renames `.old` over `<live>` rather than copying,
        // so the backup is consumed.
        expect(after.old.exists).toBe(false);
      },
      300_000,
    );

    it("reports the original version again", async () => {
      const { status } = await update.status();
      expect(status.current_version).toBe(originalVersion);
      expect(status.rollback_available).toBe(false);
      // The "we just updated" announcement is retracted — we deliberately
      // left that version.
      expect(status.applied_version).toBeNull();

      // And the release is on offer again, since we are back below it.
      expect(status.update_available).toBe(true);
    });

    it("records a rolled_back history entry", async () => {
      const { entries } = await update.history(10);
      expect(entries.some((e) => e.status === "rolled_back")).toBe(true);
    });
  });

  describe("trust anchor", () => {
    /**
     * This scenario tests the *runner*, not the daemon's install pipeline.
     *
     * The corrupt tarball is staged directly rather than served, because the
     * daemon and the runner are built against the same key in this image: a
     * wrongly-signed download would be rejected by the daemon's own
     * `verify_signature` at `InstallPhase::Verifying` and would never reach
     * the runner. Writing it straight into the staging directory is what
     * `swap.rs`'s threat model actually describes — the wardnet user can stage
     * any bytes it likes, and only the runner's embedded key decides what runs.
     *
     * `wardnetd.service`'s `OnFailure=wardnetd-rollback.service` hook does not
     * interfere. It fires when the unit enters `failed`, whereas a refused
     * `Requires=` dependency leaves the unit inactive with a dependency job
     * result. And even if it did fire, the rollback scenario above has already
     * consumed `<live>.old`, so the unit's own guard would log "no backup
     * binary found" and exit 0 without touching the binary.
     */
    it(
      "refuses to swap a tarball it cannot verify, and blocks daemon startup",
      async () => {
        const staged = await agentPost<{ staged: boolean }>(
          DAEMON_AGENT,
          "/update/stage-bad-signature",
          {},
        );
        expect(staged.staged).toBe(true);

        const restart = await restartUnit("wardnetd.service");
        // systemd refuses the start because the required oneshot exited
        // nonzero, so `systemctl restart` itself reports failure.
        expect(restart.exit_code).not.toBe(0);

        const daemon = await pollUntil(
          () => unitStatus("wardnetd.service"),
          (s) => s.active_state !== "activating",
          {
            timeoutMs: 60_000,
            describe: (last) =>
              `wardnetd.service never settled (last=${JSON.stringify(last)})`,
          },
        );
        expect(daemon.active_state).not.toBe("active");

        // The runner is what actually refused. Assert on `Result` rather than
        // only on `ExecMainStatus`: a missing property reads back as an empty
        // string, which would satisfy a bare `not.toBe("0")` even if the unit
        // had never run at all.
        const runner = await unitStatus("wardnet-postupgrade.service");
        expect(runner.result).toBe("exit-code");
        expect(runner.exec_main_status).not.toBe("");
        expect(runner.exec_main_status).not.toBe("0");

        const after = await binaries();
        // The whole point: unverifiable bytes changed nothing.
        expect(after.live.sha256).toBe(originalBinarySha);
        // And they are left on disk for an operator to inspect rather than
        // being quietly discarded.
        expect(after.pending_tarball.exists).toBe(true);
        expect(after.pending_signature.exists).toBe(true);
      },
      180_000,
    );

    it(
      "recovers once the unverifiable tarball is cleared",
      async () => {
        // Deliberately the last assertion in the file: the stack is wedged
        // until this runs, so recovery is verified here rather than surfacing
        // as a mystery timeout in whatever spec runs next.
        const cleared = await agentPost<{ removed: string[] }>(
          DAEMON_AGENT,
          "/update/clear-staged",
          {},
        );
        expect(cleared.removed.length).toBeGreaterThan(0);

        const restart = await restartUnit("wardnetd.service");
        expect(restart.exit_code).toBe(0);

        await waitForReady(client, 180_000);
        const { status } = await update.status();
        expect(status.current_version).toBe(originalVersion);
      },
      300_000,
    );
  });
});
