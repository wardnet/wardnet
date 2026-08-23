import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { DhcpLease, DhcpReservation } from "@wardnet/js";
import { DhcpEntryTable } from "@/components/compound/DhcpEntryTable";
import { makeDevice, renderWithProviders } from "../../test-utils";

function makeReservation(
  overrides: Partial<DhcpReservation> = {},
): DhcpReservation {
  return {
    id: "res-1",
    mac_address: "AA:BB:CC:DD:EE:01",
    ip_address: "10.232.1.10",
    hostname: "printer",
    description: "Office printer",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function makeLease(overrides: Partial<DhcpLease> = {}): DhcpLease {
  return {
    id: "lease-1",
    mac_address: "AA:BB:CC:DD:EE:02",
    ip_address: "10.232.1.20",
    hostname: "laptop",
    lease_start: "2026-01-01T00:00:00Z",
    lease_end: "2026-01-02T00:00:00Z",
    status: "active",
    device_id: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

const noop = {
  onMakeStatic: vi.fn(),
  onRevokeLease: vi.fn(),
  onDeleteReservation: vi.fn(),
  onAddReservation: vi.fn(),
  onGroupChange: vi.fn(),
  onSearchChange: vi.fn(),
};

// biome-ignore lint/security/noSecrets: identifier-shaped string, not a credential — the entropy heuristic misfires on long CamelCase names
describe("DhcpEntryTable", () => {
  it("shows the discovery placeholder when there are no entries", () => {
    renderWithProviders(
      <DhcpEntryTable
        {...noop}
        leases={[]}
        reservations={[]}
        devices={[]}
        activeGroup="all"
        searchValue=""
      />,
    );
    expect(screen.getByText("Waiting for DHCP activity")).toBeInTheDocument();
  });

  it("renders reservation and lease rows with device-aware host cells", () => {
    renderWithProviders(
      <DhcpEntryTable
        {...noop}
        leases={[makeLease()]}
        reservations={[makeReservation()]}
        devices={[
          makeDevice({
            id: "dev-9",
            mac: "aa:bb:cc:dd:ee:02",
            name: "Known laptop",
          }),
        ]}
        activeGroup="all"
        searchValue=""
      />,
    );
    // Reservation with no device match uses its description (shown in the
    // host cell and the description column).
    expect(screen.getAllByText("Office printer").length).toBeGreaterThan(0);
    // Lease MAC matches a device, so the device name wins.
    expect(screen.getByText("Known laptop")).toBeInTheDocument();
    expect(screen.getByText("Static")).toBeInTheDocument();
    expect(screen.getByText("Lease")).toBeInTheDocument();
  });

  it("falls back to the MAC when nothing friendlier exists", () => {
    renderWithProviders(
      <DhcpEntryTable
        {...noop}
        leases={[
          makeLease({
            id: "l-2",
            mac_address: "11:22:33:44:55:66",
            hostname: null,
          }),
        ]}
        reservations={[]}
        devices={[]}
        activeGroup="all"
        searchValue=""
      />,
    );
    expect(screen.getByText("11:22:33:44:55:66")).toBeInTheDocument();
  });

  it("deletes a reservation from the row menu", async () => {
    const user = userEvent.setup();
    const onDeleteReservation = vi.fn();
    renderWithProviders(
      <DhcpEntryTable
        {...noop}
        onDeleteReservation={onDeleteReservation}
        leases={[]}
        reservations={[makeReservation({ id: "res-9" })]}
        devices={[]}
        activeGroup="all"
        searchValue=""
      />,
    );
    await user.click(screen.getByTestId("dhcp-entry-menu"));
    await user.click(await screen.findByTestId("dhcp-entry-delete"));
    expect(onDeleteReservation).toHaveBeenCalledWith("res-9");
  });

  it("offers make-static and revoke for an active lease", async () => {
    const user = userEvent.setup();
    const onMakeStatic = vi.fn();
    const onRevokeLease = vi.fn();
    const lease = makeLease({ id: "l-9", status: "active" });
    renderWithProviders(
      <DhcpEntryTable
        {...noop}
        onMakeStatic={onMakeStatic}
        onRevokeLease={onRevokeLease}
        leases={[lease]}
        reservations={[]}
        devices={[]}
        activeGroup="all"
        searchValue=""
      />,
    );
    await user.click(screen.getByTestId("dhcp-entry-menu"));
    await user.click(await screen.findByTestId("dhcp-entry-make-static"));
    expect(onMakeStatic).toHaveBeenCalledWith(
      expect.objectContaining({ id: "l-9" }),
    );

    await user.click(screen.getByTestId("dhcp-entry-menu"));
    await user.click(await screen.findByTestId("dhcp-entry-revoke"));
    expect(onRevokeLease).toHaveBeenCalledWith("l-9");
  });

  it("renders no row-actions trigger for a non-active lease", () => {
    // Expired leases have no available actions; the shared DataTable must not
    // render a `…` trigger that would open an empty dropdown.
    renderWithProviders(
      <DhcpEntryTable
        {...noop}
        leases={[makeLease({ id: "l-exp", status: "expired" })]}
        reservations={[]}
        devices={[]}
        activeGroup="leases"
        searchValue=""
      />,
    );
    expect(screen.getByText("Expired")).toBeInTheDocument();
    expect(screen.queryByTestId("dhcp-entry-menu")).not.toBeInTheDocument();
  });

  it("filters to reservations only when that group is active", () => {
    renderWithProviders(
      <DhcpEntryTable
        {...noop}
        leases={[makeLease({ hostname: "laptop" })]}
        reservations={[makeReservation({ description: "Office printer" })]}
        devices={[]}
        activeGroup="reservations"
        searchValue=""
      />,
    );
    expect(screen.getAllByText("Office printer").length).toBeGreaterThan(0);
    expect(screen.queryByText("laptop")).not.toBeInTheDocument();
  });

  it("filters rows by the search query", () => {
    renderWithProviders(
      <DhcpEntryTable
        {...noop}
        leases={[makeLease({ hostname: "laptop", ip_address: "10.0.0.99" })]}
        reservations={[
          makeReservation({ description: "printer", ip_address: "10.0.0.10" }),
        ]}
        devices={[]}
        activeGroup="all"
        searchValue="10.0.0.99"
      />,
    );
    expect(screen.getByText("laptop")).toBeInTheDocument();
    expect(screen.queryByText("printer")).not.toBeInTheDocument();
  });

  it("navigates on row click when the MAC matches a known device", async () => {
    const user = userEvent.setup();
    const onDeviceClick = vi.fn();
    renderWithProviders(
      <DhcpEntryTable
        {...noop}
        onDeviceClick={onDeviceClick}
        leases={[makeLease({ mac_address: "AA:BB:CC:DD:EE:02" })]}
        reservations={[]}
        devices={[
          makeDevice({
            id: "dev-42",
            mac: "aa:bb:cc:dd:ee:02",
            name: "Known laptop",
          }),
        ]}
        activeGroup="all"
        searchValue=""
      />,
    );
    await user.click(screen.getByText("Known laptop"));
    expect(onDeviceClick).toHaveBeenCalledWith("dev-42");
  });

  it("orders rows alphabetically by the host label", () => {
    renderWithProviders(
      <DhcpEntryTable
        {...noop}
        leases={[
          makeLease({
            id: "l1",
            mac_address: "AA:BB:CC:DD:EE:02",
            hostname: "zeta",
          }),
          makeLease({
            id: "l2",
            mac_address: "AA:BB:CC:DD:EE:03",
            hostname: "Alpha",
          }),
        ]}
        reservations={[
          makeReservation({
            id: "r1",
            mac_address: "AA:BB:CC:DD:EE:01",
            hostname: "beta",
          }),
        ]}
        devices={[]}
        activeGroup="all"
        searchValue=""
      />,
    );
    const rows = screen.getAllByRole("row").slice(1); // drop the header row
    const hosts = rows.map((r) => r.textContent);
    expect(hosts).toEqual([
      expect.stringContaining("Alpha"),
      expect.stringContaining("beta"),
      expect.stringContaining("zeta"),
    ]);
  });

  // An empty hostname must degrade to the MAC, not sort as "" above every
  // named row while rendering a blank cell.
  it("falls back to the MAC for a blank hostname instead of sorting it first", () => {
    renderWithProviders(
      <DhcpEntryTable
        {...noop}
        leases={[
          makeLease({
            id: "l1",
            mac_address: "ZZ:ZZ:ZZ:ZZ:ZZ:99",
            hostname: "",
          }),
          makeLease({
            id: "l2",
            mac_address: "AA:BB:CC:DD:EE:03",
            hostname: "Alpha",
          }),
        ]}
        reservations={[]}
        devices={[]}
        activeGroup="all"
        searchValue=""
      />,
    );
    const rows = screen.getAllByRole("row").slice(1);
    expect(rows.map((r) => r.textContent)).toEqual([
      expect.stringContaining("Alpha"),
      expect.stringContaining("ZZ:ZZ:ZZ:ZZ:ZZ:99"),
    ]);
  });
});
