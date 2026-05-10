import { Outlet } from "react-router";
import { Sidebar } from "@/components/compound/Sidebar";
import { MobileMenu } from "@/components/compound/MobileMenu";
import { Logo } from "@/components/compound/Logo";
import { ConnectionBanner } from "@/components/compound/ConnectionBanner";
import { UncleanShutdownBanner } from "@/components/compound/UncleanShutdownBanner";
import { useAuth } from "@/hooks/useAuth";

/**
 * Main application layout.
 *
 * Desktop: persistent left sidebar (w-56) + scrollable content area.
 * Mobile: sticky top header with hamburger menu + full-width content.
 */
export function AppLayout() {
  const { isAdmin } = useAuth();

  return (
    <div className="flex h-screen bg-bg text-ink">
      {/* Desktop sidebar — only for admins */}
      {isAdmin && (
        <aside className="hidden w-56 shrink-0 border-r border-side-line bg-side md:block">
          <Sidebar />
        </aside>
      )}

      {/* Main content area */}
      <div className="flex min-h-0 flex-1 flex-col">
        {/* Mobile header */}
        <header className="flex h-14 shrink-0 items-center gap-3 border-b border-line bg-bg/80 px-4 backdrop-blur-sm md:hidden">
          {isAdmin && <MobileMenu />}
          <Logo size={24} />
          <span className="text-lg font-bold tracking-tight text-accent">Wardnet</span>
        </header>

        <ConnectionBanner />
        {isAdmin && <UncleanShutdownBanner />}

        <main className="flex min-h-0 flex-1 flex-col overflow-y-auto p-4 md:p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
