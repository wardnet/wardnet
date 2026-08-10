import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { DeviceTable } from "@/components/compound/DeviceTable";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { DeviceDnsFilterSettings, DnsFilterProfile } from "@wardnet/js";

describe("DeviceTable", () => {
  it("renders device rows with name, IP, type, and manufacturer fallback", () => {
    renderWithProviders(
      <DeviceTable
        devices={[
          makeDevice({
            id: "d1",
            name: "Laptop",
            last_ip: "10.0.0.2",
            manufacturer: "Dell",
            dhcp_status: "lease",
          }),
          makeDevice({
            id: "d2",
            name: "Phone",
            last_ip: "10.0.0.3",
            manufacturer: null,
            dhcp_status: "reservation",
          }),
        ]}
        onDeviceClick={vi.fn()}
        filterSettings={undefined}
        filterProfiles={undefined}
      />,
    );
    expect(screen.getByText("Laptop")).toBeInTheDocument();
    expect(screen.getByText("10.0.0.2")).toBeInTheDocument();
    expect(screen.getByText("Dell")).toBeInTheDocument();
    expect(screen.getByText("Lease")).toBeInTheDocument();
    expect(screen.getByText("Reserved")).toBeInTheDocument();
    // A device with no known manufacturer says so explicitly — a bare dash gave
    // the admin no way to tell "we don't know" from "the vendor hid it" (#1099).
    expect(screen.getByText("Unknown manufacturer")).toBeInTheDocument();
  });

  it("shows the External DHCP badge", () => {
    renderWithProviders(
      <DeviceTable
        devices={[makeDevice({ id: "d1", dhcp_status: "external" })]}
        onDeviceClick={vi.fn()}
        filterSettings={undefined}
        filterProfiles={undefined}
      />,
    );
    expect(screen.getByText("External")).toBeInTheDocument();
  });

  it("renders the DNS-filter column across its branches", () => {
    const filterProfiles = [
      { id: "p1", name: "Ads" },
      { id: "p2", name: "Malware" },
    ] as DnsFilterProfile[];
    const filterSettings = [
      { device_id: "d-off", enabled: false, profile_ids: [] },
      { device_id: "d-empty", enabled: true, profile_ids: [] },
      { device_id: "d-named", enabled: true, profile_ids: ["p1", "p2"] },
      { device_id: "d-unknown", enabled: true, profile_ids: ["nope"] },
    ] as DeviceDnsFilterSettings[];

    renderWithProviders(
      <DeviceTable
        devices={[
          makeDevice({ id: "d-off", name: "Off" }),
          makeDevice({ id: "d-empty", name: "Empty" }),
          makeDevice({ id: "d-named", name: "Named" }),
          makeDevice({ id: "d-unknown", name: "Unknown" }),
          makeDevice({ id: "d-default", name: "NoRow" }),
        ]}
        onDeviceClick={vi.fn()}
        filterSettings={filterSettings}
        filterProfiles={filterProfiles}
      />,
    );
    expect(screen.getByText("Filtering off")).toBeInTheDocument();
    expect(screen.getByText("Ads, Malware")).toBeInTheDocument();
    // No-settings row and empty-profile row both render "(default)".
    expect(screen.getAllByText("(default)").length).toBeGreaterThanOrEqual(2);
    // Unknown profile id resolves to no name, rendering the em dash
    // (manufacturer columns also render em dashes here).
    expect(screen.getAllByText("-").length).toBeGreaterThan(0);
  });

  it("fires onDeviceClick when a row is clicked", async () => {
    const user = userEvent.setup();
    const onDeviceClick = vi.fn();
    renderWithProviders(
      <DeviceTable
        devices={[makeDevice({ id: "d1", name: "Laptop" })]}
        onDeviceClick={onDeviceClick}
        filterSettings={undefined}
        filterProfiles={undefined}
      />,
    );
    await user.click(screen.getByText("Laptop"));
    expect(onDeviceClick).toHaveBeenCalledWith("d1");
  });
});
