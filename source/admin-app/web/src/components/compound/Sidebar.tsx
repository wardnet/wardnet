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

/** Sidebar navigation with branding and admin-conditional links. */
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
    <div className="flex h-full flex-col bg-side text-side-ink">
      <div className="flex items-center gap-2.5 p-4">
        <Logo size={28} />
        <h1 className="text-lg font-bold tracking-tight text-primary">Wardnet</h1>
      </div>

      <nav className="flex flex-1 flex-col gap-0.5 px-3">
        {links.map((link) => (
          <NavLink
            key={link.to}
            to={link.to}
            end={"end" in link ? link.end : false}
            onClick={onNavigate}
            className={({ isActive }) =>
              `rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                isActive
                  ? "bg-side-active text-side-ink-active"
                  : "text-side-ink/60 hover:bg-side-active/50 hover:text-side-ink"
              }`
            }
          >
            {link.label}
          </NavLink>
        ))}
      </nav>

      <div className="flex flex-col gap-3 border-t border-side-line p-4">
        {isAdmin && (
          <UpdateBanner
            updateAvailable={updateStatus?.status.update_available ?? false}
            latestVersion={updateStatus?.status.latest_version ?? null}
          />
        )}
        <ConnectionStatus />
        {isAdmin ? (
          <>
            <a
              href="/api/docs"
              target="_blank"
              rel="noopener noreferrer"
              className="text-left text-xs text-side-ink/40 transition-colors hover:text-side-ink/70"
            >
              API docs
            </a>
            <button
              onClick={handleLogout}
              className="text-left text-xs text-side-ink/40 transition-colors hover:text-side-ink/70"
            >
              Sign out
            </button>
          </>
        ) : (
          <NavLink
            to="/login"
            onClick={onNavigate}
            className="text-left text-xs text-side-ink/40 transition-colors hover:text-side-ink/70"
          >
            Sign in as admin
          </NavLink>
        )}
      </div>
    </div>
  );
}
