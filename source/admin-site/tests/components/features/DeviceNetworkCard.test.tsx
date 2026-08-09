import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceNetworkCard } from "@/components/features/DeviceNetworkCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { Device, DhcpReservation } from "@wardnet/js";

const createAsync = vi.fn();
const deleteAsync = vi.fn();
const createReset = vi.fn();
const deleteReset = vi.fn();

function makeReservation(over: Partial<DhcpReservation> = {}): DhcpReservation {
  return {
    id: "res-1",
    mac_address: "AA:BB:CC:DD:EE:FF",
    ip_address: "10.232.1.10",
    hostname: null,
    description: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...over,
  };
}

function cardProps({
  device = makeDevice(),
  reservations = [] as DhcpReservation[],
  create = {},
  del = {},
}: {
  device?: Device;
  reservations?: DhcpReservation[];
  create?: Partial<{
    isPending: boolean;
    isError: boolean;
    error: Error | null;
  }>;
  del?: Partial<{ isPending: boolean; isError: boolean; error: Error | null }>;
} = {}) {
  // Both create instances share the same spy, mirroring the sequence the card
  // drives: first the edit's create, then (on failure) the silent restore.
  const createHandle = {
    mutateAsync: createAsync,
    reset: createReset,
    isPending: false,
    isError: false,
    error: null,
    ...create,
  };
  return {
    device,
    reservations,
    createReservation: createHandle,
    restoreReservation: createHandle,
    deleteReservation: {
      mutateAsync: deleteAsync,
      reset: deleteReset,
      isPending: false,
      isError: false,
      error: null,
      ...del,
    },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  createAsync.mockResolvedValue(undefined);
  deleteAsync.mockResolvedValue(undefined);
});

async function setLastOctet(
  user: ReturnType<typeof userEvent.setup>,
  v: string,
) {
  const inputs = screen.getAllByRole("textbox");
  const last = inputs[inputs.length - 1];
  await user.click(last);
  await user.keyboard(`{Control>}a{/Control}{Backspace}${v}`);
}

describe("DeviceNetworkCard", () => {
  it("renders the read-only IP and lease status badge", () => {
    renderWithProviders(
      <DeviceNetworkCard
        {...cardProps({ device: makeDevice({ dhcp_status: "lease" }) })}
      />,
    );
    expect(screen.getByText("10.232.1.10")).toBeInTheDocument();
    expect(screen.getByText("Lease")).toBeInTheDocument();
  });

  it("shows the reservation badge", () => {
    renderWithProviders(
      <DeviceNetworkCard
        {...cardProps({ device: makeDevice({ dhcp_status: "reservation" }) })}
      />,
    );
    expect(screen.getByText("Reserved")).toBeInTheDocument();
  });

  it("shows the external badge", () => {
    renderWithProviders(
      <DeviceNetworkCard
        {...cardProps({ device: makeDevice({ dhcp_status: "external" }) })}
      />,
    );
    expect(screen.getByText("External")).toBeInTheDocument();
  });

  it("creates a reservation on save when none exists", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceNetworkCard
        {...cardProps({
          device: makeDevice({ name: "TV", hostname: "tv-host" }),
        })}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(createAsync).toHaveBeenCalled());
    expect(createAsync).toHaveBeenCalledWith({
      mac_address: "AA:BB:CC:DD:EE:FF",
      ip_address: "10.232.1.10",
      hostname: "tv-host",
      description: "TV",
    });
    expect(deleteAsync).not.toHaveBeenCalled();
  });

  it("closes without mutating when IP is unchanged for an existing reservation", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceNetworkCard
        {...cardProps({
          reservations: [makeReservation({ ip_address: "10.232.1.10" })],
        })}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(deleteAsync).not.toHaveBeenCalled();
    expect(createAsync).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument(),
    );
  });

  it("deletes then creates when the IP changes for an existing reservation", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceNetworkCard
        {...cardProps({
          reservations: [makeReservation({ ip_address: "10.232.1.10" })],
        })}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Edit" }));
    await setLastOctet(user, "20");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(deleteAsync).toHaveBeenCalledWith("res-1"));
    expect(createAsync).toHaveBeenCalledWith(
      expect.objectContaining({ ip_address: "10.232.1.20" }),
    );
  });

  it("restores the original reservation when the replacement create fails", async () => {
    const user = userEvent.setup();
    // First create (the new IP) is rejected; the rollback recreate succeeds.
    createAsync
      .mockRejectedValueOnce(new Error("409"))
      .mockResolvedValueOnce(undefined);
    renderWithProviders(
      <DeviceNetworkCard
        {...cardProps({
          reservations: [makeReservation({ ip_address: "10.232.1.10" })],
        })}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Edit" }));
    await setLastOctet(user, "20");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(deleteAsync).toHaveBeenCalledWith("res-1"));
    // Attempted the new IP, then rolled back to the original one.
    expect(createAsync).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({ ip_address: "10.232.1.20" }),
    );
    expect(createAsync).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({
        mac_address: "AA:BB:CC:DD:EE:FF",
        ip_address: "10.232.1.10",
      }),
    );
    expect(deleteAsync).toHaveBeenCalledTimes(1);
    // Stays in edit mode with the failure surfaced to the admin.
    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();
  });

  it("warns that the reservation is lost when the rollback recreate also fails", async () => {
    const user = userEvent.setup();
    // Both the new-IP create and the rollback recreate fail.
    createAsync.mockRejectedValue(new Error("409"));
    renderWithProviders(
      <DeviceNetworkCard
        {...cardProps({
          reservations: [makeReservation({ ip_address: "10.232.1.10" })],
        })}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Edit" }));
    await setLastOctet(user, "20");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(deleteAsync).toHaveBeenCalledWith("res-1"));
    expect(createAsync).toHaveBeenCalledTimes(2);
    // The admin is told the reservation could not be restored, not the
    // now-misleading new-IP failure.
    expect(
      await screen.findByText(/Could not restore the previous reservation/i),
    ).toBeInTheDocument();
  });

  it("removes an existing reservation", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceNetworkCard
        {...cardProps({ reservations: [makeReservation()] })}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(
      screen.getByRole("button", { name: "Remove reservation" }),
    );

    await waitFor(() => expect(deleteAsync).toHaveBeenCalledWith("res-1"));
  });

  it("cancels edit mode", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DeviceNetworkCard {...cardProps()} />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument();
    expect(createReset).toHaveBeenCalled();
  });

  it("shows a busy label and error alert", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceNetworkCard
        {...cardProps({
          create: { isPending: true, isError: true, error: new Error("x") },
        })}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Edit" }));
    expect(screen.getByRole("button", { name: "Saving…" })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });
});
