import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  useDnsRecords,
  useDnsZones,
  useCreateDnsRecord,
  useUpdateDnsRecord,
  useDeleteDnsRecord,
} from "@wardnet/web";
import { LocalRecordsCard } from "@/components/features/LocalRecordsCard";
import { renderWithProviders } from "../../test-utils";

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
    useDnsRecords: vi.fn(),
    useDnsZones: vi.fn(),
    useCreateDnsRecord: vi.fn(),
    useUpdateDnsRecord: vi.fn(),
    useDeleteDnsRecord: vi.fn(),
  };
});

const createMutate = vi.fn();
const updateMutate = vi.fn();
const deleteMutate = vi.fn();

function mutation<T>(mutate: ReturnType<typeof vi.fn>): T {
  return {
    mutate,
    mutateAsync: vi.fn(),
    isPending: false,
  } as unknown as T;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(useDnsRecords).mockReturnValue({
    data: {
      records: [
        {
          id: "rec1",
          source: "manual",
          domain: "printer.lan",
          record_type: "A",
          value: "192.168.1.50",
          ttl: 300,
          zone_id: "z1",
          enabled: true,
        },
        // DHCP-sourced record is filtered out.
        {
          id: "rec2",
          source: "dhcp",
          domain: "tv.lan",
          record_type: "A",
          value: "192.168.1.60",
          ttl: 300,
          zone_id: null,
          enabled: true,
        },
      ],
    },
  } as unknown as ReturnType<typeof useDnsRecords>);
  vi.mocked(useDnsZones).mockReturnValue({
    data: { zones: [{ id: "z1", name: "home" }] },
  } as unknown as ReturnType<typeof useDnsZones>);
  vi.mocked(useCreateDnsRecord).mockReturnValue(
    mutation<ReturnType<typeof useCreateDnsRecord>>(createMutate),
  );
  vi.mocked(useUpdateDnsRecord).mockReturnValue(
    mutation<ReturnType<typeof useUpdateDnsRecord>>(updateMutate),
  );
  vi.mocked(useDeleteDnsRecord).mockReturnValue(
    mutation<ReturnType<typeof useDeleteDnsRecord>>(deleteMutate),
  );
});

describe("LocalRecordsCard", () => {
  it("renders only manual records with their zone name", () => {
    renderWithProviders(<LocalRecordsCard />);
    expect(screen.getByText("printer.lan")).toBeInTheDocument();
    expect(screen.queryByText("tv.lan")).not.toBeInTheDocument();
    expect(screen.getByText("home")).toBeInTheDocument();
  });

  it("toggling a record calls updateRecord.mutate", async () => {
    const user = userEvent.setup();
    renderWithProviders(<LocalRecordsCard />);
    await user.click(screen.getByLabelText("Toggle printer.lan"));
    expect(updateMutate).toHaveBeenCalledWith({
      id: "rec1",
      body: { enabled: false },
    });
  });

  it("creates a record through the add form", async () => {
    const user = userEvent.setup();
    renderWithProviders(<LocalRecordsCard />);
    await user.click(screen.getByTestId("local-record-add"));
    await user.type(screen.getByTestId("local-record-domain"), "nas.lan");
    await user.type(screen.getByTestId("local-record-value"), "10.0.0.5");
    await user.click(screen.getByTestId("local-record-submit"));
    expect(createMutate).toHaveBeenCalledWith(
      {
        zone_id: null,
        domain: "nas.lan",
        record_type: "A",
        value: "10.0.0.5",
        ttl: 300,
        enabled: true,
      },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("cancelling closes the form", async () => {
    const user = userEvent.setup();
    renderWithProviders(<LocalRecordsCard />);
    await user.click(screen.getByTestId("local-record-add"));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("local-record-domain")).not.toBeInTheDocument();
  });

  it("edits a record via the row menu, preserving its zone", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<LocalRecordsCard />);
    await user.click(screen.getByTestId("local-record-row-menu"));
    await user.click(await screen.findByTestId("local-record-edit"));
    const domain = screen.getByTestId(
      "local-record-domain",
    ) as HTMLInputElement;
    expect(domain.value).toBe("printer.lan");
    await user.clear(domain);
    await user.type(domain, "printer2.lan");
    await user.click(screen.getByTestId("local-record-submit"));
    expect(updateMutate).toHaveBeenCalledWith(
      {
        id: "rec1",
        body: {
          zone_id: "z1",
          domain: "printer2.lan",
          record_type: "A",
          value: "192.168.1.50",
          ttl: 300,
        },
      },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("deletes a record after confirming", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<LocalRecordsCard />);
    await user.click(screen.getByTestId("local-record-row-menu"));
    await user.click(await screen.findByTestId("local-record-delete"));
    await user.click(await screen.findByTestId("confirm-dialog-confirm"));
    expect(deleteMutate).toHaveBeenCalledWith("rec1");
  });
});
