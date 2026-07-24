import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { PrivateDnsGrantSummary } from "@wardnet/js";
import { PrivateDnsGrantsTable } from "@/components/compound/PrivateDnsGrantsTable";
import { renderWithProviders } from "../../test-utils";

const grant: PrivateDnsGrantSummary = {
  id: "g1",
  device_id: "d1",
  hostname: "tok.abc.my.wardnet.services",
  created_at: "2026-07-21T00:00:00Z",
};

describe("PrivateDnsGrantsTable", () => {
  it("shows an empty state when there are no grants", () => {
    renderWithProviders(
      <PrivateDnsGrantsTable
        grants={[]}
        deviceName={() => "x"}
        onSendToDevice={vi.fn()}
        onRevoke={vi.fn()}
      />,
    );
    expect(
      screen.getByText(/No devices have Private DNS access yet/),
    ).toBeInTheDocument();
  });

  it("renders the resolved device name and hostname", () => {
    renderWithProviders(
      <PrivateDnsGrantsTable
        grants={[grant]}
        deviceName={(id) => (id === "d1" ? "Alice phone" : "?")}
        onSendToDevice={vi.fn()}
        onRevoke={vi.fn()}
      />,
    );
    expect(screen.getByText("Alice phone")).toBeInTheDocument();
    expect(screen.getByText("tok.abc.my.wardnet.services")).toBeInTheDocument();
  });

  it("fires the send and revoke callbacks from the row menu", async () => {
    const user = userEvent.setup();
    const onSendToDevice = vi.fn();
    const onRevoke = vi.fn();
    renderWithProviders(
      <PrivateDnsGrantsTable
        grants={[grant]}
        deviceName={() => "Alice"}
        onSendToDevice={onSendToDevice}
        onRevoke={onRevoke}
      />,
    );

    await user.click(screen.getByTestId("private-dns-grant-menu"));
    await user.click(await screen.findByTestId("private-dns-grant-send"));
    expect(onSendToDevice).toHaveBeenCalledWith(grant);

    await user.click(screen.getByTestId("private-dns-grant-menu"));
    await user.click(await screen.findByTestId("private-dns-grant-revoke"));
    expect(onRevoke).toHaveBeenCalledWith(grant);
  });
});
