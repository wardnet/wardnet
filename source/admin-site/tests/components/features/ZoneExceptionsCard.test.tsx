import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ZoneExceptionsCard } from "@/components/features/ZoneExceptionsCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { NetworkZoneView, ZoneException } from "@wardnet/js";

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

const onCreateException = vi.fn();
const onDeleteException = vi.fn();

function makeZone(over: Partial<NetworkZoneView> = {}): NetworkZoneView {
  return {
    id: "z-guest",
    name: "Guest",
    provenance: "manual",
    isolation_stance: "shared_subnet",
    allowed_targets: ["direct", "tunnel"],
    member_isolation: false,
    subnet: null,
    admin_ui_reachable: true,
    is_default: false,
    is_default_for_new: false,
    member_count: 0,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...over,
  };
}

const casting: ZoneException = {
  id: "e1",
  from: { kind: "device", id: "d1" },
  to: { kind: "zone", id: "z-guest" },
  service: { type: "preset", set: "casting" },
  bidirectional: true,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

function renderCard({
  exceptions = [] as ZoneException[],
  createError = null as Error | null,
  onResetCreateError = vi.fn(),
} = {}) {
  return renderWithProviders(
    <ZoneExceptionsCard
      exceptions={exceptions}
      zones={[makeZone()]}
      devices={[makeDevice({ id: "d1", name: "Phone" })]}
      isSaving={false}
      createError={createError}
      onResetCreateError={onResetCreateError}
      onCreateException={onCreateException}
      onDeleteException={onDeleteException}
    />,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("ZoneExceptionsCard", () => {
  it("resolves endpoint labels and the casting service for an exception", () => {
    renderCard({ exceptions: [casting] });
    expect(screen.getByText("Phone")).toBeInTheDocument();
    expect(screen.getByText("Guest")).toBeInTheDocument();
    expect(screen.getByText("Casting")).toBeInTheDocument();
    // bidirectional renders the ↔ glyph.
    expect(screen.getByText("↔")).toBeInTheDocument();
  });

  it("shows the empty state and opens the add-exception form", async () => {
    const user = userEvent.setup();
    renderCard();
    expect(
      screen.getByText(/No cross-zone exceptions yet/i),
    ).toBeInTheDocument();
    await user.click(screen.getByTestId("exception-add"));
    expect(screen.getByTestId("exception-service")).toBeInTheDocument();
    // Submit is disabled until two distinct endpoints are picked.
    expect(screen.getByTestId("exception-submit")).toBeDisabled();
  });

  it("labels a custom-port service by its port count", () => {
    renderCard({
      exceptions: [
        {
          ...casting,
          id: "e2",
          service: {
            type: "ports",
            ports: [
              { proto: "tcp", from: 8008, to: 8009 },
              { proto: "udp", from: 5353, to: 5353 },
            ],
          },
          bidirectional: false,
        },
      ],
    });
    expect(screen.getByText("2 ports")).toBeInTheDocument();
    // one-directional renders the → glyph.
    expect(screen.getByText("→")).toBeInTheDocument();
  });

  it("creates a casting exception between two chosen endpoints", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard();
    await user.click(screen.getByTestId("exception-add"));

    const combos = screen.getAllByRole("combobox");
    await user.click(combos[0]);
    await user.click(
      await screen.findByRole("option", { name: "Device: Phone" }),
    );
    await user.click(combos[1]);
    await user.click(
      await screen.findByRole("option", { name: "Zone: Guest" }),
    );

    const submit = screen.getByTestId("exception-submit");
    expect(submit).toBeEnabled();
    await user.click(submit);
    expect(onCreateException).toHaveBeenCalledWith(
      {
        from: { kind: "device", id: "d1" },
        to: { kind: "zone", id: "z-guest" },
        service: { type: "preset", set: "casting" },
        bidirectional: true,
      },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("closes the form once the create succeeds", async () => {
    // The page owns the mutation, so the card only learns of success through
    // the `onSuccess` callback it hands down; without it the form stays open
    // over a save that already landed.
    onCreateException.mockImplementation(
      (_body: unknown, cbs?: { onSuccess?: () => void }) => cbs?.onSuccess?.(),
    );
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard();
    await user.click(screen.getByTestId("exception-add"));

    const combos = screen.getAllByRole("combobox");
    await user.click(combos[0]);
    await user.click(
      await screen.findByRole("option", { name: "Device: Phone" }),
    );
    await user.click(combos[1]);
    await user.click(
      await screen.findByRole("option", { name: "Zone: Guest" }),
    );
    await user.click(screen.getByTestId("exception-submit"));

    expect(screen.queryByTestId("exception-submit")).not.toBeInTheDocument();
  });

  it("creates a smart-home exception (issue #1098)", async () => {
    // The preset the casting one was silently failing to cover: it must reach
    // the daemon as a real backend preset, not as an expanded port list.
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard();
    await user.click(screen.getByTestId("exception-add"));
    const combos = screen.getAllByRole("combobox");
    await user.click(combos[0]);
    await user.click(
      await screen.findByRole("option", { name: "Device: Phone" }),
    );
    await user.click(combos[1]);
    await user.click(
      await screen.findByRole("option", { name: "Zone: Guest" }),
    );
    await user.click(screen.getByTestId("exception-service"));
    await user.click(await screen.findByRole("option", { name: /Smart home/ }));
    await user.click(screen.getByTestId("exception-submit"));
    expect(onCreateException).toHaveBeenCalledWith(
      expect.objectContaining({
        service: { type: "preset", set: "smart_home" },
        // The device->client leg is a fresh flow, so this must be bidirectional.
        bidirectional: true,
      }),
      expect.anything(),
    );
  });

  it("creates an exception for a curated service bundle (SSH)", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard();
    await user.click(screen.getByTestId("exception-add"));
    const combos = screen.getAllByRole("combobox");
    await user.click(combos[0]);
    await user.click(
      await screen.findByRole("option", { name: "Device: Phone" }),
    );
    await user.click(combos[1]);
    await user.click(
      await screen.findByRole("option", { name: "Zone: Guest" }),
    );
    await user.click(screen.getByTestId("exception-service"));
    await user.click(await screen.findByRole("option", { name: "SSH" }));
    await user.click(screen.getByTestId("exception-submit"));
    expect(onCreateException).toHaveBeenCalledWith(
      expect.objectContaining({
        service: { type: "ports", ports: [{ proto: "tcp", from: 22, to: 22 }] },
        bidirectional: true,
      }),
      expect.anything(),
    );
  });

  it("creates a custom-ports exception", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard();
    await user.click(screen.getByTestId("exception-add"));
    const combos = screen.getAllByRole("combobox");
    await user.click(combos[0]);
    await user.click(
      await screen.findByRole("option", { name: "Device: Phone" }),
    );
    await user.click(combos[1]);
    await user.click(
      await screen.findByRole("option", { name: "Zone: Guest" }),
    );
    await user.click(screen.getByTestId("exception-service"));
    await user.click(
      await screen.findByRole("option", { name: "Custom ports…" }),
    );
    // Submit stays disabled until a valid port is entered.
    expect(screen.getByTestId("exception-submit")).toBeDisabled();
    await user.type(screen.getByTestId("exception-port-from-0"), "8080");
    await user.click(screen.getByTestId("exception-submit"));
    expect(onCreateException).toHaveBeenCalledWith(
      expect.objectContaining({
        service: {
          type: "ports",
          ports: [{ proto: "tcp", from: 8080, to: 8080 }],
        },
      }),
      expect.anything(),
    );
  });

  it("labels a known service bundle by name in the list", () => {
    renderCard({
      exceptions: [
        {
          ...casting,
          id: "e3",
          service: {
            type: "ports",
            ports: [{ proto: "tcp", from: 22, to: 22 }],
          },
        },
      ],
    });
    expect(screen.getByText("SSH")).toBeInTheDocument();
  });

  it("shows an error and blocks submit for an out-of-range custom port", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard();
    await user.click(screen.getByTestId("exception-add"));
    await user.click(screen.getByTestId("exception-service"));
    await user.click(
      await screen.findByRole("option", { name: "Custom ports…" }),
    );
    // No valid port yet → inline error + disabled submit.
    expect(screen.getByTestId("exception-port-error")).toBeInTheDocument();
    expect(screen.getByTestId("exception-submit")).toBeDisabled();
    // 70000 is out of range → still invalid.
    await user.type(screen.getByTestId("exception-port-from-0"), "70000");
    expect(screen.getByTestId("exception-port-error")).toBeInTheDocument();
  });

  it("adds and removes custom port rows", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard();
    await user.click(screen.getByTestId("exception-add"));
    await user.click(screen.getByTestId("exception-service"));
    await user.click(
      await screen.findByRole("option", { name: "Custom ports…" }),
    );
    // Add a second row, fill both, then remove the first.
    await user.click(screen.getByTestId("exception-port-add"));
    await user.type(screen.getByTestId("exception-port-from-0"), "80");
    await user.type(screen.getByTestId("exception-port-to-1"), "443");
    await user.type(screen.getByTestId("exception-port-from-1"), "443");
    expect(screen.getByTestId("exception-port-from-1")).toBeInTheDocument();
    await user.click(screen.getByTestId("exception-port-remove-0"));
    // Row 0 removed → the old row 1 is now row 0.
    expect(
      screen.queryByTestId("exception-port-from-1"),
    ).not.toBeInTheDocument();
  });

  it("rejects picking the same endpoint for both sides", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard();
    await user.click(screen.getByTestId("exception-add"));

    const combos = screen.getAllByRole("combobox");
    await user.click(combos[0]);
    await user.click(
      await screen.findByRole("option", { name: "Zone: Guest" }),
    );
    await user.click(combos[1]);
    await user.click(
      await screen
        .findAllByRole("option", { name: "Zone: Guest" })
        .then((o) => o[0]),
    );

    expect(
      screen.getByText(/Pick two different endpoints/i),
    ).toBeInTheDocument();
    expect(screen.getByTestId("exception-submit")).toBeDisabled();
  });

  it("deletes an exception after confirming", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard({ exceptions: [casting] });
    await user.click(screen.getByTestId("exception-row-menu"));
    await user.click(await screen.findByTestId("exception-delete"));
    await user.click(await screen.findByTestId("confirm-dialog-confirm"));
    expect(onDeleteException).toHaveBeenCalledWith("e1");
  });
  it("shows why the daemon rejected the exception", async () => {
    // The daemon answers an invalid exception with a 400 carrying the reason.
    // With no error channel the card discards it and the save looks like it
    // did nothing, leaving the operator to read the browser console to learn
    // that a full-range service has to be device-to-device. The reason belongs
    // with the form that is still open behind it, so it goes when the form does.
    const user = userEvent.setup();
    renderCard({
      createError: new Error(
        "a full-range (all-ports) exception requires device-to-device endpoints",
      ),
    });
    await user.click(screen.getByTestId("exception-add"));

    expect(
      screen.getByText(/requires device-to-device endpoints/i),
    ).toBeInTheDocument();
  });

  it("clears the rejection before a reopened form is shown", async () => {
    // The mutation keeps its error until the next `mutate`, so a reopened form
    // would greet the operator with the last rejection before they have
    // submitted anything.
    const onResetCreateError = vi.fn();
    const user = userEvent.setup();
    renderCard({
      createError: new Error(
        "a full-range (all-ports) exception requires device-to-device endpoints",
      ),
      onResetCreateError,
    });

    await user.click(screen.getByTestId("exception-add"));

    expect(onResetCreateError).toHaveBeenCalled();
  });

  it("drops the rejection reason when the form is cancelled", async () => {
    const user = userEvent.setup();
    renderCard({
      createError: new Error(
        "a full-range (all-ports) exception requires device-to-device endpoints",
      ),
    });
    await user.click(screen.getByTestId("exception-add"));
    await user.click(screen.getByRole("button", { name: /cancel/i }));

    expect(
      screen.queryByText(/requires device-to-device endpoints/i),
    ).not.toBeInTheDocument();
  });
});
