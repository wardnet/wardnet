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
// Stand-ins that surface the callbacks as buttons: the page's contract with
// these children is *what it does when they fire*, which a null mock hides.
// biome-ignore lint/security/noSecrets: identifier-shaped string, not a credential — the entropy heuristic misfires on long CamelCase names
vi.mock("@/components/compound/InboundWgPeersTable", () => ({
  InboundWgPeersTable: ({
    peers,
    onToggleEnabled,
    onRevoke,
  }: {
    peers: { id: string; name: string; enabled: boolean }[];
    onToggleEnabled: (p: unknown) => void;
    onRevoke: (p: unknown) => void;
  }) => (
    <div>
      {peers.map((p) => (
        <div key={p.id}>
          <button onClick={() => onToggleEnabled(p)}>toggle:{p.id}</button>
          <button onClick={() => onRevoke(p)}>revoke:{p.id}</button>
        </div>
      ))}
    </div>
  ),
}));
vi.mock("@/components/features/InboundWgPeerGrantedModal", () => ({
  InboundWgPeerGrantedModal: ({
    peer,
    onDismiss,
  }: {
    peer: { id: string } | null;
    onDismiss: () => void;
  }) =>
    peer ? (
      <div>
        <span>granted-modal:{peer.id}</span>
        <button onClick={onDismiss}>dismiss-granted</button>
      </div>
    ) : null,
}));
// Reports which devices survived the grantable filter, and lets a test pick
// one — the page's real output here is *which* devices it offers.
vi.mock("@/components/compound/DeviceSelect", () => ({
  DeviceSelect: ({
    devices,
    onChange,
  }: {
    devices: { id: string }[];
    onChange: (v: string) => void;
  }) => (
    <div data-testid="device-select">
      grantable:{devices.map((d) => d.id).join(",") || "none"}
      {devices.map((d) => (
        <button key={d.id} onClick={() => onChange(d.id)}>
          pick:{d.id}
        </button>
      ))}
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

  it("grants access to the picked device and shows the credential once", async () => {
    const user = userEvent.setup();
    const mutateAsync = vi.fn().mockResolvedValue({ id: "new-peer" });
    useAddInboundWgPeer.mockReturnValue({ mutateAsync, isPending: false });
    useDevices.mockReturnValue({
      data: { devices: [makeDevice({ id: "d1", name: "Laptop" })] },
    });
    renderWithProviders(<Vpn />);
    await user.click(screen.getByRole("button", { name: "Grant access" }));
    await user.click(screen.getByRole("button", { name: "pick:d1" }));
    await user.click(screen.getByRole("button", { name: "Grant" }));

    expect(mutateAsync).toHaveBeenCalledWith({ device_id: "d1" });
    // The credential is shown exactly once, on grant — it cannot be re-read.
    expect(
      await screen.findByText("granted-modal:new-peer"),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "dismiss-granted" }));
    expect(
      screen.queryByText("granted-modal:new-peer"),
    ).not.toBeInTheDocument();
  });

  // A failed grant must keep the panel and the selection so the admin can
  // retry, and must not surface as an unhandled rejection.
  it("keeps the grant panel open when the mutation rejects", async () => {
    const user = userEvent.setup();
    const mutateAsync = vi.fn().mockRejectedValue(new Error("409"));
    useAddInboundWgPeer.mockReturnValue({ mutateAsync, isPending: false });
    useDevices.mockReturnValue({
      data: { devices: [makeDevice({ id: "d1", name: "Laptop" })] },
    });
    renderWithProviders(<Vpn />);
    await user.click(screen.getByRole("button", { name: "Grant access" }));
    await user.click(screen.getByRole("button", { name: "pick:d1" }));
    await user.click(screen.getByRole("button", { name: "Grant" }));

    expect(mutateAsync).toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Grant" })).toBeInTheDocument();
    expect(screen.queryByText(/granted-modal/)).not.toBeInTheDocument();
  });

  it("abandons the grant on cancel", async () => {
    const user = userEvent.setup();
    useDevices.mockReturnValue({
      data: { devices: [makeDevice({ id: "d1", name: "Laptop" })] },
    });
    renderWithProviders(<Vpn />);
    await user.click(screen.getByRole("button", { name: "Grant access" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(
      screen.queryByRole("button", { name: "Grant" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Grant access" }),
    ).toBeInTheDocument();
  });

  it("toggles the server and saves an edited listen port", async () => {
    const user = userEvent.setup();
    const mutate = vi.fn();
    useSetInboundWgConfig.mockReturnValue({ mutate, isPending: false });
    renderWithProviders(<Vpn />);

    await user.click(screen.getByRole("switch"));
    expect(mutate).toHaveBeenCalledWith({ enabled: false, listen_port: 51821 });

    // By label, which only resolves because the caption is a real <label>
    // bound to the input — the query is the accessibility assertion.
    const port = screen.getByLabelText("Listen port");
    await user.clear(port);
    await user.type(port, "51999");
    await user.click(screen.getByRole("button", { name: "Save port" }));
    expect(mutate).toHaveBeenCalledWith({ enabled: true, listen_port: 51999 });
  });

  it("pauses a peer by inverting its enabled flag", async () => {
    const user = userEvent.setup();
    const mutate = vi.fn();
    useSetInboundWgPeerEnabled.mockReturnValue({ mutate, isPending: false });
    useInboundWgPeers.mockReturnValue({
      data: { peers: [makePeer({ id: "p1", enabled: true })] },
      isLoading: false,
    });
    renderWithProviders(<Vpn />);
    await user.click(screen.getByRole("button", { name: "toggle:p1" }));
    expect(mutate).toHaveBeenCalledWith({ id: "p1", enabled: false });
  });

  it("revokes a peer only after the confirm dialog", async () => {
    const user = userEvent.setup();
    const mutate = vi.fn();
    useRemoveInboundWgPeer.mockReturnValue({ mutate, isPending: false });
    useInboundWgPeers.mockReturnValue({
      data: { peers: [makePeer({ id: "p1", name: "Laptop" })] },
      isLoading: false,
    });
    renderWithProviders(<Vpn />);
    await user.click(screen.getByRole("button", { name: "revoke:p1" }));
    expect(await screen.findByText("Revoke remote access")).toBeInTheDocument();
    expect(mutate).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Revoke" }));
    expect(mutate).toHaveBeenCalledWith("p1");
  });

  it("does not revoke when the confirm dialog is dismissed", async () => {
    const user = userEvent.setup();
    const mutate = vi.fn();
    useRemoveInboundWgPeer.mockReturnValue({ mutate, isPending: false });
    useInboundWgPeers.mockReturnValue({
      data: { peers: [makePeer({ id: "p1" })] },
      isLoading: false,
    });
    renderWithProviders(<Vpn />);
    await user.click(screen.getByRole("button", { name: "revoke:p1" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(mutate).not.toHaveBeenCalled();
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
