import { Smartphone, Laptop, Tablet, Gamepad2, Tv, Cpu, HelpCircle, Router, Network, Server } from "lucide-react";
import type { DeviceType } from "@wardnet/js";
import { cn } from "@/lib/utils";

function SetTopBox({ size = 24, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <rect x="2" y="8" width="20" height="10" rx="2" />
      <path d="M6.01 13H6" />
      <path d="M10.01 13H10" />
      <path d="M14 13h4" />
    </svg>
  );
}

const iconMap: Record<DeviceType, React.ElementType> = {
  tv: Tv,
  phone: Smartphone,
  laptop: Laptop,
  tablet: Tablet,
  game_console: Gamepad2,
  settop_box: SetTopBox,
  iot: Cpu,
  router: Router,
  managed_switch: Network,
  server: Server,
  unknown: HelpCircle,
};

interface DeviceIconProps {
  type: DeviceType;
  size?: number;
  className?: string;
}

/** Icon for a device type. Renders a matching Lucide icon. */
export function DeviceIcon({ type, size = 18, className }: DeviceIconProps) {
  const Icon = iconMap[type] ?? HelpCircle;
  return <Icon size={size} className={cn("text-muted-foreground", className)} />;
}
