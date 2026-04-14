import { create } from "zustand";
import type { LogEntry, LogFilter } from "@wardnet/js";

const MAX_ENTRIES = 250;

interface LogState {
  entries: LogEntry[];
  connected: boolean;
  paused: boolean;
  skipped: number;
  filter: LogFilter;

  /** Internal — called by the connection manager. */
  _addEntry: (entry: LogEntry) => void;
  _setConnected: (connected: boolean) => void;
  _addSkipped: (count: number) => void;

  /** Update the filter. The connection manager reads this and sends to server. */
  setFilter: (filter: LogFilter) => void;
  clear: () => void;
  setPaused: (paused: boolean) => void;
}

export const useLogStore = create<LogState>((set) => ({
  entries: [],
  connected: false,
  paused: false,
  skipped: 0,
  filter: { level: "info" },

  _addEntry: (entry) =>
    set((state) => {
      const next = [entry, ...state.entries];
      return { entries: next.length > MAX_ENTRIES ? next.slice(0, MAX_ENTRIES) : next };
    }),

  _setConnected: (connected) => set({ connected }),
  _addSkipped: (count) => set((state) => ({ skipped: state.skipped + count })),

  setFilter: (filter) => set({ filter, entries: [], skipped: 0 }),
  clear: () => set({ entries: [], skipped: 0 }),
  setPaused: (paused) => set({ paused }),
}));
