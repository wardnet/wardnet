import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  AuthService,
  LogService,
  WardnetClient,
  type LogEntry,
  type LogStreamCallbacks,
} from "@wardnet/js";

import {
  ADMIN_PASSWORD,
  ADMIN_USERNAME,
  API_BASE_URL,
  AuthedClient,
  ensureAdminAndLogin,
  waitForReady,
} from "./helpers.js";

/**
 * The daemon's live log stream, exercised through the SDK's `LogService`.
 *
 * `LogService` is written for the browser: it opens a bare
 * `new WebSocket(url)` and leans on the same-origin `wardnet_session`
 * cookie for auth. Node has no cookie jar, and the daemon gates
 * `GET /api/system/logs/stream` behind `AdminAuth` (session cookie *or*
 * `Authorization: Bearer <token>`), so the only non-browser way onto the
 * socket is a header on the upgrade handshake. Node's global `WebSocket`
 * is undici's, whose constructor accepts a non-standard init object with
 * `headers` — so we wrap the global for the lifetime of this file to pin
 * the bearer onto every handshake, then restore it in `afterAll`. Wrapping
 * (rather than passing headers through `LogService`) is deliberate: it
 * keeps the SDK surface identical to what the admin UI actually calls.
 *
 * Log entries reach the stream from the daemon's tracing layer. The e2e
 * stack runs with `wardnetd_api=debug`, and every HTTP request emits a
 * `debug` "response" event under the `wardnetd_api` target (the
 * `TraceLayer` on the API router). That gives us a deterministic entry
 * source: hit any endpoint with the filter at `debug` and an entry shows
 * up. Assertions drive traffic themselves rather than trusting incidental
 * background logging to land inside a timeout.
 */
describe("logs stream (WebSocket)", () => {
  let client: WardnetClient;
  let authed: AuthedClient;
  let realWebSocket: typeof globalThis.WebSocket;

  beforeAll(async () => {
    client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    // Ensure an admin exists and the wizard is drained, then take a fresh
    // token we own — `ensureAdminAndLogin` keeps its token private, and we
    // need the raw value to sign the WebSocket handshake.
    await ensureAdminAndLogin(client);
    const { token } = await new AuthService(client).login({
      username: ADMIN_USERNAME,
      password: ADMIN_PASSWORD,
    });
    authed = new AuthedClient(API_BASE_URL, token);

    realWebSocket = globalThis.WebSocket;
    globalThis.WebSocket = class extends realWebSocket {
      constructor(url: string | URL) {
        super(url, { headers: { Authorization: `Bearer ${token}` } });
      }
    };
  }, 180_000);

  afterAll(() => {
    // Undo the global patch so no later spec inherits an authed socket —
    // vitest shares the module graph across files here (`isolate: false`).
    globalThis.WebSocket = realWebSocket;
  });

  it("delivers at least one entry over the socket", async () => {
    const sink = newSink();
    const log = new LogService(authed);
    try {
      log.connect(sink.callbacks, { level: "debug" });
      await waitUntil(() => sink.connects > 0, { timeoutMs: 10_000 });
      expect(sink.connects, "socket never reported open").toBeGreaterThan(0);

      await waitUntil(() => sink.entries.length > 0, {
        timeoutMs: 15_000,
        activity: () => pokeDaemon(client),
      });
      expect(
        sink.entries.length,
        "no log entries arrived despite generating request traffic",
      ).toBeGreaterThan(0);

      // The wire shape the UI depends on: a timestamped, levelled, targeted
      // entry. Don't pin the message text — which event wins the race is
      // nondeterministic — only that the structural fields are populated.
      const entry = sink.entries[0];
      expect(typeof entry.timestamp).toBe("string");
      expect(entry.timestamp.length).toBeGreaterThan(0);
      expect(entry.level.length).toBeGreaterThan(0);
      expect(typeof entry.target).toBe("string");
    } finally {
      log.disconnect();
    }
  });

  it("round-trips a set_filter command in both directions", async () => {
    const sink = newSink();
    const log = new LogService(authed);
    try {
      log.connect(sink.callbacks, { level: "debug" });
      await waitUntil(() => sink.entries.length > 0, {
        timeoutMs: 15_000,
        activity: () => pokeDaemon(client),
      });
      expect(sink.entries.length).toBeGreaterThan(0);

      // Narrow to a target prefix nothing emits under. The daemon's filter
      // is a `starts_with` on the entry target, so this rejects every event
      // regardless of level — a clean way to prove the command took effect
      // without racing a specific target's cadence.
      log.setFilter({ level: "trace", target: "no.such.target.zzz" });
      // Let any entries already in flight before the server processed the
      // command drain, then snapshot a quiet baseline.
      await sleep(1_000);
      const quietBaseline = sink.entries.length;
      await drive(2_000, () => pokeDaemon(client));
      expect(
        sink.entries.length,
        "entries kept arriving after narrowing the filter to an impossible target",
      ).toBe(quietBaseline);

      // Widen back to every target (empty prefix matches all). Level is left
      // untouched, so it stays at the `trace` set above.
      log.setFilter({ target: "" });
      await waitUntil(() => sink.entries.length > quietBaseline, {
        timeoutMs: 15_000,
        activity: () => pokeDaemon(client),
      });
      expect(
        sink.entries.length,
        "entries did not resume after widening the filter",
      ).toBeGreaterThan(quietBaseline);
    } finally {
      log.disconnect();
    }
  });

  it("pauses and resumes the stream", async () => {
    const sink = newSink();
    const log = new LogService(authed);
    try {
      log.connect(sink.callbacks, { level: "debug" });
      await waitUntil(() => sink.entries.length > 0, {
        timeoutMs: 15_000,
        activity: () => pokeDaemon(client),
      });
      expect(sink.entries.length).toBeGreaterThan(0);

      // Pause closes the socket; the daemon drops the subscriber, so nothing
      // can reach us regardless of how much traffic we generate.
      log.pause();
      await waitUntil(() => sink.disconnects > 0, { timeoutMs: 10_000 });
      expect(sink.disconnects, "pause did not close the socket").toBeGreaterThan(0);

      const pausedBaseline = sink.entries.length;
      await drive(2_000, () => pokeDaemon(client));
      expect(
        sink.entries.length,
        "entries arrived while the stream was paused",
      ).toBe(pausedBaseline);

      // Resume reopens the socket and replays the stored filter, so the
      // stream comes back at the same `debug` level it started on.
      const connectsBeforeResume = sink.connects;
      log.resume();
      await waitUntil(() => sink.connects > connectsBeforeResume, {
        timeoutMs: 10_000,
      });
      await waitUntil(() => sink.entries.length > pausedBaseline, {
        timeoutMs: 15_000,
        activity: () => pokeDaemon(client),
      });
      expect(
        sink.entries.length,
        "entries did not resume after resume()",
      ).toBeGreaterThan(pausedBaseline);
    } finally {
      log.disconnect();
    }
  });
});

/** Collected stream state plus the callbacks that feed it. */
interface Sink {
  entries: LogEntry[];
  connects: number;
  disconnects: number;
  lagged: number;
  callbacks: LogStreamCallbacks;
}

function newSink(): Sink {
  const sink: Sink = {
    entries: [],
    connects: 0,
    disconnects: 0,
    lagged: 0,
    callbacks: {
      onEntry: (entry) => {
        sink.entries.push(entry);
      },
      onLagged: (skipped) => {
        sink.lagged += skipped;
      },
      onConnected: () => {
        sink.connects += 1;
      },
      onDisconnected: () => {
        sink.disconnects += 1;
      },
    },
  };
  return sink;
}

const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Generate one unit of daemon activity. `/api/info` is unauthenticated and
 * cheap; each call still traverses the API router's `TraceLayer`, which
 * emits the `debug` "response" event we're streaming. Errors are swallowed
 * so a transient blip doesn't fail the poll loop that calls this.
 */
async function pokeDaemon(client: WardnetClient): Promise<void> {
  try {
    await client.request("/info");
  } catch {
    // Ignore — the surrounding assertion owns the timeout.
  }
}

/**
 * Poll `predicate` until it holds or the timeout elapses. When `activity`
 * is supplied it runs each tick to keep entries flowing while we wait.
 * Returns the predicate's final value rather than throwing so callers keep
 * ownership of the assertion (and its failure message).
 */
async function waitUntil(
  predicate: () => boolean,
  opts: {
    timeoutMs?: number;
    intervalMs?: number;
    activity?: () => Promise<void>;
  } = {},
): Promise<boolean> {
  const { timeoutMs = 15_000, intervalMs = 250, activity } = opts;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return true;
    if (activity) await activity();
    await sleep(intervalMs);
  }
  return predicate();
}

/** Run `activity` on a fixed cadence for `durationMs`, then return. Used by
 *  the "should stay quiet" assertions, which need sustained traffic to prove
 *  a filter or pause is actually holding entries back. */
async function drive(
  durationMs: number,
  activity: () => Promise<void>,
): Promise<void> {
  const deadline = Date.now() + durationMs;
  while (Date.now() < deadline) {
    await activity();
    await sleep(250);
  }
}
