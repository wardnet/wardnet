import { RefreshCwIcon } from "lucide-react";

import { Logo } from "./Logo";

export type ConnState = "online" | "reconnecting" | "offline";

interface AppHeaderProps {
  connState: ConnState;
  version: string | null;
}

/**
 * Top bar shared by the household (user-app) and admin (admin-app) PWAs:
 * the Wardnet logo lockup with the daemon version beneath it, plus a live
 * connection-status indicator on the right.
 */
export function AppHeader({ connState, version }: AppHeaderProps) {
  return (
    <header className="flex shrink-0 items-center gap-3 bg-side px-4 py-3">
      <div className="flex flex-col leading-none">
        <Logo height={22} variant="dark" />
        {version && (
          <span className="-mt-0.5 pl-[34px] font-mono text-[10px] text-white/40">
            {version}
          </span>
        )}
      </div>
      <div className="ml-auto flex items-center gap-1.5 text-[11px] font-medium text-white/70">
        {connState === "online" && (
          <>
            <span className="size-[7px] rounded-full bg-accent animate-pulse-dot" />
            Connected
          </>
        )}
        {connState === "reconnecting" && (
          <>
            <RefreshCwIcon size={12} strokeWidth={2} className="animate-spin" />
            Reconnecting
          </>
        )}
        {connState === "offline" && (
          <>
            <span className="size-[7px] rounded-full bg-warn" />
            Offline
          </>
        )}
      </div>
    </header>
  );
}
