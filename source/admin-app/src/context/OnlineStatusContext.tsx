import { createContext, useContext, useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useOnlineStatus } from "@wardnet/wardnet-web";

type OnlineStatus = ReturnType<typeof useOnlineStatus>;

const OnlineStatusContext = createContext<OnlineStatus | null>(null);

export function OnlineStatusProvider({ children }: { children: React.ReactNode }) {
  const status = useOnlineStatus();
  const qc = useQueryClient();
  const prevReachable = useRef(status.isDaemonReachable);

  useEffect(() => {
    if (!prevReachable.current && status.isDaemonReachable) {
      void qc.invalidateQueries({ queryKey: ["daemon", "info"] });
    }
    prevReachable.current = status.isDaemonReachable;
  }, [status.isDaemonReachable, qc]);

  return (
    <OnlineStatusContext.Provider value={status}>
      {children}
    </OnlineStatusContext.Provider>
  );
}

export function useOnlineStatusContext(): OnlineStatus {
  const ctx = useContext(OnlineStatusContext);
  if (!ctx) throw new Error("useOnlineStatusContext must be inside OnlineStatusProvider");
  return ctx;
}
