import { screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useSystemStatus,
  useDevices,
  useTunnels,
  useDhcpStatus,
  useRecentErrors,
  useDnsStatSummary,
  useTlsStatus,
} = vi.hoisted(() => ({
  useSystemStatus: vi.fn(),
  useDevices: vi.fn(),
  useTunnels: vi.fn(),
  useDhcpStatus: vi.fn(),
  useRecentErrors: vi.fn(),
  useDnsStatSummary: vi.fn(),
  useTlsStatus: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useSystemStatus,
    useDevices,
    useTunnels,
    useDhcpStatus,
    useRecentErrors,
    useDnsStatSummary,
    useTlsStatus,
  };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: { title: ReactNode }) => <h1>{title}</h1>,
}));
vi.mock("@/components/compound/DashboardStatCard", () => ({
  DashboardStatCard: ({
    testId,
    title,
    value,
    subtitle,
    error,
  }: {
    testId: string;
    title: ReactNode;
    value: unknown;
    subtitle?: ReactNode;
    error?: ReactNode;
  }) => (
    <div data-testid={testId}>
      <span>{title}</span>
      <span data-testid={`${testId}-value`}>{String(value)}</span>
      {subtitle != null && (
        <span data-testid={`${testId}-sub`}>{subtitle}</span>
      )}
      {error && <span data-testid={`${testId}-err`}>{error}</span>}
    </div>
  ),
}));
vi.mock("@/components/compound/DhcpSummaryCard", () => ({
  DhcpSummaryCard: ({ status }: { status: unknown }) => (
    <div data-testid="dhcp-summary">{status ? "dhcp" : "no-dhcp"}</div>
  ),
}));
vi.mock("@/components/compound/RecentErrorsCard", () => ({
  RecentErrorsCard: ({ errors }: { errors: unknown[] }) => (
    <div data-testid="recent-errors">{errors.length}</div>
  ),
}));
vi.mock("@/components/features/DashboardLogWidget", () => ({
  DashboardLogWidget: () => <div data-testid="log-widget" />,
}));
vi.mock("@/components/features/DashboardRemoteAccessBanner", () => ({
  DashboardRemoteAccessBanner: () => <div data-testid="remote-banner" />,
}));

import Dashboard from "@/pages/Dashboard";
import { renderWithProviders } from "../test-utils";

const fullStatus = {
  uptime_seconds: 3600,
  release_version: "1.2.3",
  cpu_usage_percent: 12.34,
  memory_used_bytes: 500,
  memory_total_bytes: 1000,
  disk_free_bytes: 200,
  disk_total_bytes: 1000,
  device_count: 7,
  tunnel_count: 3,
};

beforeEach(() => {
  vi.clearAllMocks();
  useSystemStatus.mockReturnValue({ data: undefined });
  useDevices.mockReturnValue({ data: undefined });
  useTunnels.mockReturnValue({ data: undefined });
  useDhcpStatus.mockReturnValue({ data: undefined });
  useRecentErrors.mockReturnValue({ data: undefined });
  useTlsStatus.mockReturnValue({ data: undefined });
  useDnsStatSummary.mockReturnValue({
    data: undefined,
    isError: false,
    error: undefined,
  });
});

describe("Dashboard", () => {
  it("renders fallbacks when no data present", () => {
    renderWithProviders(<Dashboard />);
    expect(screen.getByTestId("stat-devices-value")).toHaveTextContent("0");
    expect(screen.getByTestId("stat-tunnels-value")).toHaveTextContent("0");
    // No system status → status cards hidden.
    expect(screen.queryByTestId("stat-uptime")).not.toBeInTheDocument();
    // dnsStats undefined → em dash.
    expect(screen.getByTestId("stat-dns-queries-value")).toHaveTextContent("-");
    expect(screen.getByTestId("stat-blocked-value")).toHaveTextContent("-");
    expect(screen.getByTestId("recent-errors")).toHaveTextContent("0");
    expect(screen.getByTestId("log-widget")).toBeInTheDocument();
  });

  it("uses status counts when device/tunnel lists absent", () => {
    useSystemStatus.mockReturnValue({ data: fullStatus });
    renderWithProviders(<Dashboard />);
    expect(screen.getByTestId("stat-devices-value")).toHaveTextContent("7");
    expect(screen.getByTestId("stat-tunnels-value")).toHaveTextContent("3");
    // Status cards visible.
    expect(screen.getByTestId("stat-uptime")).toBeInTheDocument();
    expect(screen.getByTestId("stat-cpu-value")).toHaveTextContent("12.3%");
    expect(screen.getByTestId("dhcp-summary")).toHaveTextContent("no-dhcp");
  });

  it("prefers device/tunnel list lengths and computes active tunnels", () => {
    useDevices.mockReturnValue({
      data: { devices: [{ id: "d1" }, { id: "d2" }] },
    });
    useTunnels.mockReturnValue({
      data: {
        tunnels: [{ status: "up" }, { status: "down" }, { status: "up" }],
      },
    });
    useDhcpStatus.mockReturnValue({ data: { enabled: true } });
    useSystemStatus.mockReturnValue({ data: fullStatus });
    useDnsStatSummary.mockReturnValue({
      data: { total: 1234, blocked: 123, blockedPercent: 9.9 },
      isError: false,
      error: undefined,
    });
    renderWithProviders(<Dashboard />);
    expect(screen.getByTestId("stat-devices-value")).toHaveTextContent("2");
    expect(screen.getByTestId("stat-tunnels-value")).toHaveTextContent("3");
    expect(screen.getByTestId("stat-tunnels-sub")).toHaveTextContent(
      "2 active",
    );
    expect(screen.getByTestId("dhcp-summary")).toHaveTextContent("dhcp");
    expect(screen.getByTestId("stat-dns-queries-value")).toHaveTextContent(
      "1,234",
    );
    expect(screen.getByTestId("stat-blocked-value")).toHaveTextContent("9.9%");
    expect(screen.getByTestId("stat-blocked-sub")).toHaveTextContent(
      "123 of 1,234",
    );
  });

  it("handles zero memory/disk totals without dividing by zero", () => {
    useSystemStatus.mockReturnValue({
      data: { ...fullStatus, memory_total_bytes: 0, disk_total_bytes: 0 },
    });
    renderWithProviders(<Dashboard />);
    expect(screen.getByTestId("stat-memory")).toBeInTheDocument();
    expect(screen.getByTestId("stat-disk")).toBeInTheDocument();
  });

  it("surfaces a DNS stats error message (Error instance)", () => {
    useDnsStatSummary.mockReturnValue({
      data: undefined,
      isError: true,
      error: new Error("boom"),
    });
    renderWithProviders(<Dashboard />);
    expect(screen.getByTestId("stat-dns-queries-err")).toHaveTextContent(
      "boom",
    );
  });

  it("uses fallback message for non-Error DNS stats error", () => {
    useDnsStatSummary.mockReturnValue({
      data: undefined,
      isError: true,
      error: "nope",
    });
    renderWithProviders(<Dashboard />);
    expect(screen.getByTestId("stat-dns-queries-err")).toHaveTextContent(
      "Failed to load DNS stats",
    );
  });

  it("shows recent errors count", () => {
    useRecentErrors.mockReturnValue({
      data: { errors: [{ id: 1 }, { id: 2 }] },
    });
    renderWithProviders(<Dashboard />);
    expect(screen.getByTestId("recent-errors")).toHaveTextContent("2");
  });
});
