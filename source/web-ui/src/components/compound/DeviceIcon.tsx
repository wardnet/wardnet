import { Monitor, Smartphone, Laptop, Tablet, Gamepad2, Tv, Cpu, HelpCircle } from "lucide-react";
import type { DeviceType } from "@wardnet/js";
import { cn } from "@/lib/utils";

const iconMap: Record<DeviceType, React.ElementType> = {
  tv: Tv,
  phone: Smartphone,
  laptop: Laptop,
  tablet: Tablet,
  game_console: Gamepad2,
  settop_box: Monitor,
  iot: Cpu,
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
