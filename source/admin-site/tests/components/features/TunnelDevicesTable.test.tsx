import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useTunnelDevices } from "@wardnet/web";
import { TunnelDevicesTable } from "@/components/features/TunnelDevicesTable";
import { makeDevice, renderWithProviders } from "../../test-utils";

const navigate = vi.fn();

vi.mock("react-router", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useNavigate: () => navigate };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useTunnelDevices: vi.fn() };
});

const mockUseTunnelDevices = vi.mocked(useTunnelDevices);

function setState(over: Record<string, unknown>) {
  mockUseTunnelDevices.mockReturnValue({
    data: undefined,
    isLoading: false,
    isError: false,
    ...over,
  } as unknown as ReturnType<typeof useTunnelDevices>);
}

describe("TunnelDevicesTable", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the loading state", () => {
    setState({ isLoading: true });
    renderWithProviders(<TunnelDevicesTable tunnelId="t1" />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("renders the error state", () => {
    setState({ isError: true });
    renderWithProviders(<TunnelDevicesTable tunnelId="t1" />);
    expect(
      screen.getByText("Failed to load devices for this tunnel."),
    ).toBeInTheDocument();
  });

  it("renders an empty message when no devices route through the tunnel", () => {
    setState({ data: { devices: [] } });
    renderWithProviders(<TunnelDevicesTable tunnelId="t1" />);
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
    setState({ data: { devices: [device] } });
    renderWithProviders(<TunnelDevicesTable tunnelId="t1" />);

    expect(screen.getByText("Laptop")).toBeInTheDocument();
    expect(screen.getByText("10.0.0.9")).toBeInTheDocument();
    expect(screen.getByText("(1)")).toBeInTheDocument();

    await userEvent.click(screen.getByText("Laptop"));
    expect(navigate).toHaveBeenCalledWith("/devices/dev-42");
  });
});
