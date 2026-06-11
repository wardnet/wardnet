import type { TunnelStatus } from "@wardnet/js";
import type { PillProps } from "../primitives/pill";

export function tunnelStatusVariant(status: TunnelStatus): PillProps["variant"] {
  switch (status) {
    case "up":
      return "ok";
    case "connecting":
      return "info";
    case "reconnecting":
      return "warn";
    case "down":
      return "down";
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
  }
}
