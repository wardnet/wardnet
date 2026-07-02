import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { AllowlistEntry } from "@wardnet/js";
import { AllowlistTable } from "@/components/compound/AllowlistTable";
import { renderWithProviders } from "../../test-utils";

function makeEntry(overrides: Partial<AllowlistEntry> = {}): AllowlistEntry {
  return {
    id: "al-1",
    profile_id: "p-1",
    domain: "example.com",
    reason: "trusted",
    created_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("AllowlistTable", () => {
  it("renders the empty state and fires onAdd from it", async () => {
    const user = userEvent.setup();
    const onAdd = vi.fn();
    renderWithProviders(
      <AllowlistTable entries={[]} onDelete={vi.fn()} onAdd={onAdd} />,
    );
    expect(screen.getByText("No allowlist entries")).toBeInTheDocument();
    await user.click(screen.getByTestId("allowlist-add"));
    expect(onAdd).toHaveBeenCalledOnce();
  });

  it("renders rows with domain and a dash for a missing reason", () => {
    renderWithProviders(
      <AllowlistTable
        entries={[
          makeEntry({ domain: "a.com", reason: "why" }),
          makeEntry({ id: "al-2", domain: "b.com", reason: null }),
        ]}
        onDelete={vi.fn()}
        onAdd={vi.fn()}
      />,
    );
    expect(screen.getByText("a.com")).toBeInTheDocument();
    expect(screen.getByText("why")).toBeInTheDocument();
    expect(screen.getByText("b.com")).toBeInTheDocument();
    expect(screen.getByText("—")).toBeInTheDocument();
  });

  it("removes an entry via the row overflow menu", async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn();
    renderWithProviders(
      <AllowlistTable
        entries={[makeEntry({ id: "al-9" })]}
        onDelete={onDelete}
        onAdd={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Row actions" }));
    await user.click(await screen.findByText("Remove"));
    expect(onDelete).toHaveBeenCalledWith("al-9");
  });

  it("fires onAdd from the toolbar CTA", async () => {
    const user = userEvent.setup();
    const onAdd = vi.fn();
    renderWithProviders(
      <AllowlistTable
        entries={[makeEntry()]}
        onDelete={vi.fn()}
        onAdd={onAdd}
      />,
    );
    await user.click(screen.getByTestId("allowlist-add"));
    expect(onAdd).toHaveBeenCalledOnce();
  });
});
