import { screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  useSystemStatus: vi.fn(),
  useDevices: vi.fn(),
  useTunnels: vi.fn(),
  useDefaultPolicy: vi.fn(),
  useDaemonStatus: vi.fn(),
  useDashboardDnsStats: vi.fn(),
  ctx: { value: { showingLastKnownState: false } },
}));
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useSystemStatus: h.useSystemStatus,
    useDevices: h.useDevices,
    useTunnels: h.useTunnels,
    useDefaultPolicy: h.useDefaultPolicy,
    useDaemonStatus: h.useDaemonStatus,
    useDashboardDnsStats: h.useDashboardDnsStats,
  };
});
vi.mock("@/context/OnlineStatusContext", () => ({
  useOnlineStatusContext: () => h.ctx.value,
}));

import Dashboard from "@/pages/Dashboard";
import { renderWithProviders, makeDevice, makeTunnel } from "../test-utils";

describe("Dashboard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.ctx.value = { showingLastKnownState: false };
    h.useSystemStatus.mockReturnValue({
      data: { device_count: 3, tunnel_count: 2, tunnel_active_count: 1 },
    });
    h.useDevices.mockReturnValue({ data: { devices: [makeDevice()] } });
    h.useTunnels.mockReturnValue({ data: { tunnels: [makeTunnel()] } });
    h.useDefaultPolicy.mockReturnValue({ data: { policy: "direct" } });
    h.useDaemonStatus.mockReturnValue({
      data: { reachable: true, uptimeSeconds: 60 },
    });
    h.useDashboardDnsStats.mockReturnValue({
      data: {
        total: 10,
        blocked: 1,
        blockedPercent: 10,
        totalSeries: [],
        blockedSeries: [],
      },
      isLoading: false,
    });
  });

  it("renders every dashboard card", () => {
    renderWithProviders(<Dashboard />);
    expect(screen.getByTestId("dashboard-status-card")).toBeInTheDocument();
    expect(screen.getByTestId("dashboard-devices-card")).toBeInTheDocument();
    expect(screen.getByTestId("dashboard-tunnels-card")).toBeInTheDocument();
    expect(
      screen.getByTestId("dashboard-dns-queries-card"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("dashboard-blocked-card")).toBeInTheDocument();
  });

  it("falls back to defaults when queries have no data", () => {
    h.useSystemStatus.mockReturnValue({ data: undefined });
    h.useDevices.mockReturnValue({ data: undefined });
    h.useTunnels.mockReturnValue({ data: undefined });
    h.useDefaultPolicy.mockReturnValue({ data: undefined });
    h.useDaemonStatus.mockReturnValue({ data: undefined });
    h.useDashboardDnsStats.mockReturnValue({
      data: undefined,
      isLoading: true,
    });
    renderWithProviders(<Dashboard />);
    expect(screen.getByText("Daemon Unreachable")).toBeInTheDocument();
  });

  it("dims the content while showing last-known state", () => {
    h.ctx.value = { showingLastKnownState: true };
    const { container } = renderWithProviders(<Dashboard />);
    expect(container.querySelector(".opacity-40")).not.toBeNull();
  });
});
