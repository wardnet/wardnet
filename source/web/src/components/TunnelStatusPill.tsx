import type { TunnelStatus } from "@wardnet/js";
import { Pill } from "../primitives/pill";
import { tunnelStatusVariant, tunnelStatusLabel } from "../lib/tunnel";

interface TunnelStatusPillProps {
  status: TunnelStatus;
}

export function TunnelStatusPill({ status }: TunnelStatusPillProps) {
  return (
    <Pill variant={tunnelStatusVariant(status)}>
      <span className="mr-1" aria-hidden>
        ●
      </span>
      {tunnelStatusLabel(status)}
    </Pill>
  );
}
