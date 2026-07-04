import { Outlet } from "react-router";
import { AppHeader, useDaemonStatus, type ConnState } from "@wardnet/web";
import { useOnlineStatusContext } from "@/context/OnlineStatusContext";
import { ConnectionBanner } from "@/components/ConnectionBanner";
import { TabBar } from "@/components/TabBar";
import { InstallPrompt } from "@/features/InstallPrompt";

function deriveConnState(
  isOnline: boolean,
  isDaemonReachable: boolean,
): ConnState {
  if (!isOnline) return "offline";
  if (!isDaemonReachable) return "reconnecting";
  return "online";
}

export function AppLayout() {
  const { isOnline, isDaemonReachable } = useOnlineStatusContext();
  const { data } = useDaemonStatus();
  const connState = deriveConnState(isOnline, isDaemonReachable);
  const version = data?.version ?? null;

  return (
    <div className="relative flex h-screen flex-col overflow-hidden bg-bg text-ink">
      <AppHeader connState={connState} version={version} />
      <ConnectionBanner connState={connState} />
      <main className="flex-1 overflow-y-auto overscroll-contain [-webkit-overflow-scrolling:touch]">
        <Outlet />
      </main>
      <TabBar />
      <InstallPrompt />
    </div>
  );
}
