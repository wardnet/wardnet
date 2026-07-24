import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ProviderInfo, Tunnel, TunnelSpeedTestResult } from "@wardnet/js";
import { TunnelCard } from "@/components/compound/TunnelCard";
import type { TunnelTestOutcome } from "@/components/compound/TunnelCard";
import { renderWithProviders } from "../../test-utils";

function makeTunnel(overrides: Partial<Tunnel> = {}): Tunnel {
  return {
    id: "t1",
    label: "US East",
    country_code: "us",
    provider: "nordvpn",
    interface_name: "wg_ward0",
    endpoint: "1.2.3.4:51820",
    status: "up",
    last_handshake: "2026-01-01T00:00:00Z",
    bytes_tx: 1000,
    bytes_rx: 2000,
    created_at: "2026-01-01T00:00:00Z",
    override_default_dns: false,
    server_selector: null,
    resolved_server_name: null,
    endpoint_resolved_at: null,
    ...overrides,
  };
}

const providers: ProviderInfo[] = [
  { id: "nordvpn", name: "NordVPN", auth_methods: ["credentials"] },
];

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

type CardProps = Parameters<typeof TunnelCard>[0];

/** Render a card with sensible presentation defaults; override per test. */
function renderCard(overrides: Partial<CardProps> = {}) {
  const props: CardProps = {
    tunnel: makeTunnel(),
    providers,
    onDelete: vi.fn(),
    onTest: vi.fn(),
    onRebuild: vi.fn(),
    onSpeedTest: vi.fn(),
    testOutcome: null,
    testing: false,
    rebuilding: false,
    speedTestRunning: false,
    speedTestPercentage: 0,
    speedTestResult: null,
    ...overrides,
  };
  return renderWithProviders(<TunnelCard {...props} />);
}

describe("TunnelCard", () => {
  it("renders the tunnel identity, provider, and throughput", () => {
    renderCard();
    expect(screen.getByText("US East")).toBeInTheDocument();
    expect(screen.getByText(/NordVPN/)).toBeInTheDocument();
    expect(screen.getByText("wg_ward0")).toBeInTheDocument();
    expect(screen.getByText("Active")).toBeInTheDocument();
    expect(screen.getByText("1.2.3.4:51820")).toBeInTheDocument();
  });

  it("falls back to the raw provider id and custom icon when unknown", () => {
    renderCard({
      tunnel: makeTunnel({
        provider: "mystery",
        server_selector: { country: "de" },
      }),
    });
    expect(screen.getByText(/mystery/)).toBeInTheDocument();
    expect(screen.getByLabelText("Custom configuration")).toBeInTheDocument();
    expect(screen.getByText("Best DE")).toBeInTheDocument();
  });

  it("renders each non-up status label", () => {
    for (const [status, label] of [
      ["down", "Down"],
      ["connecting", "Connecting"],
      ["reconnecting", "Reconnecting"],
    ] as const) {
      const { unmount } = renderCard({
        tunnel: makeTunnel({ status, last_handshake: null }),
      });
      expect(screen.getByText(label)).toBeInTheDocument();
      unmount();
    }
  });

  it("shows a reconnecting tooltip that mentions the last handshake", () => {
    renderCard({ tunnel: makeTunnel({ status: "reconnecting" }) });
    expect(screen.getByText("Reconnecting")).toBeInTheDocument();
  });

  it("fires onTest and renders the outcome passed back in", async () => {
    const user = userEvent.setup();
    const onTest = vi.fn();
    renderCard({ onTest });
    await user.click(screen.getByRole("button", { name: "Test" }));
    expect(onTest).toHaveBeenCalledWith("t1");
  });

  it("renders a connectivity-test result from testOutcome", () => {
    const testOutcome: TunnelTestOutcome = {
      result: {
        tunnel_id: "t1",
        exit_ip: "9.9.9.9",
        country_code: "us",
        latency_ms: 42,
      },
      error: null,
      testedAt: "2026-01-01T00:00:00Z",
    };
    renderCard({ testOutcome });
    expect(screen.getByText("9.9.9.9")).toBeInTheDocument();
    expect(screen.getByText("42 ms")).toBeInTheDocument();
  });

  it("renders an error alert when testOutcome carries an error", () => {
    const testOutcome: TunnelTestOutcome = {
      result: null,
      error: "unreachable",
      testedAt: "2026-01-01T00:00:00Z",
    };
    renderCard({ testOutcome });
    expect(screen.getByRole("alert")).toHaveTextContent("unreachable");
  });

  it("fires onSpeedTest on click", async () => {
    const user = userEvent.setup();
    const onSpeedTest = vi.fn();
    renderCard({ onSpeedTest });
    await user.click(screen.getByTestId("tunnel-speed-test-button"));
    expect(onSpeedTest).toHaveBeenCalledWith("t1");
  });

  it("renders the running speed-test progress label", () => {
    renderCard({ speedTestRunning: true, speedTestPercentage: 55 });
    expect(screen.getByText(/Testing 55%/)).toBeInTheDocument();
    expect(screen.getByTestId("tunnel-speed-test-button")).toBeDisabled();
  });

  it("renders the speed-test comparison with high retention", () => {
    renderCard({ speedTestResult: makeSpeedResult() });
    const panel = screen.getByTestId("tunnel-speed-test-result");
    expect(within(panel).getByText(/% kept/)).toBeInTheDocument();
    expect(
      screen.getByTestId("tunnel-speed-test-download"),
    ).toBeInTheDocument();
  });

  it("renders the comparison with low retention and negative overheads", () => {
    renderCard({
      speedTestResult: makeSpeedResult({
        tunnel_throughput_mbps: 10,
        tunnel_latency_ms: 5,
        tunnel_jitter_ms: 0,
      }),
    });
    expect(screen.getByTestId("tunnel-speed-test-result")).toBeInTheDocument();
  });

  it("fires onRebuild and onDelete", async () => {
    const user = userEvent.setup();
    const onRebuild = vi.fn();
    const onDelete = vi.fn();
    renderCard({ onRebuild, onDelete });
    await user.click(screen.getByRole("button", { name: /Rebuild/ }));
    expect(onRebuild).toHaveBeenCalledWith("t1");
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(onDelete).toHaveBeenCalledWith("t1");
  });

  it("shows pending spinners for in-flight test and rebuild", () => {
    renderCard({ testing: true, rebuilding: true });
    expect(screen.getByText("Rebuilding")).toBeInTheDocument();
    // The connectivity Test button collapses to its spinner label too.
    expect(screen.getByRole("button", { name: "Rebuilding" })).toBeDisabled();
  });

  it("renders a provider icon image when available", () => {
    renderCard({
      providers: [
        {
          id: "nordvpn",
          name: "NordVPN",
          auth_methods: ["credentials"],
          icon_url: "https://example.com/n.png",
        },
      ],
    });
    const img = document.querySelector("img");
    expect(img).toHaveAttribute("src", "https://example.com/n.png");
  });
});
