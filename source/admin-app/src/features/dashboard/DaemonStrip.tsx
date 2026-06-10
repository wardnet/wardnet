import { ActivityIcon } from "lucide-react";
import { formatUptime } from "@wardnet/web";

interface Props {
  reachable: boolean;
  version: string | null;
  uptimeSeconds: number | null;
}

export function DaemonStrip({ reachable, version, uptimeSeconds }: Props) {
  return (
    <div className="flex items-center gap-2 rounded-lg bg-side px-4 py-3 text-xs text-white/70">
      <ActivityIcon size={13} strokeWidth={1.9} className={reachable ? "text-accent" : "text-warn"} />
      <span className={reachable ? "text-accent font-medium" : "text-warn font-medium"}>
        {reachable ? "Running" : "Unreachable"}
      </span>
      {version && (
        <>
          <span className="text-white/30">·</span>
          <span className="font-mono">{version}</span>
        </>
      )}
      {uptimeSeconds != null && (
        <>
          <span className="text-white/30">·</span>
          <span>up {formatUptime(uptimeSeconds)}</span>
        </>
      )}
    </div>
  );
}
