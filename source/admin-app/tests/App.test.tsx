import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  checkAuth: vi.fn(),
  refresh: vi.fn(),
  useAuth: vi.fn(),
  useSetupStatus: vi.fn(),
  isRegistered: vi.fn(),
  authenticate: vi.fn(),
}));
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useAuth: h.useAuth, useSetupStatus: h.useSetupStatus };
});
vi.mock("@/hooks/useBiometric", () => ({
  useBiometric: () => ({
    isRegistered: h.isRegistered,
    authenticate: h.authenticate,
  }),
}));
vi.mock("@/context/OnlineStatusContext", () => ({
  OnlineStatusProvider: ({ children }: { children: React.ReactNode }) => (
    <>{children}</>
  ),
}));
vi.mock("@/components/BiometricGate", () => ({
  BiometricGate: ({
    onSuccess,
    onUsePassword,
  }: {
    onSuccess: () => void;
    onUsePassword: () => void;
  }) => (
    <div>
      <span>BIOMETRIC-GATE</span>
      <button onClick={onSuccess}>gate-success</button>
      <button onClick={onUsePassword}>gate-password</button>
    </div>
  ),
}));
vi.mock("@/layouts/AppLayout", async () => {
  const { Outlet } = await import("react-router");
  return {
    AppLayout: () => (
      <div>
        APP-LAYOUT
        <Outlet />
      </div>
    ),
  };
});
vi.mock("@/layouts/AuthLayout", async () => {
  const { Outlet } = await import("react-router");
  return {
    AuthLayout: () => (
      <div>
        AUTH-LAYOUT
        <Outlet />
      </div>
    ),
  };
});
vi.mock("@/pages/Dashboard", () => ({ default: () => <div>DASHBOARD</div> }));
vi.mock("@/pages/Login", () => ({ default: () => <div>LOGIN</div> }));
vi.mock("@/pages/Devices", () => ({ default: () => <div>DEVICES</div> }));
vi.mock("@/pages/Tunnels", () => ({ default: () => <div>TUNNELS</div> }));
vi.mock("@/pages/Dns", () => ({ default: () => <div>DNS</div> }));
vi.mock("@/pages/System", () => ({ default: () => <div>SYSTEM</div> }));

import App from "@/App";

function renderApp(route = "/") {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[route]}>
        <App />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("App", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.checkAuth.mockResolvedValue(undefined);
    h.refresh.mockResolvedValue(undefined);
    h.useAuth.mockReturnValue({
      checkAuth: h.checkAuth,
      refresh: h.refresh,
      isAdmin: true,
      isChecking: false,
    });
    h.useSetupStatus.mockReturnValue({
      data: { wizard_step: "completed" },
      isLoading: false,
    });
    h.isRegistered.mockReturnValue(false);
  });

  it("boots then renders the dashboard for an authenticated admin", async () => {
    renderApp("/");
    expect(await screen.findByText("DASHBOARD")).toBeInTheDocument();
    expect(screen.getByText("APP-LAYOUT")).toBeInTheDocument();
    expect(h.checkAuth).toHaveBeenCalledOnce();
    expect(h.refresh).toHaveBeenCalledOnce();
  });

  it("redirects to login when not an admin", async () => {
    h.useAuth.mockReturnValue({
      checkAuth: h.checkAuth,
      refresh: h.refresh,
      isAdmin: false,
      isChecking: false,
    });
    renderApp("/");
    expect(await screen.findByText("LOGIN")).toBeInTheDocument();
  });

  it("shows the setup interstitial when the wizard is incomplete", async () => {
    h.useSetupStatus.mockReturnValue({
      data: { wizard_step: "network" },
      isLoading: false,
    });
    renderApp("/");
    expect(
      await screen.findByText("Initial setup required"),
    ).toBeInTheDocument();
    // The label is wrapped in a <Text> span, so query the anchor by role
    // rather than by its text node.
    expect(screen.getByRole("link", { name: "Go to setup" })).toHaveAttribute(
      "href",
      expect.stringContaining("returnTo=/admin-app/"),
    );
  });

  it("shows the biometric gate when a credential is registered, then unlocks", async () => {
    h.isRegistered.mockReturnValue(true);
    renderApp("/");
    expect(await screen.findByText("BIOMETRIC-GATE")).toBeInTheDocument();
    await userEvent.click(screen.getByText("gate-success"));
    expect(await screen.findByText("DASHBOARD")).toBeInTheDocument();
  });

  it("navigates devices route to the Devices page", async () => {
    renderApp("/devices");
    expect(await screen.findByText("DEVICES")).toBeInTheDocument();
  });

  it("renders null while setup status is loading", async () => {
    h.useSetupStatus.mockReturnValue({ data: undefined, isLoading: true });
    renderApp("/");
    // Boot resolves but SetupGuard returns null while loading — no page text.
    await waitFor(() => expect(h.checkAuth).toHaveBeenCalled());
    expect(screen.queryByText("DASHBOARD")).not.toBeInTheDocument();
  });
});
