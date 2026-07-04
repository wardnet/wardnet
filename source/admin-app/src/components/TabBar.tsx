import { NavLink } from "react-router";
import {
  HomeIcon,
  MonitorIcon,
  NetworkIcon,
  GlobeIcon,
  SettingsIcon,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

const TABS: Array<{
  to: string;
  label: string;
  Icon: LucideIcon;
  end?: boolean;
}> = [
  { to: "/", label: "Home", Icon: HomeIcon, end: true },
  { to: "/devices", label: "Devices", Icon: MonitorIcon },
  { to: "/tunnels", label: "Tunnels", Icon: NetworkIcon },
  { to: "/dns", label: "DNS", Icon: GlobeIcon },
  { to: "/system", label: "System", Icon: SettingsIcon },
];

export function TabBar() {
  return (
    <nav className="flex shrink-0 gap-0.5 border-t border-white/[0.06] bg-side px-2 pb-1.5 pt-2">
      {TABS.map(({ to, label, Icon, end }) => (
        <NavLink
          key={to}
          to={to}
          end={end}
          data-testid={`tab-${label.toLowerCase()}`}
          className="flex flex-1 flex-col items-center gap-1 rounded-xl py-1.5 text-[10.5px] font-semibold tracking-wide text-white/50 transition-colors duration-snap aria-[current=page]:text-accent"
        >
          <Icon size={23} strokeWidth={2} />
          {label}
        </NavLink>
      ))}
    </nav>
  );
}
