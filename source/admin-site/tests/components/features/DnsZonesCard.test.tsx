import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  useDnsZones,
  useDnsRecords,
  useCreateDnsZone,
  useUpdateDnsZone,
  useDeleteDnsZone,
} from "@wardnet/web";
import { DnsZonesCard } from "@/components/features/DnsZonesCard";
import { renderWithProviders } from "../../test-utils";

// Radix DropdownMenu / Toggle need these DOM APIs that jsdom lacks.
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
    useDnsZones: vi.fn(),
    useDnsRecords: vi.fn(),
    useCreateDnsZone: vi.fn(),
    useUpdateDnsZone: vi.fn(),
    useDeleteDnsZone: vi.fn(),
  };
});

const createMutate = vi.fn();
const updateMutate = vi.fn();
const deleteMutate = vi.fn();

function mutation<T>(mutate: ReturnType<typeof vi.fn>, isPending = false): T {
  return { mutate, mutateAsync: vi.fn(), isPending } as unknown as T;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(useDnsZones).mockReturnValue({
    data: {
      zones: [
        { id: "z1", name: "home", enabled: true, source: "user" },
        { id: "z2", name: "lan", enabled: false, source: "system" },
      ],
    },
  } as unknown as ReturnType<typeof useDnsZones>);
  vi.mocked(useDnsRecords).mockReturnValue({
    data: {
      records: [{ zone_id: "z1" }, { zone_id: "z1" }, { zone_id: null }],
    },
  } as unknown as ReturnType<typeof useDnsRecords>);
  vi.mocked(useCreateDnsZone).mockReturnValue(
    mutation<ReturnType<typeof useCreateDnsZone>>(createMutate),
  );
  vi.mocked(useUpdateDnsZone).mockReturnValue(
    mutation<ReturnType<typeof useUpdateDnsZone>>(updateMutate),
  );
  vi.mocked(useDeleteDnsZone).mockReturnValue(
    mutation<ReturnType<typeof useDeleteDnsZone>>(deleteMutate),
  );
});

describe("DnsZonesCard", () => {
  it("renders zones with their derived record counts", () => {
    renderWithProviders(<DnsZonesCard />);
    expect(screen.getByText("home")).toBeInTheDocument();
    expect(screen.getByText("lan")).toBeInTheDocument();
    // z1 has 2 records; z2 (lan) has 0.
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("toggling a zone's enabled switch calls updateZone.mutate", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsZonesCard />);
    await user.click(screen.getByLabelText("Toggle zone home"));
    expect(updateMutate).toHaveBeenCalledWith({
      id: "z1",
      body: { enabled: false },
    });
  });

  it("creates a zone through the add form", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsZonesCard />);
    await user.click(screen.getByTestId("zone-add"));
    await user.type(screen.getByTestId("zone-name"), "office");
    await user.click(screen.getByTestId("zone-submit"));
    expect(createMutate).toHaveBeenCalledWith(
      { name: "office", enabled: true },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("warns when the zone name looks like a public domain", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsZonesCard />);
    await user.click(screen.getByTestId("zone-add"));
    await user.type(screen.getByTestId("zone-name"), "example.com");
    expect(screen.getByText(/looks like a public domain/i)).toBeInTheDocument();
  });

  it("cancelling the form closes it", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsZonesCard />);
    await user.click(screen.getByTestId("zone-add"));
    expect(screen.getByTestId("zone-name")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("zone-name")).not.toBeInTheDocument();
  });

  it("edits an existing zone via the row menu", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<DnsZonesCard />);
    // The first row (home) is a non-system zone with edit + delete.
    const menus = screen.getAllByTestId("zone-row-menu");
    await user.click(menus[0]);
    await user.click(await screen.findByTestId("zone-edit"));
    const input = screen.getByTestId("zone-name") as HTMLInputElement;
    expect(input.value).toBe("home");
    await user.clear(input);
    await user.type(input, "casa");
    await user.click(screen.getByTestId("zone-submit"));
    expect(updateMutate).toHaveBeenCalledWith(
      { id: "z1", body: { name: "casa" } },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("deletes a zone after confirming", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<DnsZonesCard />);
    const menus = screen.getAllByTestId("zone-row-menu");
    await user.click(menus[0]);
    await user.click(await screen.findByTestId("zone-delete"));
    await user.click(await screen.findByTestId("confirm-dialog-confirm"));
    expect(deleteMutate).toHaveBeenCalledWith("z1");
  });

  it("hides the delete action for system zones", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<DnsZonesCard />);
    const menus = screen.getAllByTestId("zone-row-menu");
    // Second row is the system 'lan' zone — only Edit is offered.
    await user.click(menus[1]);
    expect(await screen.findByTestId("zone-edit")).toBeInTheDocument();
    expect(screen.queryByTestId("zone-delete")).not.toBeInTheDocument();
  });
});
