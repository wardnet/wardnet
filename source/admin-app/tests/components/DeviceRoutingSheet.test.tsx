import type { ComponentProps } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Device, NetworkZoneView, Tunnel } from "@wardnet/js";

const { useInboundWgPeers, addPeerMutateAsync, useAddInboundWgPeer } =
  vi.hoisted(() => {
    const addPeerMutateAsync = vi.fn();
    return {
      // The sheet reads the peer list (to decide if a device is already granted)
      // and an add-peer mutation for the grant action. Default to no peers so
      // every managed device is grantable; specs override as needed. The return
      // type is annotated so the empty default isn't inferred as `never[]`, which
      // would reject the populated-peer override in the "already granted" spec.
      useInboundWgPeers: vi.fn(
        (): {
          data: {
            peers: Array<{
              id: string;
              name: string;
              public_key: string;
              allowed_ip: string;
              enabled: boolean;
              created_at: string;
              device_id: string | null;
            }>;
          };
        } => ({ data: { peers: [] } }),
      ),
      addPeerMutateAsync,
      useAddInboundWgPeer: vi.fn(() => ({
        mutateAsync: addPeerMutateAsync,
        isPending: false,
      })),
    };
  });
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useInboundWgPeers,
    useAddInboundWgPeer,
  };
});

import { DeviceRoutingSheet } from "@/components/DeviceRoutingSheet";
import { makeDevice, makeTunnel } from "../test-utils";

// A full zone fixture; the sheet only reads id/name/is_default, but the prop is
// typed so the rest is filled with harmless defaults.
function zone(id: string, name: string, isDefault = false): NetworkZoneView {
  return {
    id,
    name,
    provenance: "manual",
    isolation_stance: "shared_subnet",
    allowed_targets: ["direct", "tunnel"],
    member_isolation: false,
    subnet: null,
    admin_ui_reachable: true,
    is_default: isDefault,
    is_default_for_new: false,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    member_count: 0,
  };
}

function renderSheet(
  props: Partial<ComponentProps<typeof DeviceRoutingSheet>> = {},
) {
  const defaults = {
    device: makeDevice() as Device | null,
    tunnels: [] as Tunnel[],
    zones: [] as NetworkZoneView[],
    busy: false,
    open: true,
    onOpenChange: vi.fn(),
    onSelectRoute: vi.fn(),
    onSelectZone: vi.fn(),
  };
  const merged = { ...defaults, ...props };
  return { ...render(<DeviceRoutingSheet {...merged} />), props: merged };
}

describe("DeviceRoutingSheet", () => {
  beforeEach(() => {
    addPeerMutateAsync.mockReset();
    useInboundWgPeers.mockReturnValue({ data: { peers: [] } });
    useAddInboundWgPeer.mockReturnValue({
      mutateAsync: addPeerMutateAsync,
      isPending: false,
    });
  });

  it("renders default/direct options plus each tunnel", () => {
    renderSheet({
      device: makeDevice({ name: "Laptop" }),
      tunnels: [makeTunnel({ id: "t1", label: "US East", status: "down" })],
    });
    expect(screen.getByTestId("device-routing-default")).toBeInTheDocument();
    expect(screen.getByTestId("device-routing-direct")).toBeInTheDocument();
    expect(screen.getByText(/US East/)).toBeInTheDocument();
    // "down" tunnels surface a status sublabel.
    expect(screen.getByText("Down")).toBeInTheDocument();
    expect(screen.getByText(/Route: Laptop/)).toBeInTheDocument();
  });

  it("fires onSelectRoute with a direct target", async () => {
    const onSelectRoute = vi.fn();
    renderSheet({ device: makeDevice({ id: "dev-9" }), onSelectRoute });
    await userEvent.click(screen.getByTestId("device-routing-direct"));
    expect(onSelectRoute).toHaveBeenCalledWith({ type: "direct" });
  });

  it("fires onSelectRoute with a tunnel target when a tunnel row is chosen", async () => {
    const onSelectRoute = vi.fn();
    renderSheet({
      device: makeDevice({ id: "dev-2" }),
      tunnels: [makeTunnel({ id: "t7", label: "JP", status: "up" })],
      onSelectRoute,
    });
    await userEvent.click(screen.getByText(/JP/));
    expect(onSelectRoute).toHaveBeenCalledWith({
      type: "tunnel",
      tunnel_id: "t7",
    });
  });

  it("disables the routing rows while a change is in flight", async () => {
    const onSelectRoute = vi.fn();
    renderSheet({
      device: makeDevice({ id: "dev-1" }),
      busy: true,
      onSelectRoute,
    });
    await userEvent.click(screen.getByTestId("device-routing-direct"));
    expect(onSelectRoute).not.toHaveBeenCalled();
  });

  it("falls back to the latched device label after device is cleared", () => {
    const { rerender } = renderSheet({ device: makeDevice({ name: "Phone" }) });
    expect(screen.getByText(/Route: Phone/)).toBeInTheDocument();
    // Parent clears the selection while animating closed — label stays latched.
    rerender(
      <DeviceRoutingSheet
        device={null}
        tunnels={[]}
        zones={[]}
        busy={false}
        open
        onOpenChange={vi.fn()}
        onSelectRoute={vi.fn()}
        onSelectZone={vi.fn()}
      />,
    );
    expect(screen.getByText(/Route: Phone/)).toBeInTheDocument();
  });

  it("fires onSelectZone when a different zone row is chosen", async () => {
    const onSelectZone = vi.fn();
    renderSheet({
      device: makeDevice({ id: "dev-5", zone_id: "zone-1" }),
      zones: [zone("zone-1", "Trusted", true), zone("zone-2", "Guest")],
      onSelectZone,
    });
    await userEvent.click(screen.getByText("Guest"));
    expect(onSelectZone).toHaveBeenCalledWith("zone-2");
  });

  it("does not fire onSelectZone when the current zone is re-selected", async () => {
    const onSelectZone = vi.fn();
    renderSheet({
      device: makeDevice({ id: "dev-6", zone_id: "zone-1" }),
      zones: [zone("zone-1", "Trusted", true)],
      onSelectZone,
    });
    await userEvent.click(screen.getByText("Trusted"));
    expect(onSelectZone).not.toHaveBeenCalled();
  });

  it("grants remote access for the tapped device and shows the config", async () => {
    addPeerMutateAsync.mockResolvedValue({
      id: "peer-1",
      name: "Laptop",
      public_key: "pub",
      allowed_ip: "10.100.64.2/32",
      // biome-ignore lint/security/noSecrets: throwaway WireGuard keypair generated for this fixture, never a live credential
      client_config: "[Interface]\nPrivateKey = k\n",
    });
    renderSheet({ device: makeDevice({ id: "dev-7", name: "Laptop" }) });
    await userEvent.click(screen.getByTestId("device-grant-remote-access"));
    expect(addPeerMutateAsync).toHaveBeenCalledWith({ device_id: "dev-7" });
    // On success the sheet swaps to the one-time QR / config view.
    expect(
      await screen.findByText(/Remote access granted - Laptop/),
    ).toBeInTheDocument();
  });

  it("hides the grant action for a device that already has a peer", () => {
    useInboundWgPeers.mockReturnValue({
      data: {
        peers: [
          {
            id: "peer-9",
            name: "Laptop",
            public_key: "pub",
            allowed_ip: "10.100.64.2/32",
            enabled: true,
            created_at: "2026-01-01T00:00:00Z",
            device_id: "dev-8",
          },
        ],
      },
    });
    renderSheet({ device: makeDevice({ id: "dev-8", name: "Laptop" }) });
    expect(
      screen.queryByTestId("device-grant-remote-access"),
    ).not.toBeInTheDocument();
  });

  it("hides the grant action for an unmanaged (unnamed) device", () => {
    renderSheet({ device: makeDevice({ id: "dev-9", name: null }) });
    expect(
      screen.queryByTestId("device-grant-remote-access"),
    ).not.toBeInTheDocument();
  });
});
