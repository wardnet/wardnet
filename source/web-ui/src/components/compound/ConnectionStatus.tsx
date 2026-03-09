import { useDaemonStatus } from "@/hooks/useDaemonStatus";

/** Traffic-light status indicator + daemon version shown in the sidebar footer. */
export function ConnectionStatus() {
  const { data, isLoading } = useDaemonStatus();

  const reachable = data?.reachable ?? false;
  const color = isLoading ? "bg-yellow-400" : reachable ? "bg-emerald-400" : "bg-red-400";
  const label = isLoading ? "Connecting..." : reachable ? "Connected" : "Disconnected";

  return (
    <div className="flex items-center gap-2">
      <span className={`inline-block size-2 rounded-full ${color}`} />
      <div className="flex flex-col">
        <span className="text-xs text-sidebar-foreground/70">{label}</span>
        {data?.version && (
          <span className="text-[10px] text-sidebar-foreground/40">v{data.version}</span>
        )}
      </div>
    </div>
  );
}
