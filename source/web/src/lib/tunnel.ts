import type { TunnelStatus } from "@wardnet/js";
import type { PillProps } from "@wardnet/ui";

export function tunnelStatusVariant(
  status: TunnelStatus,
): PillProps["variant"] {
  switch (status) {
    case "up":
      return "ok";
    case "connecting":
      return "info";
    case "reconnecting":
      return "warn";
    case "down":
      return "down";
    default: {
      const unhandled: never = status;
      throw new Error(`Unhandled TunnelStatus: ${String(unhandled)}`);
    }
  }
}

export function tunnelStatusLabel(status: TunnelStatus): string {
  switch (status) {
    case "up":
      return "Active";
    case "connecting":
      return "Connecting";
    case "reconnecting":
      return "Reconnecting";
    case "down":
      return "Down";
    default: {
      const unhandled: never = status;
      throw new Error(`Unhandled TunnelStatus: ${String(unhandled)}`);
    }
  }
}
