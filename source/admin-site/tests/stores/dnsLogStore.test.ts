import { beforeEach, describe, expect, it } from "vitest";
import type { QueryLogEvent } from "@wardnet/js";
import { useDnsLogStore } from "@/stores/dnsLogStore";

function event(overrides: Partial<QueryLogEvent> = {}): QueryLogEvent {
  return {
    timestamp: "2026-01-01T00:00:00Z",
    domain: "example.com",
    client_ip: "10.0.0.1",
    result: "allowed",
    ...overrides,
  } as QueryLogEvent;
}

describe("dnsLogStore", () => {
  beforeEach(() => {
    useDnsLogStore.setState({
      entries: [],
      connected: false,
      paused: false,
      skipped: 0,
      filter: { domain: "", client_ip: "", device_id: "", results: [] },
    });
  });

  it("prepends events when not paused", () => {
    useDnsLogStore.getState()._addEvent(event({ domain: "a.com" }));
    useDnsLogStore.getState()._addEvent(event({ domain: "b.com" }));
    expect(useDnsLogStore.getState().entries[0].domain).toBe("b.com");
  });

  it("drops events while paused", () => {
    useDnsLogStore.getState().setPaused(true);
    useDnsLogStore.getState()._addEvent(event());
    expect(useDnsLogStore.getState().entries).toHaveLength(0);
  });

  it("caps entries at MAX_ENTRIES (500)", () => {
    for (let i = 0; i < 510; i++) {
      useDnsLogStore.getState()._addEvent(event({ domain: `d${i}.com` }));
    }
    expect(useDnsLogStore.getState().entries).toHaveLength(500);
  });

  it("setFilter keeps existing entries", () => {
    useDnsLogStore.getState()._addEvent(event());
    useDnsLogStore.getState().setFilter({
      domain: "x",
      client_ip: "",
      device_id: "",
      results: ["blocked"],
    });
    const s = useDnsLogStore.getState();
    expect(s.filter.domain).toBe("x");
    expect(s.entries).toHaveLength(1);
  });

  it("clear resets entries and skipped; connection + skipped tracked", () => {
    useDnsLogStore.getState()._setConnected(true);
    useDnsLogStore.getState()._addSkipped(2);
    useDnsLogStore.getState()._addEvent(event());
    useDnsLogStore.getState().clear();
    const s = useDnsLogStore.getState();
    expect(s.connected).toBe(true);
    expect(s.entries).toHaveLength(0);
    expect(s.skipped).toBe(0);
  });
});
