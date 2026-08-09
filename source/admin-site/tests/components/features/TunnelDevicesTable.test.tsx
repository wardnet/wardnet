import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TunnelDevicesTable } from "@/components/features/TunnelDevicesTable";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { Device } from "@wardnet/js";

const navigate = vi.fn();

vi.mock("react-router", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useNavigate: () => navigate };
});

function renderTable({
  devices = undefined as Device[] | undefined,
  isLoading = false,
  isError = false,
} = {}) {
  return renderWithProviders(
    <TunnelDevicesTable
      devices={devices}
      isLoading={isLoading}
      isError={isError}
    />,
  );
}

describe("TunnelDevicesTable", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the loading state", () => {
    renderTable({ isLoading: true });
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("renders the error state", () => {
    renderTable({ isError: true });
    expect(
      screen.getByText("Failed to load devices for this tunnel."),
    ).toBeInTheDocument();
  });

  it("renders an empty message when no devices route through the tunnel", () => {
    renderTable({ devices: [] });
    expect(
      screen.getByText("No devices are currently routed through this tunnel."),
    ).toBeInTheDocument();
    // Count in the header reflects zero devices.
    expect(screen.getByText("(0)")).toBeInTheDocument();
  });

  it("renders devices and navigates on row click", async () => {
    const device = makeDevice({
      id: "dev-42",
      name: "Laptop",
      last_ip: "10.0.0.9",
    });
    renderTable({ devices: [device] });

    expect(screen.getByText("Laptop")).toBeInTheDocument();
    expect(screen.getByText("10.0.0.9")).toBeInTheDocument();
    expect(screen.getByText("(1)")).toBeInTheDocument();

    await userEvent.click(screen.getByText("Laptop"));
    expect(navigate).toHaveBeenCalledWith("/devices/dev-42");
  });
});
