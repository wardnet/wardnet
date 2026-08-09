import { Outlet, useLocation } from "react-router";
import { Sidebar } from "@/components/compound/Sidebar";
import { MobileMenu } from "@/components/compound/MobileMenu";
import { ConnectionBanner } from "@/components/compound/ConnectionBanner";
import { UncleanShutdownBanner } from "@/components/compound/UncleanShutdownBanner";
import {
  useAuth,
  useDaemonStatus,
  useUpdateStatus,
  useSystemStatus,
  useAcknowledgeShutdown,
} from "@wardnet/web";

/**
 * Main application layout.
 *
 * Renders the Forge `.app` shell: a sidebar column hosting <Sidebar /> next
 * to a `.main` content card. The `.main` card carries a `.topbar` header
 * (breadcrumbs on the left, chrome controls on the right) above the
 * connection / shutdown banners and the scrollable `<Outlet />` content.
 *
 * The shell chrome lives above every route and has no owning page, so this
 * layout wires the shell-wide auth/status hooks itself (the documented
 * layouts carve-out in `.agents/code-conventions.md`) and passes data +
 * callbacks down so the shell compounds stay pure presentation.
 */
export function AppLayout() {
  const { isAdmin, logout } = useAuth();
  const { data: daemonStatus, isLoading: daemonStatusLoading } =
    useDaemonStatus();
  // Only admins see update state — self-service users don't have the perms
  // to trigger installs, so we don't bother them with the banner.
  const { data: updateStatus } = useUpdateStatus();
  const { data: systemStatus } = useSystemStatus();
  const acknowledgeShutdown = useAcknowledgeShutdown();
  const location = useLocation();
  const crumb = crumbFromPath(location.pathname);

  const sidebarProps = {
    isAdmin,
    onLogout: logout,
    version: daemonStatus?.version,
    connectionLoading: daemonStatusLoading,
    connected: daemonStatus?.reachable ?? false,
    updateAvailable: updateStatus?.status.update_available ?? false,
    latestVersion: updateStatus?.status.latest_version ?? null,
  };

  return (
    <div className="app">
      <Sidebar {...sidebarProps} />

      <main className="main">
        <header className="topbar">
          {/* Mobile drawer trigger — Sidebar is always visible at desktop
              widths via the `.app` grid; MobileMenu hosts the same Sidebar
              inside a Drawer for narrow viewports (admin-only). */}
          {isAdmin && <MobileMenu {...sidebarProps} />}

          <div className="topbar__crumbs">
            <span>Wardnet</span>
            <span aria-hidden="true">/</span>
            <strong>{crumb}</strong>
          </div>

          <div className="topbar__spacer" />

          {/* TODO(slice T5-future): ⌘K command palette trigger goes here.
              Reserved by the Forge `.topbar` design but intentionally
              out-of-scope for this slice (no existing palette to port). */}
        </header>

        <ConnectionBanner reachable={daemonStatus?.reachable} />
        {isAdmin && (
          <UncleanShutdownBanner
            status={systemStatus}
            onDismiss={acknowledgeShutdown.mutate}
            dismissPending={acknowledgeShutdown.isPending}
          />
        )}

        <div className="scroll">
          <Outlet />
        </div>
      </main>
    </div>
  );
}

/**
 * Map a route path to a single human-readable breadcrumb label.
 * Kept intentionally tiny — full breadcrumb infra is deferred.
 */
function crumbFromPath(pathname: string): string {
  const seg = pathname.split("/").filter(Boolean)[0];
  if (!seg) return "Dashboard";
  return seg.charAt(0).toUpperCase() + seg.slice(1);
}
