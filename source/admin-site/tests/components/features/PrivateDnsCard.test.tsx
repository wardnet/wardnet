import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PrivateDnsCard } from "@/components/features/PrivateDnsCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { PrivateDnsStatusResponse } from "@wardnet/js";

// The grant panel's DeviceSelect is a Radix Select, which measures its trigger
// in a layout effect; jsdom has no ResizeObserver / pointer-capture, so stub
// them (as elsewhere in the suite).
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.releasePointerCapture ??= () => {};
Element.prototype.scrollIntoView ??= () => {};
vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
);

const onSetEnabled = vi.fn();
const grantMutateAsync = vi.fn();
const onRevokeDevice = vi.fn();
const onSendToDevice = vi.fn();

type Prereqs = {
  wardnet_provider: boolean;
  certificate: boolean;
  entitled: boolean;
  relay_connected: boolean;
};
type Grant = {
  id: string;
  device_id: string;
  hostname: string;
  created_at: string;
};

function renderCard({
  enabled = false,
  prerequisites,
  grants = [] as Grant[],
  devices = [] as ReturnType<typeof makeDevice>[],
}: {
  enabled?: boolean;
  prerequisites: Prereqs;
  grants?: Grant[];
  devices?: ReturnType<typeof makeDevice>[];
}) {
  const status = {
    enabled,
    domain: "abc.my.wardnet.services",
    prerequisites,
    grants,
  } as unknown as PrivateDnsStatusResponse;
  return renderWithProviders(
    <PrivateDnsCard
      status={status}
      isLoading={false}
      devices={devices}
      onSetEnabled={onSetEnabled}
      setEnabledPending={false}
      grantDevice={{ mutateAsync: grantMutateAsync, isPending: false }}
      onRevokeDevice={onRevokeDevice}
      onSendToDevice={onSendToDevice}
      sendPending={false}
    />,
  );
}

const ALL_MET: Prereqs = {
  wardnet_provider: true,
  certificate: true,
  entitled: true,
  relay_connected: true,
};
const GRANT: Grant = {
  id: "g1",
  device_id: "d1",
  hostname: "tok.abc.my.wardnet.services",
  created_at: "2026-07-21T00:00:00Z",
};

describe("PrivateDnsCard", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders the prerequisite checklist and disables the toggle when a hard gate is unmet", () => {
    renderCard({ prerequisites: { ...ALL_MET, entitled: false } });

    expect(screen.getByTestId("private-dns-prerequisites")).toBeInTheDocument();
    expect(screen.getByText(/Premium feature/)).toBeInTheDocument();
    expect(
      screen.getByRole("switch", { name: "Enable Private DNS" }),
    ).toBeDisabled();
  });

  it("allows enabling once every hard gate is met", async () => {
    renderCard({ prerequisites: ALL_MET });

    const toggle = screen.getByRole("switch", { name: "Enable Private DNS" });
    expect(toggle).toBeEnabled();
    await userEvent.click(toggle);
    expect(onSetEnabled).toHaveBeenCalledWith(true);
  });

  it("lists granted devices with their hostname once enabled", () => {
    renderCard({
      enabled: true,
      prerequisites: ALL_MET,
      devices: [makeDevice({ id: "d1", name: "Alice phone" })],
      grants: [GRANT],
    });

    expect(screen.getByText("Alice phone")).toBeInTheDocument();
    expect(screen.getByText("tok.abc.my.wardnet.services")).toBeInTheDocument();
    // The one granted device is no longer grantable, so no grant button.
    expect(screen.queryByText("Grant access")).not.toBeInTheDocument();
  });

  it("grants a device, then the granted modal wires send + dismiss", async () => {
    const user = userEvent.setup();
    grantMutateAsync.mockResolvedValue(GRANT);
    renderCard({
      enabled: true,
      prerequisites: ALL_MET,
      devices: [makeDevice({ id: "d1", name: "Alice phone" })],
      grants: [],
    });

    await user.click(screen.getByRole("button", { name: "Grant access" }));
    // Choose the device in the DeviceSelect combobox, then grant.
    await user.click(screen.getByRole("combobox"));
    await user.click(await screen.findByText("Alice phone"));
    await user.click(screen.getByRole("button", { name: "Grant" }));

    await waitFor(() => expect(grantMutateAsync).toHaveBeenCalledWith("d1"));
    // The granted modal opens; its actions route through the card.
    await user.click(await screen.findByTestId("private-dns-modal-send"));
    expect(onSendToDevice).toHaveBeenCalledWith("d1");
    await user.click(screen.getByRole("button", { name: "Done" }));
  });

  it("cancels the grant panel", async () => {
    const user = userEvent.setup();
    renderCard({
      enabled: true,
      prerequisites: ALL_MET,
      devices: [makeDevice({ id: "d1", name: "Alice phone" })],
    });

    await user.click(screen.getByRole("button", { name: "Grant access" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(
      screen.queryByRole("button", { name: "Grant" }),
    ).not.toBeInTheDocument();
  });

  it("sends to a granted device from the table row", async () => {
    const user = userEvent.setup();
    renderCard({
      enabled: true,
      prerequisites: ALL_MET,
      devices: [makeDevice({ id: "d1", name: "Alice phone" })],
      grants: [GRANT],
    });

    await user.click(screen.getByTestId("private-dns-grant-menu"));
    await user.click(await screen.findByTestId("private-dns-grant-send"));
    expect(onSendToDevice).toHaveBeenCalledWith("d1");
  });

  it("revokes a granted device after confirmation", async () => {
    const user = userEvent.setup();
    renderCard({
      enabled: true,
      prerequisites: ALL_MET,
      devices: [makeDevice({ id: "d1", name: "Alice phone" })],
      grants: [GRANT],
    });

    await user.click(screen.getByTestId("private-dns-grant-menu"));
    await user.click(await screen.findByTestId("private-dns-grant-revoke"));
    // ConfirmDialog opens — confirm the revoke.
    await user.click(await screen.findByRole("button", { name: "Revoke" }));
    expect(onRevokeDevice).toHaveBeenCalledWith("d1");
  });
});
