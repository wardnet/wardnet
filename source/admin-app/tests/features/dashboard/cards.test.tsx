import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StatusCard } from "@/features/dashboard/StatusCard";
import { DevicesCard } from "@/features/dashboard/DevicesCard";
import { TunnelsCard } from "@/features/dashboard/TunnelsCard";
import { DnsQueriesCard, BlockedCard } from "@/features/dashboard/DnsCard";
import { renderWithProviders, makeDevice, makeTunnel } from "../../test-utils";

const now = new Date().toISOString();
const old = "2000-01-01T00:00:00Z";

describe("StatusCard", () => {
  it("renders the healthy badge and uptime", () => {
    renderWithProviders(<StatusCard reachable uptimeSeconds={120} />);
    expect(screen.getByText("All Systems Healthy")).toBeInTheDocument();
    expect(screen.getByTestId("dashboard-status-card")).toBeInTheDocument();
  });

  it("renders the unreachable badge and an em dash for null uptime", () => {
    renderWithProviders(<StatusCard reachable={false} uptimeSeconds={null} />);
    expect(screen.getByText("Daemon Unreachable")).toBeInTheDocument();
    expect(screen.getByText("-")).toBeInTheDocument();
  });
});

describe("DevicesCard", () => {
  it("shows online count and tunnelled count", () => {
    renderWithProviders(
      <DevicesCard
        deviceCount={2}
        devices={[
          makeDevice({
            id: "a",
            last_seen: now,
            current_rule: { type: "tunnel", tunnel_id: "t" },
          }),
          makeDevice({ id: "b", last_seen: old }),
        ]}
        defaultPolicy="direct"
      />,
    );
    expect(screen.getByTestId("dashboard-devices-card")).toHaveAttribute(
      "href",
      "/devices",
    );
    expect(screen.getByText("1 routed through a tunnel")).toBeInTheDocument();
  });

  it("counts default-policy devices as tunnelled when default is a tunnel", () => {
    renderWithProviders(
      <DevicesCard
        deviceCount={1}
        devices={[makeDevice({ id: "a", last_seen: now, current_rule: null })]}
        defaultPolicy="tunnel-x"
      />,
    );
    expect(screen.getByText("1 routed through a tunnel")).toBeInTheDocument();
  });

  it("renders an em dash and no tunnel line when devices are undefined", () => {
    renderWithProviders(
      <DevicesCard
        deviceCount={5}
        devices={undefined}
        defaultPolicy={undefined}
      />,
    );
    expect(screen.getByText("-")).toBeInTheDocument();
    expect(
      screen.queryByText(/routed through a tunnel/),
    ).not.toBeInTheDocument();
  });
});

describe("TunnelsCard", () => {
  it("shows the primary active tunnel and a '+N more' line", () => {
    renderWithProviders(
      <TunnelsCard
        tunnelCount={3}
        tunnelActiveCount={2}
        tunnels={[
          makeTunnel({
            id: "t1",
            label: "US",
            status: "up",
            resolved_server_name: "US #1",
          }),
          makeTunnel({ id: "t2", label: "JP", status: "up" }),
          makeTunnel({ id: "t3", label: "DE", status: "down" }),
        ]}
      />,
    );
    expect(screen.getByText("US #1")).toBeInTheDocument();
    expect(screen.getByText("+1 more")).toBeInTheDocument();
  });

  it("renders without a primary when no tunnels are up", () => {
    renderWithProviders(
      <TunnelsCard tunnelCount={0} tunnelActiveCount={0} tunnels={[]} />,
    );
    expect(screen.getByText("Tunnels Up")).toBeInTheDocument();
    expect(screen.queryByText(/more/)).not.toBeInTheDocument();
  });
});

const dnsStats = {
  total: 1234,
  blocked: 100,
  blockedPercent: 8.1,
  totalSeries: [1, 2, 3],
  blockedSeries: [0, 1, 0],
};

describe("DnsQueriesCard", () => {
  it("shows the total and a sparkline when data is present", () => {
    renderWithProviders(<DnsQueriesCard data={dnsStats} isLoading={false} />);
    expect(screen.getByText("1,234")).toBeInTheDocument();
    expect(screen.getByTestId("dashboard-dns-queries-card")).toHaveAttribute(
      "href",
      "/dns",
    );
  });

  it("shows an ellipsis while loading with no data", () => {
    renderWithProviders(<DnsQueriesCard data={undefined} isLoading />);
    expect(screen.getByText("…")).toBeInTheDocument();
  });
});

describe("BlockedCard", () => {
  it("shows the blocked percent and the of-total subline", () => {
    renderWithProviders(<BlockedCard data={dnsStats} isLoading={false} />);
    expect(screen.getByText("8.1")).toBeInTheDocument();
    expect(screen.getByText("100 of 1,234")).toBeInTheDocument();
  });

  it("shows an ellipsis while loading with no data", () => {
    renderWithProviders(<BlockedCard data={undefined} isLoading />);
    expect(screen.getByText("…")).toBeInTheDocument();
  });
});
