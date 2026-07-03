import { type ComponentType, type SVGProps } from "react";
import { NavLink, useNavigate } from "react-router";
import {
  Archive,
  Boxes,
  Cable,
  Globe,
  GlobeLock,
  Inbox,
  LayoutGrid,
  Monitor,
  Network,
  Power,
  Router,
  Settings as SettingsIcon,
  ShieldCheck,
} from "lucide-react";
import { useAuth } from "@wardnet/web";
import { useDaemonStatus } from "@wardnet/web";
import { useUpdateStatus } from "@wardnet/web";
import { Logo } from "@wardnet/web";
import { Text } from "@wardnet/web";
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
      { to: "/tunnels", label: "Tunnels", icon: Cable, testId: "nav-tunnels" },
      { to: "/zones", label: "Zones", icon: Boxes, testId: "nav-zones" },
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
        icon: ShieldCheck,
        testId: "nav-dns-filter",
      },
      {
        to: "/rule-requests",
        label: "Rule requests",
        icon: Inbox,
        testId: "nav-rule-requests",
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
 * already on Forge accent tokens.
 */
export function Sidebar({ onNavigate }: SidebarProps) {
  const { isAdmin, logout } = useAuth();
  const navigate = useNavigate();
  // Only admins see update state — self-service users don't have the perms
  // to trigger installs, so we don't bother them with the banner.
  const { data: updateStatus } = useUpdateStatus();
  const { data: daemonStatus } = useDaemonStatus();

  const sections = isAdmin ? adminSections : [];

  function handleLogout() {
    logout();
    onNavigate?.();
    navigate("/");
  }

  return (
    <div className="side h-full">
      <div className="side__brand">
        <Logo height={28} variant="dark" />
      </div>
      {daemonStatus?.version && (
        // -mt absorbs the version into `.side__brand`'s bottom padding so it
        // sits as a tight caption right under the logo lockup. pl-[50px] lines
        // the "v" up under the start of the "WARDNET" wordmark at this height.
        <Text
          as="div"
          size="2xs"
          weight="normal"
          className="-mt-6 pb-2 pl-[50px] text-side-ink/40"
        >
          v{daemonStatus.version}
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
