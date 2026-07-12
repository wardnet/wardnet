import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  mutate,
  useUpdateDevice,
  assignZone,
  useNetworkZones,
  useAssignDeviceZone,
  useInboundWgPeers,
  addPeerMutateAsync,
  useAddInboundWgPeer,
} = vi.hoisted(() => {
  const mutate = vi.fn();
  const assignZone = vi.fn();
  const addPeerMutateAsync = vi.fn();
  return {
    mutate,
    useUpdateDevice: vi.fn(() => ({ mutate, isPending: false })),
    assignZone,
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
    // The sheet reads zones + an assign mutation for its zone-reassignment
    // section; keep the list empty so these specs stay focused on routing.
    // `zones` is typed loosely — the hook is mocked, so the component's real
    // NetworkZoneView type is irrelevant here; only the fields the sheet
    // reads (id/name/is_default) matter.
    useNetworkZones: vi.fn(
      (): {
        data: {
          zones: Array<{ id: string; name: string; is_default: boolean }>;
        };
      } => ({
        data: { zones: [] },
      }),
    ),
    useAssignDeviceZone: vi.fn(() => ({
      mutate: assignZone,
      isPending: false,
    })),
  };
});
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useUpdateDevice,
    useNetworkZones,
    useAssignDeviceZone,
    useInboundWgPeers,
    useAddInboundWgPeer,
  };
});

import { DeviceRoutingSheet } from "@/components/DeviceRoutingSheet";
import { makeDevice, makeTunnel } from "../test-utils";

describe("DeviceRoutingSheet", () => {
  beforeEach(() => {
    mutate.mockReset();
    assignZone.mockReset();
    useUpdateDevice.mockReturnValue({ mutate, isPending: false });
    useNetworkZones.mockReturnValue({ data: { zones: [] } });
    useAssignDeviceZone.mockReturnValue({
      mutate: assignZone,
      isPending: false,
    });
    addPeerMutateAsync.mockReset();
    useInboundWgPeers.mockReturnValue({ data: { peers: [] } });
    useAddInboundWgPeer.mockReturnValue({
      mutateAsync: addPeerMutateAsync,
      isPending: false,
    });
  });

  it("renders default/direct options plus each tunnel", () => {
    render(
      <DeviceRoutingSheet
        device={makeDevice({ name: "Laptop" })}
        tunnels={[makeTunnel({ id: "t1", label: "US East", status: "down" })]}
        open
        onOpenChange={vi.fn()}
      />,
    );
    expect(screen.getByTestId("device-routing-default")).toBeInTheDocument();
    expect(screen.getByTestId("device-routing-direct")).toBeInTheDocument();
    expect(screen.getByText(/US East/)).toBeInTheDocument();
    // "down" tunnels surface a status sublabel.
    expect(screen.getByText("Down")).toBeInTheDocument();
    expect(screen.getByText(/Route: Laptop/)).toBeInTheDocument();
  });

  it("mutates with a direct target and closes on success", async () => {
    const onOpenChange = vi.fn();
    mutate.mockImplementation((_vars, opts) => opts?.onSuccess?.());
    render(
      <DeviceRoutingSheet
        device={makeDevice({ id: "dev-9" })}
        tunnels={[]}
        open
        onOpenChange={onOpenChange}
      />,
    );
    await userEvent.click(screen.getByTestId("device-routing-direct"));
    expect(mutate).toHaveBeenCalledWith(
      { id: "dev-9", body: { routing_target: { type: "direct" } } },
      expect.any(Object),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("mutates with a tunnel target when a tunnel row is chosen", async () => {
    render(
      <DeviceRoutingSheet
        device={makeDevice({ id: "dev-2" })}
        tunnels={[makeTunnel({ id: "t7", label: "JP", status: "up" })]}
        open
        onOpenChange={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByText(/JP/));
    expect(mutate).toHaveBeenCalledWith(
      {
        id: "dev-2",
        body: { routing_target: { type: "tunnel", tunnel_id: "t7" } },
      },
      expect.any(Object),
    );
  });

  it("falls back to the latched device label after device is cleared", () => {
    const { rerender } = render(
      <DeviceRoutingSheet
        device={makeDevice({ name: "Phone" })}
        tunnels={[]}
        open
        onOpenChange={vi.fn()}
      />,
    );
    expect(screen.getByText(/Route: Phone/)).toBeInTheDocument();
    // Parent clears the selection while animating closed — label stays latched.
    rerender(
      <DeviceRoutingSheet
        device={null}
        tunnels={[]}
        open
        onOpenChange={vi.fn()}
      />,
    );
    expect(screen.getByText(/Route: Phone/)).toBeInTheDocument();
  });

  it("reassigns the device's zone when a zone row is chosen", async () => {
    const onOpenChange = vi.fn();
    assignZone.mockImplementation((_vars, opts) => opts?.onSuccess?.());
    useNetworkZones.mockReturnValue({
      data: {
        zones: [
          { id: "zone-1", name: "Trusted", is_default: true },
          { id: "zone-2", name: "Guest", is_default: false },
        ],
      },
    });
    render(
      <DeviceRoutingSheet
        device={makeDevice({ id: "dev-5", zone_id: "zone-1" })}
        tunnels={[]}
        open
        onOpenChange={onOpenChange}
      />,
    );
    await userEvent.click(screen.getByText("Guest"));
    expect(assignZone).toHaveBeenCalledWith(
      { deviceId: "dev-5", zoneId: "zone-2" },
      expect.any(Object),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("does not reassign when the current zone is re-selected", async () => {
    useNetworkZones.mockReturnValue({
      data: { zones: [{ id: "zone-1", name: "Trusted", is_default: true }] },
    });
    render(
      <DeviceRoutingSheet
        device={makeDevice({ id: "dev-6", zone_id: "zone-1" })}
        tunnels={[]}
        open
        onOpenChange={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByText("Trusted"));
    expect(assignZone).not.toHaveBeenCalled();
  });

  it("grants remote access for the tapped device and shows the config", async () => {
    addPeerMutateAsync.mockResolvedValue({
      id: "peer-1",
      name: "Laptop",
      public_key: "pub",
      allowed_ip: "10.100.64.2/32",
      client_config: "[Interface]\nPrivateKey = k\n",
    });
    render(
      <DeviceRoutingSheet
        device={makeDevice({ id: "dev-7", name: "Laptop" })}
        tunnels={[]}
        open
        onOpenChange={vi.fn()}
      />,
    );
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
    render(
      <DeviceRoutingSheet
        device={makeDevice({ id: "dev-8", name: "Laptop" })}
        tunnels={[]}
        open
        onOpenChange={vi.fn()}
      />,
    );
    expect(
      screen.queryByTestId("device-grant-remote-access"),
    ).not.toBeInTheDocument();
  });

  it("hides the grant action for an unmanaged (unnamed) device", () => {
    render(
      <DeviceRoutingSheet
        device={makeDevice({ id: "dev-9", name: null })}
        tunnels={[]}
        open
        onOpenChange={vi.fn()}
      />,
    );
    expect(
      screen.queryByTestId("device-grant-remote-access"),
    ).not.toBeInTheDocument();
  });
});
