import type { ReactNode } from "react";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useAuth, useSetupStatus, useTheme, checkAuth } = vi.hoisted(() => ({
  useAuth: vi.fn(),
  useSetupStatus: vi.fn(),
  useTheme: vi.fn(),
  checkAuth: vi.fn(),
}));

vi.mock("@wardnet/web", () => ({ useAuth, useSetupStatus }));
vi.mock("@/hooks/useTheme", () => ({ useTheme }));

// Layouts + boundary reduced to passthroughs that still render their Outlet.
vi.mock("react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-router")>();
  return actual;
});
vi.mock("@/components/layouts/AppLayout", async () => {
  const { Outlet } = await import("react-router");
  return {
    AppLayout: () => (
      <div data-testid="app-layout">
        <Outlet />
      </div>
    ),
  };
});
vi.mock("@/components/layouts/AuthLayout", async () => {
  const { Outlet } = await import("react-router");
  return {
    AuthLayout: () => (
      <div data-testid="auth-layout">
        <Outlet />
      </div>
    ),
  };
});
vi.mock("@/components/core/ErrorBoundary", () => ({
  ErrorBoundary: ({ children }: { children: ReactNode }) => <>{children}</>,
}));

// Stub every routed page to a marker so we assert routing, not page internals.
const { page } = vi.hoisted(() => ({
  page: (name: string) => ({ default: () => <div>{name}-page</div> }),
}));
vi.mock("@/pages/Dashboard", () => page("dashboard"));
vi.mock("@/pages/Devices", () => page("devices"));
vi.mock("@/pages/DeviceDetail", () => page("device-detail"));
vi.mock("@/pages/Tunnels", () => page("tunnels"));
vi.mock("@/pages/TunnelDetail", () => page("tunnel-detail"));
vi.mock("@/pages/Settings", () => page("settings"));
vi.mock("@/pages/RemoteAccess", () => page("remote-access"));
vi.mock("@/pages/Power", () => page("power"));
vi.mock("@/pages/Backups", () => page("backups"));
vi.mock("@/pages/Dhcp", () => page("dhcp"));
vi.mock("@/pages/Dns", () => page("dns"));
vi.mock("@/pages/DnsLocal", () => page("dns-local"));
vi.mock("@/pages/DnsLogs", () => page("dns-logs"));
vi.mock("@/pages/DnsFilter", () => page("dns-filter"));
vi.mock("@/pages/DnsFilterProfile", () => page("dns-filter-profile"));
vi.mock("@/pages/DnsFilterProfileNew", () => page("dns-filter-profile-new"));
vi.mock("@/pages/RuleRequests", () => page("rule-requests"));
vi.mock("@/pages/Login", () => page("login"));
vi.mock("@/pages/Setup", () => page("setup"));
vi.mock("@/pages/NotFound", () => page("not-found"));

import App from "@/App";

function renderAt(path: string) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <App />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const completed = { data: { wizard_step: "completed" }, isLoading: false };

describe("App routing", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAuth.mockReturnValue({ isAdmin: true, isChecking: false, checkAuth });
    useSetupStatus.mockReturnValue(completed);
    sessionStorage.clear();
  });

  it("runs checkAuth and theme on mount and shows Dashboard at root for admins", () => {
    renderAt("/");
    expect(useTheme).toHaveBeenCalled();
    expect(checkAuth).toHaveBeenCalled();
    expect(screen.getByText("dashboard-page")).toBeInTheDocument();
  });

  it("renders admin pages through the AdminRoute guard", () => {
    renderAt("/devices");
    expect(screen.getByText("devices-page")).toBeInTheDocument();
  });

  it("redirects unknown auth-checking state to nothing (returns null)", () => {
    useAuth.mockReturnValue({ isAdmin: false, isChecking: true, checkAuth });
    const { container } = renderAt("/");
    expect(container.textContent).toBe("");
  });

  it("sends non-admins from a guarded route to login", () => {
    useAuth.mockReturnValue({ isAdmin: false, isChecking: false, checkAuth });
    renderAt("/devices");
    expect(screen.getByText("login-page")).toBeInTheDocument();
  });

  it("redirects to setup when the wizard is unfinished", () => {
    useSetupStatus.mockReturnValue({
      data: { wizard_step: "network" },
      isLoading: false,
    });
    renderAt("/settings");
    expect(screen.getByText("setup-page")).toBeInTheDocument();
  });

  it("renders NotFound for unmatched paths", () => {
    renderAt("/nope");
    expect(screen.getByText("not-found-page")).toBeInTheDocument();
  });

  it("renders the setup page on its own (outside the auth layout)", () => {
    renderAt("/setup");
    expect(screen.queryByTestId("auth-layout")).not.toBeInTheDocument();
    expect(screen.getByText("setup-page")).toBeInTheDocument();
  });

  it("returns null from SetupGuard while status is loading", () => {
    useSetupStatus.mockReturnValue({ data: undefined, isLoading: true });
    const { container } = renderAt("/settings");
    expect(container.textContent).toBe("");
  });

  it("redirects back to a stored returnTo once setup completes", () => {
    const replace = vi.fn();
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { ...window.location, replace },
    });
    sessionStorage.setItem("wardnet_returnTo", "https://example.com/app");
    renderAt("/settings");
    expect(replace).toHaveBeenCalledWith("https://example.com/app");
    expect(sessionStorage.getItem("wardnet_returnTo")).toBeNull();
  });
});
