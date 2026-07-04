import { Outlet } from "react-router";
import { Card, Logo, Text } from "@wardnet/web";

export function AuthLayout() {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-8 bg-bg px-4 py-10 text-ink">
      <div className="flex flex-col items-center gap-3">
        <Logo height={48} variant="light" />
        <Text as="p" size="sm" className="text-ink-3">
          Sign in to manage your network
        </Text>
      </div>

      <Card className="w-full max-w-sm">
        <Outlet />
      </Card>
    </div>
  );
}
