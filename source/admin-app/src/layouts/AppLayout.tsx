import { Outlet } from "react-router";
import { useOnlineStatus, useDaemonStatus } from "@wardnet/wardnet-web";
import { Header } from "@/components/Header";
import { ConnectionBanner } from "@/components/ConnectionBanner";
import { TabBar } from "@/components/TabBar";
import { InstallPrompt } from "@/features/InstallPrompt";
import type { ConnState } from "@/components/Header";

function deriveConnState(isOnline: boolean, isDaemonReachable: boolean): ConnState {
  if (!isOnline) return "offline";
  if (!isDaemonReachable) return "reconnecting";
  return "online";
}

export function AppLayout() {
  const { isOnline, isDaemonReachable } = useOnlineStatus();
  const { data } = useDaemonStatus();
  const connState = deriveConnState(isOnline, isDaemonReachable);
  const version = data?.version ?? null;

  return (
    <div className="relative flex h-screen flex-col overflow-hidden bg-bg text-ink">
      <Header connState={connState} version={version} />
      <ConnectionBanner connState={connState} />
      <main className="flex-1 overflow-y-auto overscroll-contain [-webkit-overflow-scrolling:touch]">
        <Outlet />
      </main>
      <TabBar />
      <InstallPrompt />
    </div>
  );
}
