import { useDaemonStatus } from "@/hooks/useDaemonStatus";

/** Traffic-light status indicator shown in the sidebar footer. The
 *  daemon version is rendered separately under the brand mark (see
 *  Sidebar), so this row stays a single line. */
export function ConnectionStatus() {
  const { data, isLoading } = useDaemonStatus();

  const reachable = data?.reachable ?? false;
  const color = isLoading ? "bg-warn" : reachable ? "bg-accent" : "bg-danger";
  const label = isLoading ? "Connecting…" : reachable ? "Connected" : "Disconnected";

  return (
    <div className="flex items-center gap-2">
      <span className={`inline-block size-2 rounded-full ${color}`} />
      <span className="text-xs text-side-ink/70">{label}</span>
    </div>
  );
}
