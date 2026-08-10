import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocalRecordsCard } from "@/components/features/LocalRecordsCard";
import { renderWithProviders } from "../../test-utils";
import type { CustomDnsRecord, DnsZone } from "@wardnet/js";

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

const onCreateRecord = vi.fn();
const onUpdateRecord = vi.fn();
const onDeleteRecord = vi.fn();

const records = [
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
] as CustomDnsRecord[];

const zones = [{ id: "z1", name: "home" }] as DnsZone[];

function renderCard() {
  return renderWithProviders(
    <LocalRecordsCard
      records={records}
      zones={zones}
      isSaving={false}
      updatePending={false}
      onCreateRecord={onCreateRecord}
      onUpdateRecord={onUpdateRecord}
      onDeleteRecord={onDeleteRecord}
    />,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("LocalRecordsCard", () => {
  it("renders only manual records with their zone name", () => {
    renderCard();
    expect(screen.getByText("printer.lan")).toBeInTheDocument();
    expect(screen.queryByText("tv.lan")).not.toBeInTheDocument();
    expect(screen.getByText("home")).toBeInTheDocument();
  });

  it("toggling a record calls the update callback", async () => {
    const user = userEvent.setup();
    renderCard();
    await user.click(screen.getByLabelText("Toggle printer.lan"));
    expect(onUpdateRecord).toHaveBeenCalledWith({
      id: "rec1",
      body: { enabled: false },
    });
  });

  it("creates a record through the add form", async () => {
    const user = userEvent.setup();
    renderCard();
    await user.click(screen.getByTestId("local-record-add"));
    await user.type(screen.getByTestId("local-record-domain"), "nas.lan");
    await user.type(screen.getByTestId("local-record-value"), "10.0.0.5");
    await user.click(screen.getByTestId("local-record-submit"));
    expect(onCreateRecord).toHaveBeenCalledWith(
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
    renderCard();
    await user.click(screen.getByTestId("local-record-add"));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("local-record-domain")).not.toBeInTheDocument();
  });

  it("edits a record via the row menu, preserving its zone", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard();
    await user.click(screen.getByTestId("local-record-row-menu"));
    await user.click(await screen.findByTestId("local-record-edit"));
    const domain = screen.getByTestId(
      "local-record-domain",
    ) as HTMLInputElement;
    expect(domain.value).toBe("printer.lan");
    await user.clear(domain);
    await user.type(domain, "printer2.lan");
    await user.click(screen.getByTestId("local-record-submit"));
    expect(onUpdateRecord).toHaveBeenCalledWith(
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
    renderCard();
    await user.click(screen.getByTestId("local-record-row-menu"));
    await user.click(await screen.findByTestId("local-record-delete"));
    await user.click(await screen.findByTestId("confirm-dialog-confirm"));
    expect(onDeleteRecord).toHaveBeenCalledWith("rec1");
  });
});
