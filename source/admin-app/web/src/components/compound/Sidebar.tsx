import { NavLink, useNavigate } from "react-router";
import { useAuth } from "@/hooks/useAuth";
import { useUpdateStatus } from "@/hooks/useUpdate";
import { Logo } from "./Logo";
import { ConnectionStatus } from "./ConnectionStatus";
import { UpdateBanner } from "./UpdateBanner";

interface SidebarProps {
  onNavigate?: () => void;
}

interface NavItem {
  to: string;
  label: string;
  end?: boolean;
}

const selfServiceLinks: NavItem[] = [{ to: "/", label: "My device", end: true }];

const adminLinks: NavItem[] = [
  { to: "/", label: "Dashboard", end: true },
  { to: "/devices", label: "Devices" },
  { to: "/tunnels", label: "Tunnels" },
  { to: "/dhcp", label: "DHCP" },
  { to: "/dns", label: "DNS", end: true },
  { to: "/dns/filter", label: "DNS Filtering" },
  { to: "/settings", label: "Settings" },
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

  const links = isAdmin ? adminLinks : selfServiceLinks;

  function handleLogout() {
    logout();
    onNavigate?.();
    navigate("/");
  }

  return (
    <div className="side h-full">
      <div className="side__brand">
        <Logo size={28} className="logo" />
        <span>Wardnet</span>
      </div>

      <nav className="side__nav flex-1">
        {links.map((link) => (
          <NavLink
            key={link.to}
            to={link.to}
            end={"end" in link ? link.end : false}
            onClick={onNavigate}
            className={({ isActive }) => `side__item${isActive ? " is-active" : ""}`}
          >
            {link.label}
          </NavLink>
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
