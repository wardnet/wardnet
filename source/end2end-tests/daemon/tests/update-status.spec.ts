import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { UpdateService, WardnetApiError, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  ensureAdminAndLogin,
  waitForReady,
} from "./helpers.js";

// Versions published by the `update_manifest_server` fixture (see
// fixtures/update-manifests/). `stable.json` is the generator's empty
// placeholder — "nothing released on this channel". `beta.json` advertises a
// version chosen to outrank anything this repo will ever build, so
// `update_available` is deterministic regardless of the CALVER the image was
// compiled with.
const FIXTURE_BETA_VERSION = "9999.12.31";

/**
 * Auto-update status / check / channel switching against a fixture manifest
 * server (issue #249, E2E Stage 11).
 *
 * The daemon's `[update] manifest_base_url` is rewritten to
 * `http://10.92.0.56` by the compose entrypoint, so every check in this file
 * resolves against `fixtures/update-manifests/<channel>.json` and never
 * touches releases.wardnet.network.
 *
 * Nothing here installs. `auto_update_enabled` is left at its default
 * (`false`) throughout — flipping it on with a fixture release visible would
 * hand the background `UpdateRunner` a download of an asset that does not
 * exist, which is a different (and much heavier) spec than this one.
 */
describe("update status + channel (issue #249)", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  let admin: AuthedClient;
  let update: UpdateService;

  beforeAll(async () => {
    await waitForReady(client);
    admin = await ensureAdminAndLogin(client);
    update = new UpdateService(admin);

    // Other specs don't touch the update subsystem, but a re-run against a
    // persistent volume would inherit whatever channel the previous run left
    // behind. Normalise before asserting the default-channel behaviour.
    await update.updateConfig({ channel: "stable", auto_update_enabled: false });
  }, 120_000);

  afterAll(async () => {
    // Leave the box on stable with auto-update off, so a subsequent run — and
    // any spec that later reads system state — sees the shipped defaults.
    try {
      await update.updateConfig({
        channel: "stable",
        auto_update_enabled: false,
      });
    } catch {
      // ignore
    }
  });

  it("reports a resting status on the stable channel", async () => {
    const { status } = await update.status();

    expect(status.channel).toBe("stable");
    expect(status.auto_update_enabled).toBe(false);
    expect(status.current_version).not.toBe("");
    expect(status.install_phase).toEqual({ phase: "idle" });
    expect(status.pending_version).toBeNull();
    // `allow_edge_channel` is deliberately absent from the e2e TOML — putting
    // a box on edge must require root on the box, not just an admin session.
    expect(status.edge_available).toBe(false);
  });

  it("reports no update available when the channel manifest is a placeholder", async () => {
    const { status } = await update.check();

    // stable.json carries `version: ""` and `binary: null`, which the daemon
    // reads as "no release published" rather than as a fetch failure.
    expect(status.latest_version).toBeNull();
    expect(status.update_available).toBe(false);
    // A check always records its timestamp, even when it finds nothing.
    expect(status.last_check_at).not.toBeNull();
    expect(Number.isNaN(Date.parse(status.last_check_at!))).toBe(false);
  });

  it("picks up the fixture release after switching to the beta channel", async () => {
    const switched = await update.updateConfig({ channel: "beta" });
    expect(switched.status.channel).toBe("beta");
    // Switching channels invalidates the version resolved against the old
    // one — it was never a beta version and must not be offered as one.
    expect(switched.status.latest_version).toBeNull();

    const { status } = await update.check();
    expect(status.channel).toBe("beta");
    expect(status.latest_version).toBe(FIXTURE_BETA_VERSION);
    expect(status.update_available).toBe(true);
    // Discovering a release is not installing one.
    expect(status.install_phase).toEqual({ phase: "idle" });
    expect(status.pending_version).toBeNull();
    expect(status.auto_update_enabled).toBe(false);
  });

  it("persists the resolved version across a plain status read", async () => {
    // `status` reads the stored `update_last_known_version`; it does not
    // re-fetch. This is what the Settings UI polls, so it must agree with the
    // check that preceded it.
    const { status } = await update.status();
    expect(status.channel).toBe("beta");
    expect(status.latest_version).toBe(FIXTURE_BETA_VERSION);
    expect(status.update_available).toBe(true);
  });

  it("clears the beta finding when switching back to stable", async () => {
    const { status } = await update.updateConfig({ channel: "stable" });
    expect(status.channel).toBe("stable");
    expect(status.latest_version).toBeNull();
    expect(status.update_available).toBe(false);
  });

  it("refuses the edge channel on a box without the deploy-time flag", async () => {
    const err = await update
      .updateConfig({ channel: "edge" })
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(WardnetApiError);
    expect((err as WardnetApiError).status).toBe(403);

    // The rejection must be total: a body carrying both a channel and an
    // auto-update flag is validated before anything is written.
    const rejected = await update
      .updateConfig({ channel: "edge", auto_update_enabled: true })
      .catch((e: unknown) => e);
    expect(rejected).toBeInstanceOf(WardnetApiError);

    const { status } = await update.status();
    expect(status.channel).toBe("stable");
    expect(status.auto_update_enabled).toBe(false);
  });

  it("exposes an install history endpoint", async () => {
    // Nothing in the e2e stack installs, so the list is expected to be empty —
    // the assertion is that the endpoint is wired and returns the envelope,
    // not that it has content.
    const { entries } = await update.history(5);
    expect(Array.isArray(entries)).toBe(true);
  });

  it("rejects unauthenticated access to the update surface", async () => {
    const anon = new UpdateService(new WardnetClient({ baseUrl: API_BASE_URL }));
    const err = await anon.status().catch((e: unknown) => e);
    expect(err).toBeInstanceOf(WardnetApiError);
    expect((err as WardnetApiError).status).toBe(401);
  });
});
