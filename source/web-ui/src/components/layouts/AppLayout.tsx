import { Outlet } from "react-router";
import { Sidebar } from "@/components/compound/Sidebar";
import { MobileMenu } from "@/components/compound/MobileMenu";
import { Logo } from "@/components/compound/Logo";

/**
 * Main application layout.
 *
 * Desktop: persistent left sidebar (w-56) + scrollable content area.
 * Mobile: sticky top header with hamburger menu + full-width content.
 */
export function AppLayout() {
  return (
    <div className="flex min-h-screen bg-background text-foreground">
      {/* Desktop sidebar — always dark */}
      <aside className="hidden w-56 shrink-0 border-r border-sidebar-border bg-sidebar md:block">
        <Sidebar />
      </aside>

      {/* Main content area */}
      <div className="flex flex-1 flex-col">
        {/* Mobile header */}
        <header className="sticky top-0 z-30 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur-sm md:hidden">
          <MobileMenu />
          <Logo size={24} />
          <span className="text-lg font-bold tracking-tight text-primary">Wardnet</span>
        </header>

        <main className="flex-1 p-4 md:p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
