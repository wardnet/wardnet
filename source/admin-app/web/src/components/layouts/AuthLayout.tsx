import { Outlet } from "react-router";
import { Card } from "@wardnet/forge-web/card";
import { Logo } from "@/components/compound/Logo";

/**
 * Centered-card layout for unauthenticated screens (Login, first-run Setup).
 *
 * Pure Forge shell: full-bleed `--bg`, brand mark above a single `<Card>`
 * that hosts the routed page. No chrome (no sidebar, no topbar). The Logo
 * is sized at 64px so its marketing variant (>= 40px threshold) is used,
 * matching the launch-screen treatment defined in the design system.
 */
export function AuthLayout() {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-8 bg-bg px-4 py-10 text-ink">
      <div className="flex flex-col items-center gap-3">
        <Logo size={64} />
        <h1 className="text-2xl font-bold tracking-wider uppercase">Wardnet</h1>
        <p className="text-sm text-ink-3">Sign in to manage your network</p>
      </div>

      <Card className="w-full max-w-sm">
        <Outlet />
      </Card>
    </div>
  );
}
