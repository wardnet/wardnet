import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { PrivateDnsGrantSummary } from "@wardnet/js";
import { PrivateDnsGrantedModal } from "@/components/features/PrivateDnsGrantedModal";
import { renderWithProviders } from "../../test-utils";

const grant: PrivateDnsGrantSummary = {
  id: "g1",
  device_id: "d1",
  hostname: "tok.abc.my.wardnet.services",
  created_at: "2026-07-21T00:00:00Z",
};

describe("PrivateDnsGrantedModal", () => {
  it("renders nothing when there is no grant", () => {
    const { container } = renderWithProviders(
      <PrivateDnsGrantedModal
        grant={null}
        deviceName=""
        onSendToDevice={vi.fn()}
        sending={false}
        onDismiss={vi.fn()}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the hostname/instructions and wires the send + dismiss actions", async () => {
    const onSendToDevice = vi.fn();
    const onDismiss = vi.fn();
    renderWithProviders(
      <PrivateDnsGrantedModal
        grant={grant}
        deviceName="Alice phone"
        onSendToDevice={onSendToDevice}
        sending={false}
        onDismiss={onDismiss}
      />,
    );

    expect(screen.getByTestId("private-dns-hostname")).toHaveTextContent(
      "tok.abc.my.wardnet.services",
    );

    await userEvent.click(screen.getByTestId("private-dns-modal-send"));
    expect(onSendToDevice).toHaveBeenCalledWith(grant);

    await userEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(onDismiss).toHaveBeenCalled();
  });
});
