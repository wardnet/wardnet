import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { HostCell, buildDeviceIndex } from "@/components/compound/HostCell";
import { makeDevice, renderWithProviders } from "../../test-utils";

describe("buildDeviceIndex", () => {
  it("maps devices by MAC", () => {
    const a = makeDevice({ mac: "aa:bb", name: "A" });
    const b = makeDevice({ mac: "cc:dd", name: "B" });
    const index = buildDeviceIndex([a, b]);
    expect(index.get("aa:bb")).toBe(a);
    expect(index.get("cc:dd")).toBe(b);
    expect(index.size).toBe(2);
  });

  it("returns an empty map for no devices", () => {
    expect(buildDeviceIndex([]).size).toBe(0);
  });
});

describe("HostCell", () => {
  it("renders the primary line and icon", () => {
    renderWithProviders(
      <HostCell primary="Laptop" secondary={null} icon={<span>ic</span>} />,
    );
    expect(screen.getByText("Laptop")).toBeInTheDocument();
    expect(screen.getByText("ic")).toBeInTheDocument();
  });

  it("renders the secondary line when provided", () => {
    renderWithProviders(
      <HostCell primary="Laptop" secondary="aa:bb:cc" icon={<span>ic</span>} />,
    );
    expect(screen.getByText("aa:bb:cc")).toBeInTheDocument();
  });

  it("omits the secondary line when null", () => {
    const { container } = renderWithProviders(
      <HostCell primary="Laptop" secondary={null} icon={<span>ic</span>} />,
    );
    expect(container.querySelector(".mac")).not.toBeInTheDocument();
  });

  it("renders extra content when provided", () => {
    renderWithProviders(
      <HostCell
        primary="Laptop"
        secondary={null}
        extra={<span>10.0.0.1</span>}
        icon={<span>ic</span>}
      />,
    );
    expect(screen.getByText("10.0.0.1")).toBeInTheDocument();
  });
});
