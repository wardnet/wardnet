import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  useTunnels: vi.fn(),
  useDevices: vi.fn(),
  useDefaultPolicy: vi.fn(),
  useTunnelStats: vi.fn(),
  useCombinedTunnelStats: vi.fn(),
  useSpeedTestResults: vi.fn(),
  rebuildMutate: vi.fn(),
  startSpeedTest: vi.fn(),
  useStartSpeedTest: vi.fn(),
  useRebuildTunnel: vi.fn(),
  ctx: { value: { showingLastKnownState: false } },
}));
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useTunnels: h.useTunnels,
    useDevices: h.useDevices,
    useDefaultPolicy: h.useDefaultPolicy,
    useTunnelStats: h.useTunnelStats,
    useCombinedTunnelStats: h.useCombinedTunnelStats,
    useSpeedTestResults: h.useSpeedTestResults,
    useStartSpeedTest: h.useStartSpeedTest,
    useRebuildTunnel: h.useRebuildTunnel,
  };
});
vi.mock("@/context/OnlineStatusContext", () => ({
  useOnlineStatusContext: () => h.ctx.value,
}));

import Tunnels from "@/pages/Tunnels";
import { renderWithProviders, makeDevice, makeTunnel } from "../test-utils";

const speedResult = {
  id: "r1",
  tunnel_throughput_mbps: 90,
  direct_throughput_mbps: 100,
  tunnel_latency_ms: 20,
  direct_latency_ms: 10,
  tested_at: "2026-01-01T00:00:00Z",
};

describe("Tunnels page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.ctx.value = { showingLastKnownState: false };
    h.useDevices.mockReturnValue({ data: { devices: [makeDevice()] } });
    h.useDefaultPolicy.mockReturnValue({ data: { policy: "direct" } });
    h.useTunnelStats.mockReturnValue({ data: { points: [{ bytesRx: 6000, bytesTx: 1200 }] } });
    h.useCombinedTunnelStats.mockReturnValue({ data: { sparkValues: [1, 2, 3], rxRate: 100, txRate: 50 } });
    h.useSpeedTestResults.mockReturnValue({ data: { results: [] } });
    h.useStartSpeedTest.mockReturnValue({
      isRunning: false,
      activeTunnelId: null,
      percentage: 0,
      start: h.startSpeedTest,
    });
    h.useRebuildTunnel.mockReturnValue({ mutate: h.rebuildMutate, isPending: false, variables: undefined });
  });

  it("shows a skeleton while loading", () => {
    h.useTunnels.mockReturnValue({ data: undefined, isLoading: true });
    const { container } = renderWithProviders(<Tunnels />);
    expect(screen.getByText("Tunnels")).toBeInTheDocument();
    expect(container.querySelector(".animate-pulse")).not.toBeNull();
  });

  it("shows the empty message when there are no tunnels", () => {
    h.useTunnels.mockReturnValue({ data: { tunnels: [] }, isLoading: false });
    renderWithProviders(<Tunnels />);
    expect(screen.getByText("No tunnels configured.")).toBeInTheDocument();
    expect(screen.getByTestId("tunnels-summary")).toBeInTheDocument();
  });

  it("renders a tunnel card with its summary and throughput", () => {
    h.useTunnels.mockReturnValue({
      isLoading: false,
      data: { tunnels: [makeTunnel({ id: "t1", label: "US East", status: "up" })] },
    });
    renderWithProviders(<Tunnels />);
    expect(screen.getByTestId("tunnel-card")).toBeInTheDocument();
    expect(screen.getByText("/ 1")).toBeInTheDocument();
    expect(screen.getByText("0 routed")).toBeInTheDocument();
  });

  it("rebuilds a tunnel when the rebuild button is pressed", async () => {
    h.useTunnels.mockReturnValue({
      isLoading: false,
      data: { tunnels: [makeTunnel({ id: "t1", status: "down" })] },
    });
    renderWithProviders(<Tunnels />);
    await userEvent.click(screen.getByTestId("tunnel-rebuild"));
    expect(h.rebuildMutate).toHaveBeenCalledWith("t1");
  });

  it("starts a speed test and shows the latest result", async () => {
    h.useTunnels.mockReturnValue({
      isLoading: false,
      data: { tunnels: [makeTunnel({ id: "t1", status: "up" })] },
    });
    h.useSpeedTestResults.mockReturnValue({
      data: { results: [speedResult, { ...speedResult, id: "r2" }] },
    });
    renderWithProviders(<Tunnels />);
    await userEvent.click(screen.getByTestId("tunnel-speed-test-button"));
    expect(h.startSpeedTest).toHaveBeenCalledWith("t1");

    const result = screen.getByTestId("tunnel-speed-test-result");
    expect(result).toBeInTheDocument();
    // Expand to reveal the history rows.
    await userEvent.click(result.querySelector("button")!);
    expect(screen.getByText(/% kept/)).toBeInTheDocument();
  });
});
