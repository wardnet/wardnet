import { CheckIcon } from "lucide-react";

export type BusyPhase = "working" | "done";
export type BusyAction = "reboot" | "restart";

interface Props {
  phase: BusyPhase;
  action: BusyAction;
}

const COPY: Record<BusyAction, Record<BusyPhase, { title: string; subtitle: string }>> = {
  reboot: {
    working: { title: "Rebooting wardnet-pi…", subtitle: "This takes about 60 seconds" },
    done: { title: "Pi is back online", subtitle: "Connection restored" },
  },
  restart: {
    working: { title: "Restarting daemon…", subtitle: "Reloading service" },
    done: { title: "Daemon restarted", subtitle: "Connection restored" },
  },
};

export function BusyOverlay({ phase, action }: Props) {
  const { title, subtitle } = COPY[action][phase];

  return (
    <div className="fixed inset-0 z-[60] flex flex-col items-center justify-center gap-5 bg-side text-white">
      {phase === "working" ? (
        <div className="h-[46px] w-[46px] animate-spin rounded-full border-[3px] border-white/15 border-t-accent" />
      ) : (
        <div className="flex h-[46px] w-[46px] items-center justify-center rounded-full bg-accent text-accent-ink">
          <CheckIcon size={26} strokeWidth={2.6} />
        </div>
      )}
      <div className="flex flex-col items-center gap-2 text-center">
        <p className="text-[17px] font-semibold tracking-tight">{title}</p>
        <p className="font-mono text-[12px] text-white/50">{subtitle}</p>
      </div>
    </div>
  );
}
