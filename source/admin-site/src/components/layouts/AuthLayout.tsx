import { Outlet, useLocation } from "react-router";
import { Card, Logo, Text } from "@wardnet/web";

/**
 * Centered-card layout for unauthenticated screens (Login, first-run Setup).
 *
 * Pure Forge shell: full-bleed `--bg`, brand mark above a single `<Card>`
 * that hosts the routed page. No chrome (no sidebar, no topbar). The Logo
 * is sized at 64px so its marketing variant (>= 40px threshold) is used,
 * matching the launch-screen treatment defined in the design system.
 */
export function AuthLayout() {
  const { pathname } = useLocation();
  const tagline = pathname.startsWith("/setup")
    ? "Let's get your network ready"
    : "Sign in to manage your network";

  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-8 bg-bg px-4 py-10 text-ink">
      <div className="flex flex-col items-center gap-3">
        <Logo height={48} variant="light" />
        <Text as="p" size="sm" className="text-ink-3">
          {tagline}
        </Text>
      </div>

      <Card className="w-full max-w-sm">
        <Outlet />
      </Card>
    </div>
  );
}
