import { useEffect, useRef } from "react";
import { logService } from "@/lib/sdk";
import { useLogStore } from "@/stores/logStore";

/**
 * Global log stream connection manager.
 *
 * Renders nothing — mount once at the app root. Uses the SDK LogService
 * for WebSocket management and pushes entries into the Zustand log store
 * so they persist across page navigation.
 */
export function LogStreamManager() {
  const store = useLogStore;
  const prevFilter = useRef(store.getState().filter);
  const prevPaused = useRef(store.getState().paused);

  useEffect(() => {
    // Connect once on mount.
    logService.connect(
      {
        onEntry: (entry) => store.getState()._addEntry(entry),
        onLagged: (skipped) => store.getState()._addSkipped(skipped),
        onConnected: () => store.getState()._setConnected(true),
        onDisconnected: () => store.getState()._setConnected(false),
      },
      store.getState().filter,
    );

    // Subscribe to store changes for filter and pause/resume.
    const unsub = store.subscribe((state) => {
      // Filter changed.
      const filterChanged =
        state.filter.level !== prevFilter.current.level ||
        state.filter.target !== prevFilter.current.target;
      if (filterChanged) {
        prevFilter.current = state.filter;
        logService.setFilter(state.filter);
      }

      // Pause/resume changed.
      if (state.paused !== prevPaused.current) {
        prevPaused.current = state.paused;
        if (state.paused) {
          logService.pause();
        } else {
          logService.resume();
        }
      }
    });

    return () => {
      unsub();
      logService.disconnect();
    };
  }, [store]);

  return null;
}
