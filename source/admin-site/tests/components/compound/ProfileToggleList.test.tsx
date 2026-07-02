import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { DnsFilterProfile } from "@wardnet/js";
import { ProfileToggleList } from "@/components/compound/ProfileToggleList";
import { renderWithProviders } from "../../test-utils";

function makeProfile(
  overrides: Partial<DnsFilterProfile> = {},
): DnsFilterProfile {
  return {
    id: "prof-1",
    name: "Ads",
    description: null,
    builtin: false,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("ProfileToggleList", () => {
  it("renders the default empty message when there are no profiles", () => {
    renderWithProviders(
      <ProfileToggleList profiles={[]} selectedIds={[]} onChange={vi.fn()} />,
    );
    expect(screen.getByText("No profiles defined.")).toBeInTheDocument();
  });

  it("renders a custom empty message", () => {
    renderWithProviders(
      <ProfileToggleList
        profiles={[]}
        selectedIds={[]}
        onChange={vi.fn()}
        emptyMessage="Nothing here"
      />,
    );
    expect(screen.getByText("Nothing here")).toBeInTheDocument();
  });

  it("renders rows with a builtin pill and reflects selection", () => {
    renderWithProviders(
      <ProfileToggleList
        profiles={[
          makeProfile({ id: "a", name: "Alpha", builtin: true }),
          makeProfile({ id: "b", name: "Beta", builtin: false }),
        ]}
        selectedIds={["a"]}
        onChange={vi.fn()}
        ariaLabelledBy="group-label"
      />,
    );
    expect(screen.getByText("Alpha")).toBeInTheDocument();
    expect(screen.getByText("Builtin")).toBeInTheDocument();
    expect(screen.getByRole("group")).toHaveAttribute(
      "aria-labelledby",
      "group-label",
    );
  });

  it("adds an id when toggling an unselected row on", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(
      <ProfileToggleList
        profiles={[makeProfile({ id: "a", name: "Alpha" })]}
        selectedIds={[]}
        onChange={onChange}
      />,
    );
    await user.click(screen.getByRole("switch", { name: "Alpha" }));
    expect(onChange).toHaveBeenCalledWith(["a"]);
  });

  it("removes an id when toggling a selected row off", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(
      <ProfileToggleList
        profiles={[
          makeProfile({ id: "a", name: "Alpha" }),
          makeProfile({ id: "b", name: "Beta" }),
        ]}
        selectedIds={["a", "b"]}
        onChange={onChange}
      />,
    );
    await user.click(screen.getByRole("switch", { name: "Alpha" }));
    expect(onChange).toHaveBeenCalledWith(["b"]);
  });

  it("disables every switch when disabled", () => {
    renderWithProviders(
      <ProfileToggleList
        profiles={[makeProfile({ id: "a", name: "Alpha" })]}
        selectedIds={[]}
        onChange={vi.fn()}
        disabled
      />,
    );
    expect(screen.getByRole("switch", { name: "Alpha" })).toBeDisabled();
  });
});
