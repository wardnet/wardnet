import { NavLink } from "react-router";
import { Text } from "@wardnet/web";
import { HomeIcon, ChartColumnIcon, SettingsIcon } from "lucide-react";
import type { LucideIcon } from "lucide-react";

const TABS: Array<{
  to: string;
  label: string;
  Icon: LucideIcon;
  end?: boolean;
}> = [
  { to: "/", label: "Home", Icon: HomeIcon, end: true },
  { to: "/stats", label: "Stats", Icon: ChartColumnIcon },
  { to: "/settings", label: "Settings", Icon: SettingsIcon },
];

export function TabBar() {
  return (
    <nav className="flex shrink-0 gap-0.5 border-t border-white/[0.06] bg-side px-2 pb-1.5 pt-2">
      {TABS.map(({ to, label, Icon, end }) => (
        <NavLink
          key={to}
          to={to}
          end={end}
          className="flex flex-1 flex-col items-center gap-1 rounded-xl py-1.5 tracking-wide text-white/50 transition-colors duration-snap aria-[current=page]:text-accent"
        >
          <Icon size={23} strokeWidth={2} />
          <Text as="span" size="2xs" weight="semibold">
            {label}
          </Text>
        </NavLink>
      ))}
    </nav>
  );
}
