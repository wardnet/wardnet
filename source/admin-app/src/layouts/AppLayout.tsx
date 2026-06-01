import { Outlet } from "react-router";

/**
 * Shell for authenticated routes.
 *
 * Minimal for now — a mobile nav bar will be added in a subsequent issue.
 */
export function AppLayout() {
  return (
    <div className="flex min-h-screen flex-col bg-bg text-ink">
      <main className="flex-1 overflow-y-auto">
        <Outlet />
      </main>
    </div>
  );
}
