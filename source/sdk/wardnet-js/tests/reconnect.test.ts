import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WardnetClient } from "../src/client.js";
import { LogService } from "../src/services/logs.js";
import { DnsLogStreamService } from "../src/services/dnsLogStream.js";

/**
 * Minimal WebSocket stub. The real global never opens on its own, so a test
 * drives the lifecycle explicitly: `close()` fires `onclose` (simulating a
 * daemon that is down and rejects the handshake), `open()` fires `onopen`.
 */
class MockWebSocket {
  static readonly OPEN = 1;
  static instances: MockWebSocket[] = [];

  readyState = 0;
  sent: string[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(readonly url: string) {
    MockWebSocket.instances.push(this);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.readyState = 3;
    this.onclose?.();
  }

  /** Simulate a successful handshake. */
  open(): void {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
  }

  static last(): MockWebSocket {
    const ws = MockWebSocket.instances.at(-1);
    if (!ws) throw new Error("no WebSocket was constructed");
    return ws;
  }
}

/** Captured pending timers — we fire them by hand instead of by wall clock. */
interface PendingTimer {
  id: number;
  fn: () => void;
  delay: number;
}
let timers: PendingTimer[] = [];
let nextTimerId = 1;

function lastDelay(): number {
  const timer = timers.at(-1);
  if (!timer) throw new Error("no timer was scheduled");
  return timer.delay;
}

/** Fire the most recently scheduled reconnect timer. */
function fireLastTimer(): void {
  const timer = timers.at(-1);
  if (!timer) throw new Error("no timer was scheduled");
  timer.fn();
}

const client = { baseUrl: "http://localhost:7411/api" } as unknown as WardnetClient;

/**
 * Both stream services compose the same `ReconnectingStream`, so the backoff
 * contract is asserted against each public service to guarantee neither drifts
 * back to a flat-timer reconnect.
 */
const cases = [
  {
    name: "LogService",
    endpoint: "ws://localhost:7411/api/system/logs/stream",
    // The JSON frame setFilter() is expected to push over the open socket.
    filterPayload: { type: "set_filter", level: "debug" },
    create: () => {
      const service = new LogService(client, "http://localhost:7411");
      return {
        connect: () =>
          service.connect({
            onEntry: vi.fn(),
            onLagged: vi.fn(),
            onConnected: vi.fn(),
            onDisconnected: vi.fn(),
          }),
        setFilter: () => service.setFilter({ level: "debug" }),
        pause: () => service.pause(),
        resume: () => service.resume(),
        disconnect: () => service.disconnect(),
      };
    },
  },
  {
    name: "DnsLogStreamService",
    endpoint: "ws://localhost:7411/api/dns/log/stream",
    filterPayload: { type: "set_filter", domain: "example.com" },
    create: () => {
      const service = new DnsLogStreamService(client, "http://localhost:7411");
      return {
        connect: () =>
          service.connect({
            onEvent: vi.fn(),
            onLagged: vi.fn(),
            onConnected: vi.fn(),
            onDisconnected: vi.fn(),
          }),
        setFilter: () => service.setFilter({ domain: "example.com" }),
        pause: () => service.pause(),
        resume: () => service.resume(),
        disconnect: () => service.disconnect(),
      };
    },
  },
];

beforeEach(() => {
  MockWebSocket.instances = [];
  timers = [];
  nextTimerId = 1;

  vi.stubGlobal("WebSocket", MockWebSocket);
  vi.stubGlobal("setTimeout", (fn: () => void, delay: number) => {
    const id = nextTimerId++;
    timers.push({ id, fn, delay });
    return id;
  });
  vi.stubGlobal("clearTimeout", (id: number) => {
    timers = timers.filter((timer) => timer.id !== id);
  });
  // Freeze jitter at its midpoint (0.75 + 0.5*0.5 = 1.0) so the backoff delay
  // equals the raw exponential term and is exactly assertable. Jitter spread
  // is covered separately below.
  vi.spyOn(Math, "random").mockReturnValue(0.5);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe.each(cases)("$name reconnect backoff", ({ endpoint, filterPayload, create }) => {
  it("connects to the stream endpoint", () => {
    const h = create();
    h.connect();
    expect(MockWebSocket.last().url).toBe(endpoint);
  });

  it("grows the reconnect delay exponentially and caps it at RECONNECT_MAX_MS", () => {
    const h = create();
    h.connect();

    const delays: number[] = [];
    // Each iteration: the open handshake fails (socket closes) → a reconnect is
    // scheduled → fire it to attempt the next connection, which fails again.
    for (let i = 0; i < 7; i++) {
      MockWebSocket.last().close();
      delays.push(lastDelay());
      fireLastTimer();
    }

    expect(delays).toEqual([1000, 2000, 4000, 8000, 16000, 30000, 30000]);
  });

  it("resets the delay after a successful open", () => {
    const h = create();
    h.connect();

    MockWebSocket.last().close();
    expect(lastDelay()).toBe(1000);
    fireLastTimer();
    MockWebSocket.last().close();
    expect(lastDelay()).toBe(2000);
    fireLastTimer();

    // Handshake finally succeeds — the attempt counter must reset.
    MockWebSocket.last().open();
    MockWebSocket.last().close();
    expect(lastDelay()).toBe(1000);
  });

  it("resets the delay on resume()", () => {
    const h = create();
    h.connect();

    MockWebSocket.last().close();
    fireLastTimer();
    MockWebSocket.last().close();
    expect(lastDelay()).toBe(2000);
    fireLastTimer();

    h.pause();
    h.resume();
    MockWebSocket.last().close();
    expect(lastDelay()).toBe(1000);
  });

  it("resets the delay on a fresh connect()", () => {
    const h = create();
    h.connect();

    MockWebSocket.last().close();
    fireLastTimer();
    MockWebSocket.last().close();
    expect(lastDelay()).toBe(2000);
    fireLastTimer();

    h.connect();
    MockWebSocket.last().close();
    expect(lastDelay()).toBe(1000);
  });

  it("does not schedule a reconnect while paused", () => {
    const h = create();
    h.connect();

    h.pause();
    const scheduledBefore = timers.length;
    MockWebSocket.last().close();
    expect(timers.length).toBe(scheduledBefore);
  });

  it("keeps the jittered delay within ±25% of the exponential term", () => {
    vi.spyOn(Math, "random").mockReturnValueOnce(0).mockReturnValueOnce(1);

    const h = create();
    h.connect();

    // First attempt (exp = 1000) with random() = 0 → 0.75 factor.
    MockWebSocket.last().close();
    expect(lastDelay()).toBeCloseTo(750);
    fireLastTimer();

    // Second attempt (exp = 2000) with random() = 1 → 1.25 factor.
    MockWebSocket.last().close();
    expect(lastDelay()).toBeCloseTo(2500);
  });

  it("pushes a filter change over the open socket", () => {
    const h = create();
    h.connect();
    MockWebSocket.last().open();

    h.setFilter();

    const frames = MockWebSocket.last().sent.map((raw) => JSON.parse(raw));
    expect(frames).toContainEqual(filterPayload);
  });

  it("clears a pending reconnect on disconnect()", () => {
    const h = create();
    h.connect();

    MockWebSocket.last().close();
    expect(timers.length).toBe(1);

    h.disconnect();
    expect(timers.length).toBe(0);
  });
});
