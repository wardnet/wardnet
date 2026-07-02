import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { mutate, useUpdateDevice } = vi.hoisted(() => {
  const mutate = vi.fn();
  return {
    mutate,
    useUpdateDevice: vi.fn(() => ({ mutate, isPending: false })),
  };
});
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useUpdateDevice };
});

import { DeviceRoutingSheet } from "@/components/DeviceRoutingSheet";
import { makeDevice, makeTunnel } from "../test-utils";

describe("DeviceRoutingSheet", () => {
  beforeEach(() => {
    mutate.mockReset();
    useUpdateDevice.mockReturnValue({ mutate, isPending: false });
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
      { id: "dev-2", body: { routing_target: { type: "tunnel", tunnel_id: "t7" } } },
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
      <DeviceRoutingSheet device={null} tunnels={[]} open onOpenChange={vi.fn()} />,
    );
    expect(screen.getByText(/Route: Phone/)).toBeInTheDocument();
  });
});
