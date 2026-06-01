import { Outlet } from "react-router";
import { Card } from "@wardnet/forge-web/card";
import logoSrc from "@/assets/logo.png";

export function AuthLayout() {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-8 bg-bg px-4 py-10 text-ink">
      <div className="flex flex-col items-center gap-3">
        <div className="flex items-center gap-3">
          <img src={logoSrc} alt="Wardnet" width={48} height={48} />
          <h1 className="text-2xl font-bold tracking-wider uppercase">
            Ward<span style={{ color: "var(--accent)" }}>net</span>
          </h1>
        </div>
        <p className="text-sm text-ink-3">Sign in to manage your network</p>
      </div>

      <Card className="w-full max-w-sm">
        <Outlet />
      </Card>
    </div>
  );
}
