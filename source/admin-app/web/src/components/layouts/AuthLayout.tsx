import { Outlet } from "react-router";
import { Logo } from "@/components/compound/Logo";

/**
 * Full-screen branded layout for authentication pages.
 *
 * Deep indigo gradient background with logo/title hero area,
 * form card floated over the bottom half.
 */
export function AuthLayout() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-gradient-to-b from-[oklch(0.22_0.12_275)] via-[oklch(0.2_0.1_260)] to-[oklch(0.18_0.06_240)] px-4">
      <div className="flex w-full max-w-sm flex-col items-center gap-8">
        {/* Hero: logo + branding */}
        <div className="flex flex-col items-center gap-4">
          <Logo size={80} className="drop-shadow-lg" />
          <h1 className="text-2xl font-bold tracking-wider text-white uppercase">Wardnet</h1>
          <p className="text-lg text-white/70">Sign in to manage your network</p>
        </div>

        {/* Form card */}
        <div className="w-full">
          <Outlet />
        </div>
      </div>
    </div>
  );
}
