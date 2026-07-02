import { beforeEach, describe, expect, it } from "vitest";
import type { LogEntry } from "@wardnet/js";
import { useLogStore } from "@/stores/logStore";

function entry(overrides: Partial<LogEntry> = {}): LogEntry {
  return {
    timestamp: "2026-01-01T00:00:00Z",
    level: "info",
    message: "hi",
    ...overrides,
  } as LogEntry;
}

describe("logStore", () => {
  beforeEach(() => {
    useLogStore.setState({
      entries: [],
      connected: false,
      paused: false,
      skipped: 0,
      filter: { level: "info" },
    });
  });

  it("prepends entries", () => {
    useLogStore.getState()._addEntry(entry({ message: "first" }));
    useLogStore.getState()._addEntry(entry({ message: "second" }));
    const { entries } = useLogStore.getState();
    expect(entries[0].message).toBe("second");
    expect(entries).toHaveLength(2);
  });

  it("caps entries at MAX_ENTRIES (250)", () => {
    for (let i = 0; i < 260; i++) {
      useLogStore.getState()._addEntry(entry({ message: `m${i}` }));
    }
    expect(useLogStore.getState().entries).toHaveLength(250);
  });

  it("tracks connection and skipped counts", () => {
    useLogStore.getState()._setConnected(true);
    expect(useLogStore.getState().connected).toBe(true);
    useLogStore.getState()._addSkipped(3);
    useLogStore.getState()._addSkipped(2);
    expect(useLogStore.getState().skipped).toBe(5);
  });

  it("setFilter resets entries and skipped", () => {
    useLogStore.getState()._addEntry(entry());
    useLogStore.getState()._addSkipped(4);
    useLogStore.getState().setFilter({ level: "error" });
    const s = useLogStore.getState();
    expect(s.filter.level).toBe("error");
    expect(s.entries).toHaveLength(0);
    expect(s.skipped).toBe(0);
  });

  it("clear empties entries and setPaused toggles", () => {
    useLogStore.getState()._addEntry(entry());
    useLogStore.getState().clear();
    expect(useLogStore.getState().entries).toHaveLength(0);
    useLogStore.getState().setPaused(true);
    expect(useLogStore.getState().paused).toBe(true);
  });
});
