import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Tunnel, ProviderInfo, TunnelSpeedTestResult } from "@wardnet/js";
import { TunnelGrid } from "@/components/compound/TunnelGrid";
import { renderWithProviders } from "../../test-utils";

const providers: ProviderInfo[] = [
  { id: "nord", name: "NordVPN" } as ProviderInfo,
];

function makeTunnel(overrides: Partial<Tunnel> = {}): Tunnel {
  return {
    id: "t1",
    label: "My tunnel",
    provider: "nord",
    interface_name: "wg_ward0",
    country_code: "us",
    status: "up",
    endpoint: "1.2.3.4:51820",
    last_handshake: null,
    bytes_tx: 1000,
    bytes_rx: 2000,
    ...overrides,
  } as Tunnel;
}

function makeSpeedResult(
  overrides: Partial<TunnelSpeedTestResult> = {},
): TunnelSpeedTestResult {
  return {
    id: "s1",
    tunnel_id: "t1",
    direct_throughput_mbps: 100,
    tunnel_throughput_mbps: 90,
    direct_latency_ms: 10,
    tunnel_latency_ms: 15,
    direct_jitter_ms: 1,
    tunnel_jitter_ms: 2,
    tested_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

type GridProps = Parameters<typeof TunnelGrid>[0];

function renderGrid(overrides: Partial<GridProps> = {}) {
  const props: GridProps = {
    tunnels: [makeTunnel()],
    providers,
    isLoading: false,
    isError: false,
    onDelete: vi.fn(),
    onTest: vi.fn(),
    onRebuild: vi.fn(),
    onSpeedTest: vi.fn(),
    testOutcomes: {},
    speedTestResults: {},
    testingId: null,
    rebuildingId: null,
    speedTestRunningId: null,
    speedTestPercentage: 0,
    ...overrides,
  };
  return renderWithProviders(<TunnelGrid {...props} />);
}

describe("TunnelGrid", () => {
  it("shows a loading state", () => {
    renderGrid({ tunnels: [], isLoading: true });
    expect(screen.getByText("Loading tunnels...")).toBeInTheDocument();
  });

  it("shows the empty state with an Add action", async () => {
    const user = userEvent.setup();
    const onAdd = vi.fn();
    renderGrid({ tunnels: [], onAdd });
    expect(screen.getByText("No tunnels configured")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Add tunnel" }));
    expect(onAdd).toHaveBeenCalledTimes(1);
  });

  it("renders null when there are no tunnels and an error occurred", () => {
    const { container } = renderGrid({ tunnels: [], isError: true });
    expect(container).toBeEmptyDOMElement();
  });

  it("renders a card per tunnel and forwards delete", async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn();
    renderGrid({
      tunnels: [makeTunnel(), makeTunnel({ id: "t2", label: "Second" })],
      onDelete,
    });
    expect(screen.getByText("My tunnel")).toBeInTheDocument();
    expect(screen.getByText("Second")).toBeInTheDocument();

    const deleteButtons = screen.getAllByRole("button", { name: "Delete" });
    await user.click(deleteButtons[0]);
    expect(onDelete).toHaveBeenCalledWith("t1");
  });

  it("forwards per-tunnel action callbacks with the card's id", async () => {
    const user = userEvent.setup();
    const onTest = vi.fn();
    const onRebuild = vi.fn();
    const onSpeedTest = vi.fn();
    renderGrid({
      tunnels: [makeTunnel({ id: "t9", label: "Only" })],
      onTest,
      onRebuild,
      onSpeedTest,
    });
    await user.click(screen.getByRole("button", { name: "Test" }));
    await user.click(screen.getByRole("button", { name: /Rebuild/ }));
    await user.click(screen.getByTestId("tunnel-speed-test-button"));
    expect(onTest).toHaveBeenCalledWith("t9");
    expect(onRebuild).toHaveBeenCalledWith("t9");
    expect(onSpeedTest).toHaveBeenCalledWith("t9");
  });

  it("routes per-tunnel state to the matching card only", () => {
    renderGrid({
      tunnels: [
        makeTunnel({ id: "t1", label: "Alpha" }),
        makeTunnel({ id: "t2", label: "Beta" }),
      ],
      testingId: "t1",
      speedTestResults: { t2: makeSpeedResult({ tunnel_id: "t2" }) },
    });
    // Exactly one card shows the in-flight connectivity spinner ("Testing"),
    // and exactly one shows the speed-test comparison panel.
    expect(screen.getAllByText("Testing")).toHaveLength(1);
    expect(screen.getAllByTestId("tunnel-speed-test-result")).toHaveLength(1);
  });

  // Card order is the grid's job. Asserted on the rendered sequence so
  // dropping the sort can't pass silently.
  it("renders tunnel cards alphabetically regardless of the order passed in", () => {
    renderGrid({
      tunnels: [
        makeTunnel({ id: "t1", label: "zeta" }),
        makeTunnel({ id: "t2", label: "Alpha" }),
        makeTunnel({ id: "t3", label: "beta" }),
      ],
    });
    // Each card is a link wrapping the whole tunnel; assert on href order so
    // the check is about sequence, not card markup.
    expect(
      screen.getAllByRole("link").map((a) => a.getAttribute("href")),
    ).toEqual(
      ["/tunnels/t2", "/tunnels/t3", "/tunnels/t1"], // Alpha, beta, zeta
    );
  });
});
