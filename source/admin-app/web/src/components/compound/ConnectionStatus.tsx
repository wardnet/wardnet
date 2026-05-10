import { useDaemonStatus } from "@/hooks/useDaemonStatus";

/** Traffic-light status indicator + daemon version shown in the sidebar footer. */
export function ConnectionStatus() {
  const { data, isLoading } = useDaemonStatus();

  const reachable = data?.reachable ?? false;
  const color = isLoading ? "bg-warn" : reachable ? "bg-accent" : "bg-danger";
  const label = isLoading ? "Connecting…" : reachable ? "Connected" : "Disconnected";

  return (
    <div className="flex items-center gap-2">
      <span className={`inline-block size-2 rounded-full ${color}`} />
      <div className="flex flex-col">
        <span className="text-xs text-side-ink/70">{label}</span>
        {data?.version && <span className="text-[10px] text-side-ink/40">v{data.version}</span>}
      </div>
    </div>
  );
}
