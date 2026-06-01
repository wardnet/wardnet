import { ShieldIcon, RefreshCwIcon } from "lucide-react";

export type ConnState = "online" | "reconnecting" | "offline";

interface Props {
  connState: ConnState;
  version: string | null;
}

export function Header({ connState, version }: Props) {
  return (
    <header className="flex shrink-0 items-center gap-3 bg-side px-4 py-3">
      <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-accent text-accent-ink">
        <ShieldIcon size={16} strokeWidth={2} />
      </div>
      <div className="flex flex-col leading-none">
        <span className="text-[17px] font-bold tracking-tight text-white">
          Ward<em className="not-italic text-accent">net</em>
        </span>
        {version && (
          <span className="mt-0.5 font-mono text-[10px] text-white/40">{version}</span>
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
