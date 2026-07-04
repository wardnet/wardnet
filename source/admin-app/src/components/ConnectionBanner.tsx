import { RefreshCwIcon, WifiOffIcon } from "lucide-react";
import { Banner, type ConnState } from "@wardnet/web";

interface Props {
  connState: ConnState;
}

export function ConnectionBanner({ connState }: Props) {
  if (connState === "online") return null;

  if (connState === "offline") {
    return (
      <Banner
        tone="down"
        role="alert"
        icon={<WifiOffIcon size={15} strokeWidth={1.9} />}
      >
        No connection to wardnet daemon — showing last known state
      </Banner>
    );
  }

  return (
    <Banner
      tone="warn"
      icon={
        <RefreshCwIcon size={14} strokeWidth={2} className="animate-spin" />
      }
    >
      Reconnecting to wardnet daemon…
    </Banner>
  );
}
