import { RefreshCwIcon } from "lucide-react";

import { Logo } from "@wardnet/ui";

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
    <header className="app-header">
      <div className="app-header__brand">
        <Logo height={22} variant="dark" />
        {version && <span className="app-header__version">{version}</span>}
      </div>
      <div className="app-header__status" data-testid="conn-status">
        {connState === "online" && (
          <>
            <span className="app-header__dot app-header__dot--online" />
            Connected
          </>
        )}
        {connState === "reconnecting" && (
          <>
            <RefreshCwIcon
              size={12}
              strokeWidth={2}
              className="app-header__spinner"
            />
            Reconnecting
          </>
        )}
        {connState === "offline" && (
          <>
            <span className="app-header__dot app-header__dot--offline" />
            Offline
          </>
        )}
      </div>
    </header>
  );
}
