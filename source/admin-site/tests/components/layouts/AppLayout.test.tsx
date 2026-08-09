import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useAuth,
  useDaemonStatus,
  useUpdateStatus,
  useSystemStatus,
  useAcknowledgeShutdown,
} = vi.hoisted(() => ({
  useAuth: vi.fn(),
  useDaemonStatus: vi.fn(),
  useUpdateStatus: vi.fn(),
  useSystemStatus: vi.fn(),
  useAcknowledgeShutdown: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useAuth,
    useDaemonStatus,
    useUpdateStatus,
    useSystemStatus,
    useAcknowledgeShutdown,
  };
});

// Stub the compound children (owned/tested elsewhere) so this test isolates
// the layout shell — the harnesses surface the props the layout wired.
vi.mock("@/components/compound/Sidebar", () => ({
  Sidebar: ({
    version,
    connected,
    updateAvailable,
  }: {
    version?: string;
    connected: boolean;
    updateAvailable: boolean;
  }) => (
    <nav
      data-version={version ?? "-"}
      data-connected={String(connected)}
      data-update={String(updateAvailable)}
    >
      sidebar
    </nav>
  ),
}));
vi.mock("@/components/compound/MobileMenu", () => ({
  MobileMenu: () => <button>menu</button>,
}));
vi.mock("@/components/compound/ConnectionBanner", () => ({
  ConnectionBanner: ({ reachable }: { reachable: boolean | undefined }) => (
    <div data-testid="connection-banner" data-reachable={String(reachable)} />
  ),
}));
vi.mock("@/components/compound/UncleanShutdownBanner", () => ({
  UncleanShutdownBanner: ({ onDismiss }: { onDismiss: () => void }) => (
    <div>
      shutdown-banner
      <button onClick={() => onDismiss()}>dismiss-shutdown</button>
    </div>
  ),
}));

import { AppLayout } from "@/components/layouts/AppLayout";

const ackMutate = vi.fn();

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<AppLayout />}>
          <Route path="*" element={<div>page body</div>} />
        </Route>
      </Routes>
    </MemoryRouter>,
  );
}

describe("AppLayout", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useDaemonStatus.mockReturnValue({
      data: { version: "1.2.3", reachable: true },
      isLoading: false,
    });
    useUpdateStatus.mockReturnValue({
      data: { status: { update_available: true, latest_version: "9.9.9" } },
    });
    useSystemStatus.mockReturnValue({ data: undefined });
    useAcknowledgeShutdown.mockReturnValue({
      mutate: ackMutate,
      isPending: false,
    });
  });

  it("derives the breadcrumb from the path and renders admin chrome", () => {
    useAuth.mockReturnValue({ isAdmin: true, logout: vi.fn() });
    renderAt("/devices");
    expect(screen.getByText("Devices")).toBeInTheDocument();
    expect(screen.getByText("menu")).toBeInTheDocument();
    expect(screen.getByText("shutdown-banner")).toBeInTheDocument();
    expect(screen.getByText("page body")).toBeInTheDocument();
  });

  it("shows the Dashboard crumb at root and hides admin-only chrome for non-admins", () => {
    useAuth.mockReturnValue({ isAdmin: false, logout: vi.fn() });
    renderAt("/");
    expect(screen.getByText("Dashboard")).toBeInTheDocument();
    expect(screen.queryByText("menu")).not.toBeInTheDocument();
    expect(screen.queryByText("shutdown-banner")).not.toBeInTheDocument();
  });

  it("passes the shell status data down to the sidebar and banners", () => {
    useAuth.mockReturnValue({ isAdmin: true, logout: vi.fn() });
    renderAt("/devices");
    const sidebar = screen.getByText("sidebar");
    expect(sidebar).toHaveAttribute("data-version", "1.2.3");
    expect(sidebar).toHaveAttribute("data-connected", "true");
    expect(sidebar).toHaveAttribute("data-update", "true");
    expect(screen.getByTestId("connection-banner")).toHaveAttribute(
      "data-reachable",
      "true",
    );
  });

  it("wires the shutdown acknowledgement mutation to the banner", async () => {
    useAuth.mockReturnValue({ isAdmin: true, logout: vi.fn() });
    renderAt("/devices");
    screen.getByText("dismiss-shutdown").click();
    expect(ackMutate).toHaveBeenCalled();
  });
});
