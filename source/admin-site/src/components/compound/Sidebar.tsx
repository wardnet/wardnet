import { type ComponentType, type SVGProps } from "react";
import { NavLink, useNavigate } from "react-router";
import {
  Archive,
  Layers,
  Globe,
  GlobeLock,
  Inbox,
  LayoutGrid,
  Monitor,
  Network,
  Power,
  Route,
  Router,
  Settings as SettingsIcon,
  Split,
} from "lucide-react";
import { ShieldWifi, GlobeFilter } from "@wardnet/web";
import { Logo } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { ConnectionStatus } from "./ConnectionStatus";
import { UpdateBanner } from "./UpdateBanner";

export interface SidebarProps {
  onNavigate?: () => void;
  /** Whether the current session is an admin (drives nav + footer links). */
  isAdmin: boolean;
  /** Revoke the server session; resolves even when the network call fails. */
  onLogout: () => Promise<unknown>;
  /** Daemon version for the caption under the brand mark, if known. */
  version: string | null | undefined;
  /** Connection-indicator state, from the shell layout's daemon-status query. */
  connectionLoading: boolean;
  connected: boolean;
  /** Update-banner state, from the shell layout's update-status query. */
  updateAvailable: boolean;
  latestVersion: string | null;
}

type Icon = ComponentType<SVGProps<SVGSVGElement>>;

interface NavItem {
  to: string;
  label: string;
  icon: Icon;
  end?: boolean;
  /** e2e locator rendered as `data-testid` on the nav link. */
  testId: string;
}

interface NavSection {
  /** Section label rendered as `.side__section`. Omit for ungrouped items. */
  heading?: string;
  items: NavItem[];
}

const adminSections: NavSection[] = [
  {
    items: [
      {
        to: "/",
        label: "Dashboard",
        icon: LayoutGrid,
        end: true,
        testId: "nav-dashboard",
      },
    ],
  },
  {
    heading: "Network",
    items: [
      {
        to: "/devices",
        label: "Devices",
        icon: Monitor,
        testId: "nav-devices",
      },
      { to: "/tunnels", label: "Tunnels", icon: Route, testId: "nav-tunnels" },
      { to: "/routing", label: "Routing", icon: Split, testId: "nav-routing" },
      { to: "/vpn", label: "VPN", icon: ShieldWifi, testId: "nav-vpn" },
      { to: "/zones", label: "Zones", icon: Layers, testId: "nav-zones" },
      { to: "/dhcp", label: "DHCP", icon: Router, testId: "nav-dhcp" },
    ],
  },
  {
    heading: "Resolver",
    items: [
      { to: "/dns", label: "DNS", icon: Globe, end: true, testId: "nav-dns" },
      {
        to: "/dns/local",
        label: "Local DNS",
        icon: Network,
        testId: "nav-dns-local",
      },
      {
        to: "/dns/filter",
        label: "DNS Filtering",
        icon: GlobeFilter,
        testId: "nav-dns-filter",
      },
      {
        to: "/access-requests",
        label: "Access requests",
        icon: Inbox,
        testId: "nav-access-requests",
      },
    ],
  },
  {
    heading: "System",
    items: [
      {
        to: "/settings",
        label: "Settings",
        icon: SettingsIcon,
        testId: "nav-settings",
      },
      {
        to: "/remote-access",
        label: "Remote access",
        icon: GlobeLock,
        testId: "nav-remote-access",
      },
      {
        to: "/backups",
        label: "Backups",
        icon: Archive,
        testId: "nav-backups",
      },
      { to: "/power", label: "Power", icon: Power, testId: "nav-power" },
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
 * already on Forge accent tokens. Pure presentation — the shell layout wires
 * the auth/status hooks and passes data + callbacks in.
 */
export function Sidebar({
  onNavigate,
  isAdmin,
  onLogout,
  version,
  connectionLoading,
  connected,
  updateAvailable,
  latestVersion,
}: SidebarProps) {
  const navigate = useNavigate();

  const sections = isAdmin ? adminSections : [];

  function handleLogout() {
    // Revoke the server session before leaving the page. onLogout() clears
    // local auth state even when the network call fails, so the redirect
    // always happens.
    void onLogout().then(() => {
      onNavigate?.();
      navigate("/");
    });
  }

  return (
    <div className="side h-full">
      <div className="side__brand">
        <Logo height={28} variant="dark" />
      </div>
      {version && (
        // -mt absorbs the version into `.side__brand`'s bottom padding so it
        // sits as a tight caption right under the logo lockup. pl-[50px] lines
        // the "v" up under the start of the "WARDNET" wordmark at this height.
        <Text
          as="div"
          size="2xs"
          weight="normal"
          className="-mt-6 pb-2 pl-[50px] text-side-ink/40"
        >
          v{version}
        </Text>
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
                  data-testid={item.testId}
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
            updateAvailable={updateAvailable}
            latestVersion={latestVersion}
          />
        )}
        <div className="side__status">
          <ConnectionStatus
            isLoading={connectionLoading}
            reachable={connected}
          />
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
