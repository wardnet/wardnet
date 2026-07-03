import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { CustomFilterRule } from "@wardnet/js";
import { FilterRuleTable } from "@/components/compound/FilterRuleTable";
import { renderWithProviders } from "../../test-utils";

function makeRule(overrides: Partial<CustomFilterRule> = {}): CustomFilterRule {
  return {
    id: "r-1",
    profile_id: "p-1",
    rule_text: "||ads.example.com^",
    enabled: true,
    comment: "block ads",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("FilterRuleTable", () => {
  it("shows the empty state and fires onAdd", async () => {
    const user = userEvent.setup();
    const onAdd = vi.fn();
    renderWithProviders(
      <FilterRuleTable
        rules={[]}
        onToggle={vi.fn()}
        onDelete={vi.fn()}
        onAdd={onAdd}
      />,
    );
    expect(screen.getByText("No custom filter rules")).toBeInTheDocument();
    await user.click(screen.getByTestId("rule-add"));
    expect(onAdd).toHaveBeenCalledOnce();
  });

  it("renders rule text, optional comment, and status", () => {
    renderWithProviders(
      <FilterRuleTable
        rules={[
          makeRule({ rule_text: "rule-a", comment: "note-a", enabled: true }),
          makeRule({
            id: "r-2",
            rule_text: "rule-b",
            comment: null,
            enabled: false,
          }),
        ]}
        onToggle={vi.fn()}
        onDelete={vi.fn()}
        onAdd={vi.fn()}
      />,
    );
    expect(screen.getByText("rule-a")).toBeInTheDocument();
    expect(screen.getByText("note-a")).toBeInTheDocument();
    expect(screen.getByText("rule-b")).toBeInTheDocument();
    expect(screen.getByText("Enabled")).toBeInTheDocument();
    expect(screen.getByText("Disabled")).toBeInTheDocument();
  });

  it("toggles and deletes via the row overflow menu", async () => {
    const user = userEvent.setup();
    const onToggle = vi.fn();
    const onDelete = vi.fn();
    renderWithProviders(
      <FilterRuleTable
        rules={[makeRule({ id: "r-9", enabled: true })]}
        onToggle={onToggle}
        onDelete={onDelete}
        onAdd={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Row actions" }));
    await user.click(await screen.findByText("Disable"));
    expect(onToggle).toHaveBeenCalledWith("r-9", false);

    await user.click(screen.getByRole("button", { name: "Row actions" }));
    await user.click(await screen.findByText("Delete"));
    expect(onDelete).toHaveBeenCalledWith("r-9");
  });

  it("labels the toggle action Enable for a disabled rule", async () => {
    const user = userEvent.setup();
    const onToggle = vi.fn();
    renderWithProviders(
      <FilterRuleTable
        rules={[makeRule({ id: "r-3", enabled: false })]}
        onToggle={onToggle}
        onDelete={vi.fn()}
        onAdd={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Row actions" }));
    await user.click(await screen.findByText("Enable"));
    expect(onToggle).toHaveBeenCalledWith("r-3", true);
  });
});
