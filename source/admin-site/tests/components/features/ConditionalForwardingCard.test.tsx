import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ConditionalForwardingCard } from "@/components/features/ConditionalForwardingCard";
import { renderWithProviders } from "../../test-utils";
import type { ConditionalForwardingRule } from "@wardnet/js";

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

const onCreateRule = vi.fn();
const onUpdateRule = vi.fn();
const onDeleteRule = vi.fn();

const rules = [
  {
    id: "r1",
    domain: "corp.internal",
    upstream: "10.0.0.1",
    enabled: true,
  },
] as ConditionalForwardingRule[];

function renderCard() {
  return renderWithProviders(
    <ConditionalForwardingCard
      rules={rules}
      isSaving={false}
      updatePending={false}
      onCreateRule={onCreateRule}
      onUpdateRule={onUpdateRule}
      onDeleteRule={onDeleteRule}
    />,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("ConditionalForwardingCard", () => {
  it("renders existing rules", () => {
    renderCard();
    expect(screen.getByText("corp.internal")).toBeInTheDocument();
    expect(screen.getByText("10.0.0.1")).toBeInTheDocument();
  });

  it("toggling a rule calls the update callback", async () => {
    const user = userEvent.setup();
    renderCard();
    await user.click(screen.getByLabelText("Toggle rule for corp.internal"));
    expect(onUpdateRule).toHaveBeenCalledWith({
      id: "r1",
      body: { enabled: false },
    });
  });

  it("creates a rule through the add form", async () => {
    const user = userEvent.setup();
    renderCard();
    await user.click(screen.getByTestId("fwd-add"));
    await user.type(screen.getByTestId("fwd-domain"), "lab.internal");
    await user.type(screen.getByTestId("fwd-upstream"), "10.1.1.1");
    await user.click(screen.getByTestId("fwd-submit"));
    expect(onCreateRule).toHaveBeenCalledWith(
      { domain: "lab.internal", upstream: "10.1.1.1", enabled: true },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("cancelling closes the form", async () => {
    const user = userEvent.setup();
    renderCard();
    await user.click(screen.getByTestId("fwd-add"));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("fwd-domain")).not.toBeInTheDocument();
  });

  it("edits a rule via the row menu", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard();
    await user.click(screen.getByTestId("fwd-row-menu"));
    await user.click(await screen.findByTestId("fwd-edit"));
    const domain = screen.getByTestId("fwd-domain") as HTMLInputElement;
    expect(domain.value).toBe("corp.internal");
    await user.clear(domain);
    await user.type(domain, "corp.local");
    await user.click(screen.getByTestId("fwd-submit"));
    expect(onUpdateRule).toHaveBeenCalledWith(
      { id: "r1", body: { domain: "corp.local", upstream: "10.0.0.1" } },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("deletes a rule after confirming", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderCard();
    await user.click(screen.getByTestId("fwd-row-menu"));
    await user.click(await screen.findByTestId("fwd-delete"));
    await user.click(await screen.findByTestId("confirm-dialog-confirm"));
    expect(onDeleteRule).toHaveBeenCalledWith("r1");
  });
});
