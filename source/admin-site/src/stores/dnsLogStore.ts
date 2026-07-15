import { create } from "zustand";
import type { QueryLogEvent } from "@wardnet/js";

const MAX_ENTRIES = 500;

export interface DnsLogFilter {
  domain: string;
  client_ip: string;
  /** Matches the event's write-time device attribution (empty = any). */
  device_id: string;
  results: string[];
}

interface DnsLogState {
  entries: QueryLogEvent[];
  connected: boolean;
  paused: boolean;
  skipped: number;
  filter: DnsLogFilter;

  /** Internal — called by the connection manager. */
  _addEvent: (event: QueryLogEvent) => void;
  _setConnected: (connected: boolean) => void;
  _addSkipped: (count: number) => void;

  setFilter: (filter: DnsLogFilter) => void;
  clear: () => void;
  setPaused: (paused: boolean) => void;
}

export const useDnsLogStore = create<DnsLogState>((set) => ({
  entries: [],
  connected: false,
  paused: false,
  skipped: 0,
  filter: { domain: "", client_ip: "", device_id: "", results: [] },

  _addEvent: (event) =>
    set((state) => {
      if (state.paused) return state;
      const next = [event, ...state.entries];
      return {
        entries: next.length > MAX_ENTRIES ? next.slice(0, MAX_ENTRIES) : next,
      };
    }),

  _setConnected: (connected) => set({ connected }),
  _addSkipped: (count) => set((state) => ({ skipped: state.skipped + count })),

  // Filter is informational — entries are kept whole and consumers
  // narrow them at render time. That way widening or clearing the
  // filter immediately reveals previously buffered matches instead of
  // waiting for new events to trickle in from the server.
  setFilter: (filter) => set({ filter }),
  clear: () => set({ entries: [], skipped: 0 }),
  setPaused: (paused) => set({ paused }),
}));
