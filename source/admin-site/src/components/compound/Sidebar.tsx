import { brand } from "@wardnet/styles/tokens";
import { type ComponentType, type SVGProps } from "react";
import { NavLink, useNavigate } from "react-router";
import {
  Archive,
  Cable,
  Globe,
  LayoutGrid,
  Monitor,
  Power,
  Router,
  Settings as SettingsIcon,
  ShieldCheck,
  Smartphone,
} from "lucide-react";
import { useAuth } from "@wardnet/web";
import { useDaemonStatus } from "@wardnet/web";
import { useUpdateStatus } from "@wardnet/web";
import { Logo } from "./Logo";
import { ConnectionStatus } from "./ConnectionStatus";
import { UpdateBanner } from "./UpdateBanner";

interface SidebarProps {
  onNavigate?: () => void;
}

type Icon = ComponentType<SVGProps<SVGSVGElement>>;

interface NavItem {
  to: string;
  label: string;
  icon: Icon;
  end?: boolean;
}

interface NavSection {
  /** Section label rendered as `.side__section`. Omit for ungrouped items. */
  heading?: string;
  items: NavItem[];
}

const selfServiceSections: NavSection[] = [
  { items: [{ to: "/", label: "My device", icon: Smartphone, end: true }] },
];

const adminSections: NavSection[] = [
  { items: [{ to: "/", label: "Dashboard", icon: LayoutGrid, end: true }] },
  {
    heading: "Network",
    items: [
      { to: "/devices", label: "Devices", icon: Monitor },
      { to: "/tunnels", label: "Tunnels", icon: Cable },
      { to: "/dhcp", label: "DHCP", icon: Router },
    ],
  },
  {
    heading: "Resolver",
    items: [
      { to: "/dns", label: "DNS", icon: Globe, end: true },
      { to: "/dns/filter", label: "DNS Filtering", icon: ShieldCheck },
    ],
  },
  {
    heading: "System",
    items: [
      { to: "/settings", label: "Settings", icon: SettingsIcon },
      { to: "/backups", label: "Backups", icon: Archive },
      { to: "/power", label: "Power", icon: Power },
    ],
  },
];

/**
 * Sidebar navigation with branding and admin-conditional links.
 *
 * Renders the Forge `.side` family throughout: `.side__brand` for the brand
 * mark, `.side__nav` wrapping `.side__item` rows (`.is-active` driven by
 * react-router's `NavLink` active state), and `.side__foot` for the footer
 * cluster with `.side__status` around `<ConnectionStatus />` and `.side__links`
 * for API-docs / sign-out / sign-in. Floating chrome is baked into `.side`
 * itself per the slice 0a sweep, so no variant prop is needed.
 *
 * `<UpdateBanner />` stays mounted as a child — it's a separate compound
 * already on Forge accent tokens.
 */
export function Sidebar({ onNavigate }: SidebarProps) {
  const { isAdmin, logout } = useAuth();
  const navigate = useNavigate();
  // Only admins see update state — self-service users don't have the perms
  // to trigger installs, so we don't bother them with the banner.
  const { data: updateStatus } = useUpdateStatus();
  const { data: daemonStatus } = useDaemonStatus();

  const sections = isAdmin ? adminSections : selfServiceSections;

  function handleLogout() {
    logout();
    onNavigate?.();
    navigate("/");
  }

  return (
    <div className="side h-full">
      <div className="side__brand">
        <Logo size={22} />
        <span>
          Ward<span style={{ color: brand.accent }}>net</span>
        </span>
      </div>
      {daemonStatus?.version && (
        // -mt absorbs the version into `.side__brand`'s bottom padding
        // so the wordmark stays vertically centered on the logo while
        // the version sits as a tight caption right under it.
        // pl-[50px] = 18px (brand padding) + 22px (logo) + 10px (gap)
        // → aligns the "v" with the start of "Wardnet".
        <div className="-mt-6 pb-2 pl-[50px] text-[10px] font-normal text-side-ink/40">
          v{daemonStatus.version}
        </div>
      )}

      <nav className="flex-1 overflow-y-auto">
        {sections.map((section, index) => (
          <div key={section.heading ?? `top-${index}`}>
            {section.heading && (
              <div className="side__section">{section.heading}</div>
            )}
            <div className="side__nav">
              {section.items.map((item) => (
                <NavLink
                  key={item.to}
                  to={item.to}
                  end={item.end}
                  onClick={onNavigate}
                  className={({ isActive }) =>
                    `side__item${isActive ? " is-active" : ""}`
                  }
                >
                  <item.icon className="ico" aria-hidden="true" />
                  <span>{item.label}</span>
                </NavLink>
              ))}
            </div>
          </div>
        ))}
      </nav>

      <div className="side__foot">
        {isAdmin && (
          <UpdateBanner
            updateAvailable={updateStatus?.status.update_available ?? false}
            latestVersion={updateStatus?.status.latest_version ?? null}
          />
        )}
        <div className="side__status">
          <ConnectionStatus />
        </div>
        <div className="side__links">
          {isAdmin ? (
            <>
              <a href="/api/docs" target="_blank" rel="noopener noreferrer">
                API docs
              </a>
              <button
                type="button"
                onClick={handleLogout}
                className="cursor-pointer border-none bg-transparent p-0 text-inherit"
              >
                Sign out
              </button>
            </>
          ) : (
            <NavLink to="/login" onClick={onNavigate}>
              Sign in as admin
            </NavLink>
          )}
        </div>
      </div>
    </div>
  );
}
