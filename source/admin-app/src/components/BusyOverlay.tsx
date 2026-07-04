import { CheckIcon } from "lucide-react";
import { Text } from "@wardnet/web";

export type BusyPhase = "working" | "done";
export type BusyAction = "reboot" | "restart";

interface Props {
  phase: BusyPhase;
  action: BusyAction;
}

const COPY: Record<
  BusyAction,
  Record<BusyPhase, { title: string; subtitle: string }>
> = {
  reboot: {
    working: {
      title: "Rebooting device…",
      subtitle: "This takes about 60 seconds",
    },
    done: { title: "Device is back online", subtitle: "Connection restored" },
  },
  restart: {
    working: { title: "Restarting daemon…", subtitle: "Reloading service" },
    done: { title: "Daemon restarted", subtitle: "Connection restored" },
  },
};

export function BusyOverlay({ phase, action }: Props) {
  const { title, subtitle } = COPY[action][phase];

  return (
    <div
      data-testid="system-busy-overlay"
      className="fixed inset-0 z-[60] flex flex-col items-center justify-center gap-5 bg-side text-white"
    >
      {phase === "working" ? (
        <div className="h-[46px] w-[46px] animate-spin rounded-full border-[3px] border-white/15 border-t-accent" />
      ) : (
        <div className="flex h-[46px] w-[46px] items-center justify-center rounded-full bg-accent text-accent-ink">
          <CheckIcon size={26} strokeWidth={2.6} />
        </div>
      )}
      <div className="flex flex-col items-center gap-2 text-center">
        <Text as="p" size="lg" weight="semibold" className="tracking-tight">
          {title}
        </Text>
        <Text as="p" size="xs" className="font-mono text-white/50">
          {subtitle}
        </Text>
      </div>
    </div>
  );
}
