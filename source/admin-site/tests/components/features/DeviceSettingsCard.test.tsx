import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceSettingsCard } from "@/components/features/DeviceSettingsCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { Device, RoutingTarget, Tunnel } from "@wardnet/js";

// Radix primitives (Select) measure their trigger in a layout effect; jsdom has
// no ResizeObserver / pointer-capture, so stub them as elsewhere in the suite.
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

const mutateAsync = vi.fn();
const reset = vi.fn();

function makeTunnel(over: Partial<Tunnel> = {}): Tunnel {
  return {
    id: "tun-1",
    label: "US West",
    country_code: "US",
    provider: null,
    interface_name: "wg_ward0",
    endpoint: "1.2.3.4:51820",
    status: "up",
    last_handshake: null,
    bytes_tx: 0,
    bytes_rx: 0,
    created_at: "2026-01-01T00:00:00Z",
    override_default_dns: false,
    server_selector: null,
    resolved_server_name: null,
    endpoint_resolved_at: null,
    ...over,
  } as Tunnel;
}

function cardProps({
  device = makeDevice({ name: "TV" }),
  currentRule = null as RoutingTarget | null,
  tunnels = [] as Tunnel[],
  update = {},
}: {
  device?: Device;
  currentRule?: RoutingTarget | null;
  tunnels?: Tunnel[];
  update?: Partial<{
    isPending: boolean;
    isError: boolean;
    error: Error | null;
  }>;
} = {}) {
  return {
    device,
    currentRule,
    tunnels,
    updateDevice: {
      mutateAsync,
      reset,
      isPending: false,
      isError: false,
      error: null,
      ...update,
    },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mutateAsync.mockResolvedValue(undefined);
});

describe("DeviceSettingsCard routing label", () => {
  it("shows Direct when there is no rule", () => {
    renderWithProviders(<DeviceSettingsCard {...cardProps()} />);
    expect(
      screen.getByTestId("device-settings-routing-value"),
    ).toHaveTextContent("Direct (no VPN)");
  });

  it("shows Direct for an explicit direct rule", () => {
    renderWithProviders(
      <DeviceSettingsCard
        {...cardProps({ currentRule: { type: "direct" } as RoutingTarget })}
      />,
    );
    expect(
      screen.getByTestId("device-settings-routing-value"),
    ).toHaveTextContent("Direct (no VPN)");
  });

  it("shows the tunnel label for a matched tunnel", () => {
    renderWithProviders(
      <DeviceSettingsCard
        {...cardProps({
          tunnels: [makeTunnel({ id: "tun-1", label: "US West" })],
          currentRule: { type: "tunnel", tunnel_id: "tun-1" },
        })}
      />,
    );
    expect(
      screen.getByTestId("device-settings-routing-value"),
    ).toHaveTextContent("US West");
  });

  it("falls back to 'Via tunnel' when the tunnel is unknown", () => {
    renderWithProviders(
      <DeviceSettingsCard
        {...cardProps({
          currentRule: { type: "tunnel", tunnel_id: "missing" },
        })}
      />,
    );
    expect(
      screen.getByTestId("device-settings-routing-value"),
    ).toHaveTextContent("Via tunnel");
  });
});

describe("DeviceSettingsCard editing", () => {
  it("shows read-only fields for a managed device", () => {
    renderWithProviders(
      <DeviceSettingsCard
        {...cardProps({
          device: makeDevice({ name: "Laptop", admin_locked: true }),
        })}
      />,
    );
    expect(screen.getByText("Laptop")).toBeInTheDocument();
    expect(screen.getByText("Locked")).toBeInTheDocument();
  });

  it("saves edits for a managed device", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DeviceSettingsCard {...cardProps()} />);
    await user.click(screen.getByTestId("device-settings-edit"));
    const nameInput = screen.getByLabelText("Friendly name");
    await user.clear(nameInput);
    await user.type(nameInput, "Living room");
    await user.click(screen.getByTestId("device-settings-save"));

    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(mutateAsync).toHaveBeenCalledWith({
      id: "dev-1",
      body: {
        name: "Living room",
        device_type: "unknown",
        routing_target: undefined,
        admin_locked: false,
      },
    });
  });

  it("labels the save 'Save' until a name will actually promote the device", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSettingsCard
        {...cardProps({ device: makeDevice({ name: null }) })}
      />,
    );
    await user.click(screen.getByTestId("device-settings-edit"));
    // No name yet — saving just persists settings, so it isn't a "promotion",
    // and the form hints how to manage the device.
    expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();
    expect(screen.getByText(/name to manage it/i)).toBeInTheDocument();
    // Entering a name turns the save into an actual promotion.
    await user.type(screen.getByLabelText("Friendly name"), "Alice phone");
    expect(
      screen.getByRole("button", { name: "To Managed Device" }),
    ).toBeInTheDocument();
  });

  it("saves an unnamed device's settings without forcing a name", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSettingsCard
        {...cardProps({ device: makeDevice({ name: null }) })}
      />,
    );
    await user.click(screen.getByTestId("device-settings-edit"));
    // The whole regression: changing routing/settings on an unnamed device must
    // still save (it just doesn't promote); `name` is omitted, not blocked.
    await user.click(screen.getByTestId("device-settings-save"));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(mutateAsync).toHaveBeenCalledWith(
      expect.objectContaining({
        id: "dev-1",
        body: expect.objectContaining({ name: undefined }),
      }),
    );
  });

  it("promotes and sends the name once one is entered", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSettingsCard
        {...cardProps({ device: makeDevice({ name: null }) })}
      />,
    );
    await user.click(screen.getByTestId("device-settings-edit"));
    await user.type(screen.getByLabelText("Friendly name"), "Alice phone");
    await user.click(screen.getByRole("button", { name: "To Managed Device" }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(mutateAsync).toHaveBeenCalledWith(
      expect.objectContaining({
        id: "dev-1",
        body: expect.objectContaining({ name: "Alice phone" }),
      }),
    );
  });

  it("shows the saving label and error alert while pending", async () => {
    const user = userEvent.setup();
    const { rerender } = renderWithProviders(
      <DeviceSettingsCard {...cardProps()} />,
    );
    await user.click(screen.getByTestId("device-settings-edit"));
    rerender(
      <DeviceSettingsCard
        {...cardProps({
          update: { isPending: true, isError: true, error: new Error("x") },
        })}
      />,
    );
    expect(screen.getByRole("button", { name: "Saving…" })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("cancels editing", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DeviceSettingsCard {...cardProps()} />);
    await user.click(screen.getByTestId("device-settings-edit"));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByTestId("device-settings-edit")).toBeInTheDocument();
    expect(reset).toHaveBeenCalled();
  });
});
