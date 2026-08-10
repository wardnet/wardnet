import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useTunnel,
  useDeleteTunnel,
  useSetTunnelDnsOverride,
  useRebuildTunnel,
  useSpeedTestResults,
  useTunnelStats,
  useTunnelDevices,
  navigate,
} = vi.hoisted(() => ({
  useTunnel: vi.fn(),
  useDeleteTunnel: vi.fn(),
  useSetTunnelDnsOverride: vi.fn(),
  useRebuildTunnel: vi.fn(),
  useSpeedTestResults: vi.fn(),
  useTunnelStats: vi.fn(),
  useTunnelDevices: vi.fn(),
  navigate: vi.fn(),
}));

vi.mock("react-router", async (io) => {
  const actual = await io<typeof import("react-router")>();
  return {
    ...actual,
    useParams: () => ({ id: "t-1" }),
    useNavigate: () => navigate,
  };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useTunnel,
    useDeleteTunnel,
    useSetTunnelDnsOverride,
    useRebuildTunnel,
    useSpeedTestResults,
    useTunnelStats,
    useTunnelDevices,
  };
});

vi.mock("@/components/compound/DetailPageHeader", () => ({
  DetailPageHeader: ({
    itemLabel,
    status,
    meta,
  }: {
    itemLabel: ReactNode;
    status: ReactNode;
    meta: ReactNode;
  }) => (
    <div data-testid="detail-header">
      <span data-testid="item-label">{itemLabel}</span>
      <span data-testid="status">{status}</span>
      <span data-testid="meta">{meta}</span>
    </div>
  ),
}));
vi.mock("@/components/compound/StatusBadge", () => ({
  StatusBadge: ({ children, tone }: { children: ReactNode; tone?: string }) => (
    <span data-testid="status-badge" data-tone={tone}>
      {children}
    </span>
  ),
}));
vi.mock("@/components/features/TunnelDevicesTable", () => ({
  TunnelDevicesTable: () => <div data-testid="tunnel-devices-table" />,
}));
vi.mock("@/components/features/TunnelThroughputChart", () => ({
  TunnelThroughputChart: ({
    isLoading,
    isError,
  }: {
    isLoading?: boolean;
    isError?: boolean;
  }) => (
    <div data-testid="throughput-chart">
      {isLoading ? "loading" : isError ? "error" : "ok"}
    </div>
  ),
}));
vi.mock("@/components/features/TunnelLatencyChart", () => ({
  TunnelLatencyChart: () => <div data-testid="latency-chart" />,
}));

import TunnelDetail from "@/pages/TunnelDetail";
import { renderWithProviders } from "../test-utils";

function makeTunnel(overrides: Record<string, unknown> = {}) {
  return {
    id: "t-1",
    label: "My VPN",
    status: "up",
    country_code: "us",
    last_handshake: "2026-01-01T00:00:00Z",
    provider: "nord",
    endpoint: "1.2.3.4:51820",
    interface_name: "wg_ward0",
    server_selector: null,
    resolved_server_name: null,
    endpoint_resolved_at: null,
    override_default_dns: false,
    ...overrides,
  };
}

const deleteMutate = vi.fn();
const rebuildMutate = vi.fn();
const dnsMutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useDeleteTunnel.mockReturnValue({ mutate: deleteMutate });
  useRebuildTunnel.mockReturnValue({ mutate: rebuildMutate, isPending: false });
  useSetTunnelDnsOverride.mockReturnValue({
    mutate: dnsMutate,
    isPending: false,
  });
  useSpeedTestResults.mockReturnValue({ data: { results: [] } });
  useTunnelStats.mockReturnValue({
    data: undefined,
    isLoading: false,
    isError: false,
  });
  useTunnelDevices.mockReturnValue({
    data: undefined,
    isLoading: false,
    isError: false,
  });
  useTunnel.mockReturnValue({
    data: { tunnel: makeTunnel() },
    isLoading: false,
    isError: false,
  });
});

describe("TunnelDetail", () => {
  it("shows loading state", () => {
    useTunnel.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    });
    renderWithProviders(<TunnelDetail />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("shows not-found on error", () => {
    useTunnel.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
    });
    renderWithProviders(<TunnelDetail />);
    expect(screen.getByText("Tunnel not found")).toBeInTheDocument();
  });

  it("renders a populated tunnel with flag and configuration", () => {
    renderWithProviders(<TunnelDetail />);
    expect(screen.getByTestId("item-label")).toHaveTextContent("My VPN");
    expect(screen.getByTestId("status-badge")).toHaveTextContent("Active");
    expect(screen.getByTestId("tunnel-devices-table")).toBeInTheDocument();
    expect(screen.getByTestId("throughput-chart")).toHaveTextContent("ok");
  });

  it("renders server-selector fields when present", () => {
    useTunnel.mockReturnValue({
      data: {
        tunnel: makeTunnel({
          server_selector: { country: "de" },
          resolved_server_name: "de-berlin-1",
          endpoint_resolved_at: "2026-01-01T00:00:00Z",
        }),
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<TunnelDetail />);
    expect(screen.getByText("Best DE")).toBeInTheDocument();
    expect(screen.getByText("de-berlin-1")).toBeInTheDocument();
  });

  it("handles a tunnel with no country code and never-handshaked", () => {
    useTunnel.mockReturnValue({
      data: {
        tunnel: makeTunnel({ country_code: null, last_handshake: null }),
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<TunnelDetail />);
    expect(screen.getByTestId("meta")).toHaveTextContent("never");
  });

  it.each([
    ["up", "Active"],
    ["down", "Down"],
    ["connecting", "Connecting"],
    ["reconnecting", "Reconnecting"],
  ])("maps status %s to label %s", (status, label) => {
    useTunnel.mockReturnValue({
      data: { tunnel: makeTunnel({ status }) },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<TunnelDetail />);
    expect(screen.getByTestId("status-badge")).toHaveTextContent(label);
  });

  it("shows chart loading and error states", () => {
    useTunnelStats.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    });
    const { unmount } = renderWithProviders(<TunnelDetail />);
    expect(screen.getByTestId("throughput-chart")).toHaveTextContent("loading");
    unmount();

    useTunnelStats.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
    });
    renderWithProviders(<TunnelDetail />);
    expect(screen.getByTestId("throughput-chart")).toHaveTextContent("error");
  });

  it("renders speed test history when results exist", () => {
    useSpeedTestResults.mockReturnValue({
      data: {
        results: [
          {
            id: "r1",
            tested_at: "2026-01-01T00:00:00Z",
            tunnel_throughput_mbps: 90.5,
            direct_throughput_mbps: 100.2,
            tunnel_latency_ms: 20.4,
            direct_latency_ms: 10.1,
          },
        ],
      },
    });
    renderWithProviders(<TunnelDetail />);
    expect(screen.getByTestId("tunnel-speed-test-history")).toBeInTheDocument();
    expect(screen.getByText("Speed test history")).toBeInTheDocument();
  });

  it("changes the stats range via the tabs", async () => {
    const user = userEvent.setup();
    renderWithProviders(<TunnelDetail />);
    // Initially 24h; switch to 1h.
    await user.click(screen.getByRole("tab", { name: "1h" }));
    // useTunnelStats called again with new range at some point.
    expect(useTunnelStats).toHaveBeenCalledWith("t-1", "1h");
  });

  it("toggles the DNS override", async () => {
    const user = userEvent.setup();
    renderWithProviders(<TunnelDetail />);
    await user.click(screen.getByRole("switch"));
    expect(dnsMutate).toHaveBeenCalledWith({ id: "t-1", value: true });
  });

  it("rebuilds the tunnel", async () => {
    const user = userEvent.setup();
    renderWithProviders(<TunnelDetail />);
    await user.click(screen.getByRole("button", { name: "Rebuild tunnel" }));
    expect(rebuildMutate).toHaveBeenCalledWith("t-1");
  });

  it("shows the rebuilding label while pending", () => {
    useRebuildTunnel.mockReturnValue({
      mutate: rebuildMutate,
      isPending: true,
    });
    renderWithProviders(<TunnelDetail />);
    expect(screen.getByRole("button", { name: "Rebuilding…" })).toBeDisabled();
  });

  it("deletes the tunnel and navigates on success", async () => {
    deleteMutate.mockImplementation((_id, opts) => opts?.onSuccess?.());
    const user = userEvent.setup();
    renderWithProviders(<TunnelDetail />);
    await user.click(screen.getByRole("button", { name: "Delete tunnel" }));
    expect(deleteMutate).toHaveBeenCalled();
    expect(navigate).toHaveBeenCalledWith("/tunnels");
  });
});
