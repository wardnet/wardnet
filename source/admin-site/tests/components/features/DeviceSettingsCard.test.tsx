import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceSettingsCard } from "@/components/features/DeviceSettingsCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { RoutingTarget, Tunnel } from "@wardnet/js";

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

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useTunnels: vi.fn(), useUpdateDevice: vi.fn() };
});

import { useTunnels, useUpdateDevice } from "@wardnet/web";

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

function setup({
  tunnels = [] as Tunnel[],
  update = {},
}: { tunnels?: Tunnel[]; update?: Record<string, unknown> } = {}) {
  vi.mocked(useTunnels).mockReturnValue({
    data: { tunnels },
  } as unknown as ReturnType<typeof useTunnels>);
  vi.mocked(useUpdateDevice).mockReturnValue({
    mutateAsync,
    reset,
    isPending: false,
    isError: false,
    error: null,
    ...update,
  } as unknown as ReturnType<typeof useUpdateDevice>);
}

beforeEach(() => {
  vi.clearAllMocks();
  mutateAsync.mockResolvedValue(undefined);
  setup();
});

describe("DeviceSettingsCard routing label", () => {
  it("shows Direct when there is no rule", () => {
    renderWithProviders(
      <DeviceSettingsCard
        device={makeDevice({ name: "TV" })}
        currentRule={null}
      />,
    );
    expect(
      screen.getByTestId("device-settings-routing-value"),
    ).toHaveTextContent("Direct (no VPN)");
  });

  it("shows Direct for an explicit direct rule", () => {
    renderWithProviders(
      <DeviceSettingsCard
        device={makeDevice({ name: "TV" })}
        currentRule={{ type: "direct" } as RoutingTarget}
      />,
    );
    expect(
      screen.getByTestId("device-settings-routing-value"),
    ).toHaveTextContent("Direct (no VPN)");
  });

  it("shows the tunnel label for a matched tunnel", () => {
    setup({ tunnels: [makeTunnel({ id: "tun-1", label: "US West" })] });
    renderWithProviders(
      <DeviceSettingsCard
        device={makeDevice({ name: "TV" })}
        currentRule={{ type: "tunnel", tunnel_id: "tun-1" }}
      />,
    );
    expect(
      screen.getByTestId("device-settings-routing-value"),
    ).toHaveTextContent("US West");
  });

  it("falls back to 'Via tunnel' when the tunnel is unknown", () => {
    renderWithProviders(
      <DeviceSettingsCard
        device={makeDevice({ name: "TV" })}
        currentRule={{ type: "tunnel", tunnel_id: "missing" }}
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
        device={makeDevice({ name: "Laptop", admin_locked: true })}
        currentRule={null}
      />,
    );
    expect(screen.getByText("Laptop")).toBeInTheDocument();
    expect(screen.getByText("Locked")).toBeInTheDocument();
  });

  it("saves edits for a managed device", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSettingsCard
        device={makeDevice({ name: "TV" })}
        currentRule={null}
      />,
    );
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

  it("uses promote copy for an unmanaged device", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSettingsCard
        device={makeDevice({ name: null })}
        currentRule={null}
      />,
    );
    await user.click(screen.getByTestId("device-settings-edit"));
    expect(
      screen.getByRole("button", { name: "To Managed Device" }),
    ).toBeInTheDocument();
  });

  it("blocks promoting an unmanaged device without a name and says why on submit", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSettingsCard
        device={makeDevice({ name: null })}
        currentRule={null}
      />,
    );
    await user.click(screen.getByTestId("device-settings-edit"));
    // The error is not shown just for entering edit mode.
    expect(screen.queryByText(/name is required/i)).not.toBeInTheDocument();
    // The whole point of the bug: promoting with an empty name used to "succeed"
    // as a silent no-op. Submitting surfaces the requirement and blocks the save.
    await user.click(screen.getByTestId("device-settings-save"));
    expect(await screen.findByText(/name is required/i)).toBeInTheDocument();
    expect(mutateAsync).not.toHaveBeenCalled();
  });

  it("enables promotion and sends the name once one is entered", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSettingsCard
        device={makeDevice({ name: null })}
        currentRule={null}
      />,
    );
    await user.click(screen.getByTestId("device-settings-edit"));
    await user.type(screen.getByLabelText("Friendly name"), "Alice phone");
    const save = screen.getByRole("button", { name: "To Managed Device" });
    expect(save).toBeEnabled();
    await user.click(save);
    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(mutateAsync).toHaveBeenCalledWith(
      expect.objectContaining({
        id: "dev-1",
        body: expect.objectContaining({ name: "Alice phone" }),
      }),
    );
  });

  it("blocks blanking the name of an already-managed device on submit", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSettingsCard
        device={makeDevice({ name: "TV" })}
        currentRule={null}
      />,
    );
    await user.click(screen.getByTestId("device-settings-edit"));
    await user.clear(screen.getByLabelText("Friendly name"));
    await user.click(screen.getByTestId("device-settings-save"));
    expect(await screen.findByText(/name is required/i)).toBeInTheDocument();
    expect(mutateAsync).not.toHaveBeenCalled();
  });

  it("shows the saving label and error alert while pending", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSettingsCard
        device={makeDevice({ name: "TV" })}
        currentRule={null}
      />,
    );
    await user.click(screen.getByTestId("device-settings-edit"));
    setup({
      update: { isPending: true, isError: true, error: new Error("x") },
    });
    await user.type(screen.getByLabelText("Friendly name"), "x");
    expect(screen.getByRole("button", { name: "Saving…" })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("cancels editing", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSettingsCard
        device={makeDevice({ name: "TV" })}
        currentRule={null}
      />,
    );
    await user.click(screen.getByTestId("device-settings-edit"));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByTestId("device-settings-edit")).toBeInTheDocument();
    expect(reset).toHaveBeenCalled();
  });
});
