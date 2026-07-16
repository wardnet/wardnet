import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { InboundWgPeerSummary } from "@wardnet/js";

const {
  useInboundWgConfig,
  useSetInboundWgConfig,
  useInboundWgPeers,
  useAddInboundWgPeer,
  useRemoveInboundWgPeer,
  useSetInboundWgPeerEnabled,
  useDevices,
} = vi.hoisted(() => ({
  useInboundWgConfig: vi.fn(),
  useSetInboundWgConfig: vi.fn(),
  useInboundWgPeers: vi.fn(),
  useAddInboundWgPeer: vi.fn(),
  useRemoveInboundWgPeer: vi.fn(),
  useSetInboundWgPeerEnabled: vi.fn(),
  useDevices: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useInboundWgConfig,
    useSetInboundWgConfig,
    useInboundWgPeers,
    useAddInboundWgPeer,
    useRemoveInboundWgPeer,
    useSetInboundWgPeerEnabled,
    useDevices,
    InboundWgBetaNotice: () => null,
  };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: { title: ReactNode }) => <h1>{title}</h1>,
}));
vi.mock("@/components/compound/InboundWgPeersTable", () => ({
  InboundWgPeersTable: () => null,
}));
// Stand-in that reports how many devices survived the grantable filter — the
// page's real output here is *which* devices it offers, not how they render.
vi.mock("@/components/compound/DeviceSelect", () => ({
  DeviceSelect: ({ devices }: { devices: { id: string }[] }) => (
    <div data-testid="device-select">
      grantable:{devices.map((d) => d.id).join(",") || "none"}
    </div>
  ),
}));

import Vpn from "@/pages/Vpn";
import { makeDevice, renderWithProviders } from "../test-utils";

function makePeer(over: Partial<InboundWgPeerSummary> = {}) {
  return {
    id: "p1",
    name: "Laptop",
    public_key: "key",
    allowed_ip: "10.90.0.2/32",
    enabled: true,
    created_at: "2026-01-01T00:00:00Z",
    device_id: "d1",
    ...over,
  } as InboundWgPeerSummary;
}

beforeEach(() => {
  vi.clearAllMocks();
  useInboundWgConfig.mockReturnValue({
    data: { enabled: true, listen_port: 51821 },
    isLoading: false,
  });
  useSetInboundWgConfig.mockReturnValue({ mutate: vi.fn(), isPending: false });
  useInboundWgPeers.mockReturnValue({ data: { peers: [] }, isLoading: false });
  useAddInboundWgPeer.mockReturnValue({ mutate: vi.fn(), isPending: false });
  useRemoveInboundWgPeer.mockReturnValue({ mutate: vi.fn(), isPending: false });
  useSetInboundWgPeerEnabled.mockReturnValue({
    mutate: vi.fn(),
    isPending: false,
  });
  useDevices.mockReturnValue({ data: { devices: [] } });
});

describe("Vpn", () => {
  it("renders the page heading", () => {
    renderWithProviders(<Vpn />);
    expect(screen.getByRole("heading", { name: /VPN/i })).toBeInTheDocument();
  });

  // Unmanaged (unnamed) devices are rejected by the backend, so they must
  // never reach the picker.
  it("offers only managed devices for a grant", async () => {
    const user = userEvent.setup();
    useDevices.mockReturnValue({
      data: {
        devices: [
          makeDevice({ id: "d1", name: "Laptop" }),
          makeDevice({ id: "d2", name: null, hostname: "discovered-only" }),
        ],
      },
    });
    renderWithProviders(<Vpn />);
    await user.click(screen.getByRole("button", { name: "Grant access" }));
    expect(screen.getByTestId("device-select")).toHaveTextContent(
      "grantable:d1",
    );
  });

  // A device that already holds a peer row would 409 on re-grant, so it drops
  // out of the picker even though it is still managed.
  it("excludes devices that already have a peer", async () => {
    const user = userEvent.setup();
    useDevices.mockReturnValue({
      data: {
        devices: [
          makeDevice({ id: "d1", name: "Laptop" }),
          makeDevice({ id: "d2", name: "Phone" }),
        ],
      },
    });
    useInboundWgPeers.mockReturnValue({
      data: { peers: [makePeer({ device_id: "d1" })] },
      isLoading: false,
    });
    renderWithProviders(<Vpn />);
    await user.click(screen.getByRole("button", { name: "Grant access" }));
    expect(screen.getByTestId("device-select")).toHaveTextContent(
      "grantable:d2",
    );
  });

  // A legacy peer row with no device link must not knock an unrelated device
  // out of the picker.
  it("ignores peers with no device link when computing grantable", async () => {
    const user = userEvent.setup();
    useDevices.mockReturnValue({
      data: { devices: [makeDevice({ id: "d1", name: "Laptop" })] },
    });
    useInboundWgPeers.mockReturnValue({
      data: { peers: [makePeer({ device_id: null })] },
      isLoading: false,
    });
    renderWithProviders(<Vpn />);
    await user.click(screen.getByRole("button", { name: "Grant access" }));
    expect(screen.getByTestId("device-select")).toHaveTextContent(
      "grantable:d1",
    );
  });

  it("explains that a device must be named before it can be granted", () => {
    useDevices.mockReturnValue({
      data: { devices: [makeDevice({ id: "d2", name: null })] },
    });
    renderWithProviders(<Vpn />);
    expect(
      screen.getByText(/Only managed devices can be granted remote access/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Grant access" }),
    ).not.toBeInTheDocument();
  });

  it("reports when every managed device already has access", () => {
    useDevices.mockReturnValue({
      data: { devices: [makeDevice({ id: "d1", name: "Laptop" })] },
    });
    useInboundWgPeers.mockReturnValue({
      data: { peers: [makePeer({ device_id: "d1" })] },
      isLoading: false,
    });
    renderWithProviders(<Vpn />);
    expect(
      screen.getByText(/Every managed device already has remote access/),
    ).toBeInTheDocument();
  });

  it("hides the peers card while the server is disabled", () => {
    useInboundWgConfig.mockReturnValue({
      data: { enabled: false, listen_port: 51821 },
      isLoading: false,
    });
    useDevices.mockReturnValue({
      data: { devices: [makeDevice({ id: "d1", name: "Laptop" })] },
    });
    renderWithProviders(<Vpn />);
    expect(screen.queryByText("Peers")).not.toBeInTheDocument();
  });
});
