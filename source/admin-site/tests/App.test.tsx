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
vi.mock("@/pages/AccessRequests", () => page("access-requests"));
vi.mock("@/pages/Login", () => {
  // Named, so the rules-of-hooks lint recognises it as a component — an
  // anonymous arrow calling `useLocation` is indistinguishable from a hook
  // called out of place.
  function LoginStub() {
    // Renders its own search string so a routing test can assert what
    // survived the redirect, without reaching into router internals.
    const { search } = useLocation();
    return <div data-testid="login-search">{`login-page${search}`}</div>;
  }
  return { default: LoginStub };
});
vi.mock("@/pages/Setup", () => page("setup"));
vi.mock("@/pages/NotFound", () => page("not-found"));

import { useLocation } from "react-router";

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

  // The daemon's OAuth callback redirects to `/admin/` on failure — the
  // **index** route, which is not wrapped in `AdminRoute`. Forwarding the code
  // only from the guard left the one path the callback actually targets
  // silently dropping it, so both bounces are asserted here.
  it("carries an oauth_error from the index route to login", () => {
    useAuth.mockReturnValue({ isAdmin: false, isChecking: false, checkAuth });
    renderAt("/?oauth_error=access_denied");
    expect(screen.getByTestId("login-search")).toHaveTextContent(
      "login-page?oauth_error=access_denied",
    );
  });

  it("carries an oauth_error from a guarded route to login", () => {
    useAuth.mockReturnValue({ isAdmin: false, isChecking: false, checkAuth });
    renderAt("/devices?oauth_error=server_error");
    expect(screen.getByTestId("login-search")).toHaveTextContent(
      "login-page?oauth_error=server_error",
    );
  });

  it("adds no query when there was no oauth failure", () => {
    useAuth.mockReturnValue({ isAdmin: false, isChecking: false, checkAuth });
    renderAt("/");
    expect(screen.getByTestId("login-search")).toHaveTextContent("login-page");
    expect(screen.getByTestId("login-search").textContent).not.toContain("?");
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
