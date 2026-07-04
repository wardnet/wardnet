import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  mutate,
  useUpdateDevice,
  assignZone,
  useNetworkZones,
  useAssignDeviceZone,
} = vi.hoisted(() => {
  const mutate = vi.fn();
  const assignZone = vi.fn();
  return {
    mutate,
    useUpdateDevice: vi.fn(() => ({ mutate, isPending: false })),
    assignZone,
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
  return { ...actual, useUpdateDevice, useNetworkZones, useAssignDeviceZone };
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
});
