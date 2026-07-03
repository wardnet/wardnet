import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  useNetworkZones,
  useCreateNetworkZone,
  useUpdateNetworkZone,
  useDeleteNetworkZone,
} from "@wardnet/web";
import { ZonesCard } from "@/components/features/ZonesCard";
import { renderWithProviders } from "../../test-utils";
import type { NetworkZoneView } from "@wardnet/js";

// Radix DropdownMenu / Select need these DOM APIs that jsdom lacks.
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
  return {
    ...actual,
    useNetworkZones: vi.fn(),
    useCreateNetworkZone: vi.fn(),
    useUpdateNetworkZone: vi.fn(),
    useDeleteNetworkZone: vi.fn(),
  };
});

const createMutate = vi.fn();
const updateMutate = vi.fn();
const deleteMutate = vi.fn();

function mutation<T>(mutate: ReturnType<typeof vi.fn>): T {
  return { mutate, mutateAsync: vi.fn(), isPending: false } as unknown as T;
}

function makeZone(over: Partial<NetworkZoneView> = {}): NetworkZoneView {
  return {
    id: "z-trusted",
    name: "Trusted",
    provenance: "system",
    isolation_stance: "shared_subnet",
    allowed_targets: ["direct", "tunnel"],
    member_isolation: false,
    subnet: null,
    admin_ui_reachable: true,
    is_default: true,
    is_default_for_new: false,
    member_count: 2,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...over,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(useNetworkZones).mockReturnValue({
    data: {
      zones: [
        makeZone(),
        makeZone({
          id: "z-iot",
          name: "IoT",
          provenance: "manual",
          is_default: false,
          is_default_for_new: false,
          member_count: 0,
        }),
      ],
    },
  } as unknown as ReturnType<typeof useNetworkZones>);
  vi.mocked(useCreateNetworkZone).mockReturnValue(
    mutation<ReturnType<typeof useCreateNetworkZone>>(createMutate),
  );
  vi.mocked(useUpdateNetworkZone).mockReturnValue(
    mutation<ReturnType<typeof useUpdateNetworkZone>>(updateMutate),
  );
  vi.mocked(useDeleteNetworkZone).mockReturnValue(
    mutation<ReturnType<typeof useDeleteNetworkZone>>(deleteMutate),
  );
});

describe("ZonesCard", () => {
  it("renders zones with their status pills", () => {
    renderWithProviders(<ZonesCard />);
    expect(screen.getByText("Trusted")).toBeInTheDocument();
    expect(screen.getByText("IoT")).toBeInTheDocument();
    expect(screen.getByText("Home")).toBeInTheDocument();
    expect(screen.getByText("System")).toBeInTheDocument();
  });

  it("creates a zone with the form defaults", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ZonesCard />);
    await user.click(screen.getByTestId("zone-add"));
    await user.type(screen.getByTestId("zone-name"), "Cameras");
    await user.click(screen.getByTestId("zone-submit"));
    expect(createMutate).toHaveBeenCalledWith(
      {
        name: "Cameras",
        isolation_stance: "shared_subnet",
        allowed_targets: ["direct", "tunnel"],
        member_isolation: false,
        admin_ui_reachable: true,
        subnet: null,
      },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("creates a zone with edited fields (stance, targets, subnet, toggles)", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ZonesCard />);
    await user.click(screen.getByTestId("zone-add"));
    await user.type(screen.getByTestId("zone-name"), "Cameras");

    // Isolation stance → Isolate members.
    await user.click(screen.getByRole("combobox"));
    await user.click(
      await screen.findByRole("option", { name: "Isolate members" }),
    );

    // Drop tunnel so only direct remains; flip the two boolean toggles.
    await user.click(screen.getByLabelText("Allow tunnel routing"));
    await user.click(screen.getByLabelText("Member isolation"));
    await user.click(screen.getByLabelText("Admin UI reachable"));
    await user.type(screen.getByTestId("zone-subnet"), "10.44.0.0/24");

    await user.click(screen.getByTestId("zone-submit"));
    expect(createMutate).toHaveBeenCalledWith(
      {
        name: "Cameras",
        isolation_stance: "isolate_members",
        allowed_targets: ["direct"],
        member_isolation: true,
        admin_ui_reachable: false,
        subnet: { cidr: "10.44.0.0/24" },
      },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("blocks submit when no routing target is allowed", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ZonesCard />);
    await user.click(screen.getByTestId("zone-add"));
    await user.type(screen.getByTestId("zone-name"), "Nowhere");
    await user.click(screen.getByLabelText("Allow direct routing"));
    await user.click(screen.getByLabelText("Allow tunnel routing"));
    expect(
      screen.getByText(/Allow at least one routing target/i),
    ).toBeInTheDocument();
    expect(screen.getByTestId("zone-submit")).toBeDisabled();
  });

  it("edits an existing zone via the row menu", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ZonesCard />);
    await user.click(screen.getAllByTestId("zone-row-menu")[1]);
    await user.click(await screen.findByTestId("zone-edit"));
    const name = screen.getByTestId("zone-name") as HTMLInputElement;
    expect(name.value).toBe("IoT");
    await user.clear(name);
    await user.type(name, "Sensors");
    await user.click(screen.getByTestId("zone-submit"));
    expect(updateMutate).toHaveBeenCalledWith(
      expect.objectContaining({ id: "z-iot" }),
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("sets a zone as the default for new devices via the row menu", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ZonesCard />);
    await user.click(screen.getAllByTestId("zone-row-menu")[1]);
    await user.click(await screen.findByTestId("zone-set-default-for-new"));
    expect(updateMutate).toHaveBeenCalledWith({
      id: "z-iot",
      body: { is_default_for_new: true },
    });
  });

  it("cancels the add form", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ZonesCard />);
    await user.click(screen.getByTestId("zone-add"));
    expect(screen.getByTestId("zone-name")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("zone-name")).not.toBeInTheDocument();
  });

  it("promotes a zone to home via the row menu", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ZonesCard />);
    // Second row (IoT) is a non-default manual zone.
    await user.click(screen.getAllByTestId("zone-row-menu")[1]);
    await user.click(await screen.findByTestId("zone-set-home"));
    expect(updateMutate).toHaveBeenCalledWith({
      id: "z-iot",
      body: { is_default: true },
    });
  });

  it("deletes an empty manual zone after confirming", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ZonesCard />);
    await user.click(screen.getAllByTestId("zone-row-menu")[1]);
    await user.click(await screen.findByTestId("zone-delete"));
    await user.click(await screen.findByTestId("confirm-dialog-confirm"));
    expect(deleteMutate).toHaveBeenCalledWith("z-iot");
  });

  it("hides delete for the protected default/system zone", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ZonesCard />);
    // First row (Trusted) is system + is_default + has members → no delete.
    await user.click(screen.getAllByTestId("zone-row-menu")[0]);
    expect(await screen.findByTestId("zone-edit")).toBeInTheDocument();
    expect(screen.queryByTestId("zone-delete")).not.toBeInTheDocument();
    expect(screen.queryByTestId("zone-set-home")).not.toBeInTheDocument();
  });
});
