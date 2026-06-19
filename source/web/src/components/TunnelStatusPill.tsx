import type { TunnelStatus } from "@wardnet/js";
import { Pill } from "@wardnet/ui";
import { tunnelStatusVariant, tunnelStatusLabel } from "../lib/tunnel";

interface TunnelStatusPillProps {
  status: TunnelStatus;
}

export function TunnelStatusPill({ status }: TunnelStatusPillProps) {
  return (
    <Pill variant={tunnelStatusVariant(status)}>
      <span className="tunnel-status-pill__dot" aria-hidden>
        ●
      </span>
      {tunnelStatusLabel(status)}
    </Pill>
  );
}
