/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { DeviceOwnerCard } from "@/components/features/DeviceOwnerCard";
import { ownerValueToId } from "@/lib/deviceOwner";
import { renderWithProviders } from "../../test-utils";

const device = { id: "d1", owner_user_id: null } as any;
const users = [
  { id: "u-ana", display_name: "Ana" },
  { id: "u-bruno", display_name: "Bruno" },
] as any;

function handle() {
  return {
    mutateAsync: vi.fn().mockResolvedValue(undefined),
    reset: vi.fn(),
    isPending: false,
    isError: false,
    error: null,
  };
}

describe("DeviceOwnerCard", () => {
  it("says in words that ownership grants nothing", () => {
    // Device identity is source-IP-derived, so "Owner: Ana (admin)" is a very
    // natural thing to misread as a permission. The card must say otherwise.
    renderWithProviders(
      <DeviceOwnerCard device={device} users={users} setOwner={handle()} />,
    );
    expect(screen.getByText(/grants no access/i)).toBeInTheDocument();
  });

  it("shows the current owner, and 'Nobody' when there is none", () => {
    const { rerender } = renderWithProviders(
      <DeviceOwnerCard device={device} users={users} setOwner={handle()} />,
    );
    expect(screen.getByTestId("device-owner")).toHaveTextContent("Nobody");

    rerender(
      <DeviceOwnerCard
        device={{ ...device, owner_user_id: "u-ana" }}
        users={users}
        setOwner={handle()}
      />,
    );
    expect(screen.getByTestId("device-owner")).toHaveTextContent("Ana");
  });

  it("maps the 'nobody' sentinel back to null, and a real id through", () => {
    // The dropdown is a Radix `Select`, which renders no native element and so
    // cannot be driven in jsdom — hence testing the mapping directly. Sending
    // the sentinel as an owner id would fail the foreign key.
    expect(ownerValueToId("__unassigned__")).toBeNull();
    expect(ownerValueToId("u-bruno")).toBe("u-bruno");
  });

  it("disables the control while a change is in flight", () => {
    renderWithProviders(
      <DeviceOwnerCard
        device={device}
        users={users}
        setOwner={{ ...handle(), isPending: true }}
      />,
    );
    expect(screen.getByTestId("device-owner")).toBeDisabled();
  });

  it("points at the directory when there is nobody to assign", () => {
    renderWithProviders(
      <DeviceOwnerCard device={device} users={[]} setOwner={handle()} />,
    );
    expect(screen.getByText(/No users yet/)).toBeInTheDocument();
  });
});
