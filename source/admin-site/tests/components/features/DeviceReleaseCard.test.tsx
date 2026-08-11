import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceReleaseCard } from "@/components/features/DeviceReleaseCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { Device } from "@wardnet/js";

const mutateAsync = vi.fn();
const reset = vi.fn();

function renderCard(device: Device, isPending = false) {
  return renderWithProviders(
    <DeviceReleaseCard
      device={device}
      release={{
        mutateAsync,
        reset,
        isPending,
        isError: false,
        error: null,
      }}
    />,
  );
}

describe("DeviceReleaseCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mutateAsync.mockResolvedValue(undefined);
  });

  it("renders nothing for an unmanaged device", () => {
    // There is nothing to release — offering the action would imply the device
    // is configured when it isn't.
    const { container } = renderCard(makeDevice({ managed: false }));
    expect(container).toBeEmptyDOMElement();
  });

  it("renders for a managed device even when it has no name", () => {
    // The #1181 regression in miniature: managed is the server's flag, not
    // `name != null`. A device granted Private DNS but never named is managed
    // and must be releasable.
    renderCard(makeDevice({ managed: true, name: null }));
    expect(screen.getByTestId("device-release-card")).toBeInTheDocument();
  });

  it("enumerates every revert, flagging the two that disconnect the device", () => {
    renderCard(makeDevice({ managed: true }));
    const effects = screen.getByTestId("device-release-effects");

    // The two destructive ones are called out as disconnecting.
    expect(effects).toHaveTextContent(
      /Revokes Private DNS access — this disconnects the device/,
    );
    expect(effects).toHaveTextContent(
      /Deletes its remote-access credential — this disconnects the device/,
    );
    // And the rest of the sweep is stated, not implied.
    expect(effects).toHaveTextContent(/DHCP reservation/);
    expect(effects).toHaveTextContent(/zone exceptions/);
    expect(effects).toHaveTextContent(/routing rule and routing profiles/);
    expect(effects).toHaveTextContent(/DNS filter settings/);
    expect(effects).toHaveTextContent(/name and admin lock/);
    expect(effects).toHaveTextContent(/default zone for new devices/);
    // Including the delayed consequence, which is the whole point of the flag.
    expect(effects).toHaveTextContent(/forgotten after 30 days away/);
  });

  it("confirms before releasing, and does not fire on the first click", async () => {
    const user = userEvent.setup();
    renderCard(makeDevice({ managed: true }));

    await user.click(screen.getByTestId("device-release-button"));
    expect(mutateAsync).not.toHaveBeenCalled();
    expect(
      screen.getByText(/Stop managing this device\?/i),
    ).toBeInTheDocument();

    await user.click(screen.getByTestId("confirm-dialog-confirm"));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledWith("dev-1"));
  });

  it("does not release when the confirmation is cancelled", async () => {
    const user = userEvent.setup();
    renderCard(makeDevice({ managed: true }));

    await user.click(screen.getByTestId("device-release-button"));
    await user.click(screen.getByTestId("confirm-dialog-cancel"));

    expect(mutateAsync).not.toHaveBeenCalled();
  });

  it("names the irreversible part in the confirmation", async () => {
    const user = userEvent.setup();
    renderCard(makeDevice({ managed: true }));

    await user.click(screen.getByTestId("device-release-button"));
    // Re-granting mints new credentials; the old ones are gone. An admin
    // should read that before confirming, not discover it afterwards.
    expect(
      screen.getByText(/old ones cannot be restored/i),
    ).toBeInTheDocument();
  });

  it("disables the button while the release is in flight", () => {
    renderCard(makeDevice({ managed: true }), true);
    expect(screen.getByTestId("device-release-button")).toBeDisabled();
    expect(screen.getByText("Releasing…")).toBeInTheDocument();
  });
});
