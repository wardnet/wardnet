import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderInfo, Tunnel, TunnelSpeedTestResult } from "@wardnet/js";
import { TunnelCard } from "@/components/compound/TunnelCard";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useTestTunnel: vi.fn(),
    useRebuildTunnel: vi.fn(),
    useSpeedTestResults: vi.fn(),
    useStartSpeedTest: vi.fn(),
  };
});

import {
  useTestTunnel,
  useRebuildTunnel,
  useSpeedTestResults,
  useStartSpeedTest,
} from "@wardnet/web";

const mockTest = vi.mocked(useTestTunnel);
const mockRebuild = vi.mocked(useRebuildTunnel);
const mockSpeedResults = vi.mocked(useSpeedTestResults);
const mockStartSpeed = vi.mocked(useStartSpeedTest);

const testMutate = vi.fn();
const rebuildMutate = vi.fn();
const startSpeed = vi.fn();

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

beforeEach(() => {
  testMutate.mockReset();
  rebuildMutate.mockReset();
  startSpeed.mockReset();
  mockTest.mockReturnValue({ mutate: testMutate, isPending: false } as never);
  mockRebuild.mockReturnValue({
    mutate: rebuildMutate,
    isPending: false,
    variables: undefined,
  } as never);
  mockStartSpeed.mockReturnValue({
    start: startSpeed,
    activeTunnelId: null,
    percentage: 0,
    isRunning: false,
  } as never);
  mockSpeedResults.mockReturnValue({ data: { results: [] } } as never);
});

describe("TunnelCard", () => {
  it("renders the tunnel identity, provider, and throughput", () => {
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel()}
        providers={providers}
        onDelete={vi.fn()}
      />,
    );
    expect(screen.getByText("US East")).toBeInTheDocument();
    expect(screen.getByText(/NordVPN/)).toBeInTheDocument();
    expect(screen.getByText("wg_ward0")).toBeInTheDocument();
    expect(screen.getByText("Active")).toBeInTheDocument();
    expect(screen.getByText("1.2.3.4:51820")).toBeInTheDocument();
  });

  it("falls back to the raw provider id and custom icon when unknown", () => {
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel({
          provider: "mystery",
          server_selector: { country: "de" },
        })}
        providers={providers}
        onDelete={vi.fn()}
      />,
    );
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
      const { unmount } = renderWithProviders(
        <TunnelCard
          tunnel={makeTunnel({ status, last_handshake: null })}
          providers={providers}
          onDelete={vi.fn()}
        />,
      );
      expect(screen.getByText(label)).toBeInTheDocument();
      unmount();
    }
  });

  it("shows a reconnecting tooltip that mentions the last handshake", () => {
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel({ status: "reconnecting" })}
        providers={providers}
        onDelete={vi.fn()}
      />,
    );
    expect(screen.getByText("Reconnecting")).toBeInTheDocument();
  });

  it("runs a connectivity test and shows the result", async () => {
    const user = userEvent.setup();
    testMutate.mockImplementation((_id, opts) =>
      opts.onSuccess({
        result: {
          tunnel_id: "t1",
          exit_ip: "9.9.9.9",
          country_code: "us",
          latency_ms: 42,
        },
      }),
    );
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel()}
        providers={providers}
        onDelete={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Test" }));
    expect(testMutate).toHaveBeenCalledWith("t1", expect.any(Object));
    expect(screen.getByText("9.9.9.9")).toBeInTheDocument();
    expect(screen.getByText("42 ms")).toBeInTheDocument();
  });

  it("shows an error alert when the connectivity test fails", async () => {
    const user = userEvent.setup();
    testMutate.mockImplementation((_id, opts) =>
      opts.onError(new Error("unreachable")),
    );
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel()}
        providers={providers}
        onDelete={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Test" }));
    expect(screen.getByRole("alert")).toHaveTextContent("unreachable");
  });

  it("starts a speed test on click and reflects a running test", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel()}
        providers={providers}
        onDelete={vi.fn()}
      />,
    );
    await user.click(screen.getByTestId("tunnel-speed-test-button"));
    expect(startSpeed).toHaveBeenCalledWith("t1");
  });

  it("renders the running speed-test progress label", () => {
    mockStartSpeed.mockReturnValue({
      start: startSpeed,
      activeTunnelId: "t1",
      percentage: 55,
      isRunning: true,
    } as never);
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel()}
        providers={providers}
        onDelete={vi.fn()}
      />,
    );
    expect(screen.getByText(/Testing 55%/)).toBeInTheDocument();
    expect(screen.getByTestId("tunnel-speed-test-button")).toBeDisabled();
  });

  it("renders the speed-test comparison with high retention", () => {
    mockSpeedResults.mockReturnValue({
      data: { results: [makeSpeedResult()] },
    } as never);
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel()}
        providers={providers}
        onDelete={vi.fn()}
      />,
    );
    const panel = screen.getByTestId("tunnel-speed-test-result");
    expect(within(panel).getByText(/% kept/)).toBeInTheDocument();
    expect(
      screen.getByTestId("tunnel-speed-test-download"),
    ).toBeInTheDocument();
  });

  it("renders the comparison with low retention and negative overheads", () => {
    mockSpeedResults.mockReturnValue({
      data: {
        results: [
          makeSpeedResult({
            tunnel_throughput_mbps: 10,
            tunnel_latency_ms: 5,
            tunnel_jitter_ms: 0,
          }),
        ],
      },
    } as never);
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel()}
        providers={providers}
        onDelete={vi.fn()}
      />,
    );
    expect(screen.getByTestId("tunnel-speed-test-result")).toBeInTheDocument();
  });

  it("rebuilds and deletes the tunnel", async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn();
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel()}
        providers={providers}
        onDelete={onDelete}
      />,
    );
    await user.click(screen.getByRole("button", { name: /Rebuild/ }));
    expect(rebuildMutate).toHaveBeenCalledWith("t1");
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(onDelete).toHaveBeenCalledWith("t1");
  });

  it("shows pending spinners for in-flight test and rebuild", () => {
    mockTest.mockReturnValue({ mutate: testMutate, isPending: true } as never);
    mockRebuild.mockReturnValue({
      mutate: rebuildMutate,
      isPending: true,
      variables: "t1",
    } as never);
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel()}
        providers={providers}
        onDelete={vi.fn()}
      />,
    );
    expect(screen.getByText("Rebuilding")).toBeInTheDocument();
  });

  it("renders a provider icon image when available", () => {
    renderWithProviders(
      <TunnelCard
        tunnel={makeTunnel()}
        providers={[
          {
            id: "nordvpn",
            name: "NordVPN",
            auth_methods: ["credentials"],
            icon_url: "https://example.com/n.png",
          },
        ]}
        onDelete={vi.fn()}
      />,
    );
    const img = document.querySelector("img");
    expect(img).toHaveAttribute("src", "https://example.com/n.png");
  });
});
