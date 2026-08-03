import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Blocklist } from "@wardnet/js";
import { BlocklistTable } from "@/components/compound/BlocklistTable";
import { renderWithProviders } from "../../test-utils";

function makeBlocklist(overrides: Partial<Blocklist> = {}): Blocklist {
  return {
    id: "bl-1",
    profile_id: "p-1",
    name: "StevenBlack",
    url: "https://example.com/hosts",
    enabled: true,
    entry_count: 12345,
    last_updated: "2026-01-01T00:00:00Z",
    cron_schedule: "0 3 * * *",
    last_error: null,
    last_error_at: null,
    consecutive_failures: 0,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("BlocklistTable", () => {
  it("shows the empty state and fires onAdd", async () => {
    const user = userEvent.setup();
    const onAdd = vi.fn();
    renderWithProviders(
      <BlocklistTable
        blocklists={[]}
        onRefresh={vi.fn()}
        onToggle={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onAdd={onAdd}
      />,
    );
    expect(screen.getByText("No blocklists configured")).toBeInTheDocument();
    await user.click(screen.getByTestId("blocklist-add"));
    expect(onAdd).toHaveBeenCalledOnce();
  });

  it("renders name, url, entry count, last error, and never-updated fallback", () => {
    renderWithProviders(
      <BlocklistTable
        blocklists={[
          makeBlocklist({
            id: "bl-a",
            name: "With error",
            last_error: "404 not found",
            last_updated: null,
            entry_count: 1000,
            enabled: false,
          }),
        ]}
        onRefresh={vi.fn()}
        onToggle={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onAdd={vi.fn()}
      />,
    );
    expect(screen.getByText("With error")).toBeInTheDocument();
    expect(screen.getByText("404 not found")).toBeInTheDocument();
    expect(screen.getByText("1,000")).toBeInTheDocument();
    expect(screen.getByText("Never")).toBeInTheDocument();
    expect(screen.getByText("Disabled")).toBeInTheDocument();
  });

  it("opens the edit sheet on row click", async () => {
    const user = userEvent.setup();
    const onEdit = vi.fn();
    renderWithProviders(
      <BlocklistTable
        blocklists={[makeBlocklist({ id: "bl-x", name: "Clickable" })]}
        onRefresh={vi.fn()}
        onToggle={vi.fn()}
        onEdit={onEdit}
        onDelete={vi.fn()}
        onAdd={vi.fn()}
      />,
    );
    await user.click(screen.getByText("Clickable"));
    expect(onEdit).toHaveBeenCalledWith(
      expect.objectContaining({ id: "bl-x" }),
    );
  });

  it("exposes toggle, refresh, and delete row actions", async () => {
    const user = userEvent.setup();
    const onToggle = vi.fn();
    const onRefresh = vi.fn();
    const onDelete = vi.fn();
    renderWithProviders(
      <BlocklistTable
        blocklists={[makeBlocklist({ id: "bl-9", enabled: true })]}
        onRefresh={onRefresh}
        onToggle={onToggle}
        onEdit={vi.fn()}
        onDelete={onDelete}
        onAdd={vi.fn()}
      />,
    );
    await user.click(screen.getByTestId("blocklist-row-menu"));
    await user.click(await screen.findByText("Disable"));
    expect(onToggle).toHaveBeenCalledWith(
      expect.objectContaining({ id: "bl-9" }),
    );

    await user.click(screen.getByTestId("blocklist-row-menu"));
    await user.click(await screen.findByTestId("blocklist-refresh"));
    expect(onRefresh).toHaveBeenCalledWith("bl-9");

    await user.click(screen.getByTestId("blocklist-row-menu"));
    await user.click(await screen.findByText("Delete"));
    expect(onDelete).toHaveBeenCalledWith("bl-9");
  });

  it("shows the updating label for the refreshing row", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <BlocklistTable
        blocklists={[makeBlocklist({ id: "bl-9" })]}
        onRefresh={vi.fn()}
        onToggle={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onAdd={vi.fn()}
        refreshingId="bl-9"
      />,
    );
    await user.click(screen.getByTestId("blocklist-row-menu"));
    expect(await screen.findByText("Updating…")).toBeInTheDocument();
  });
});
