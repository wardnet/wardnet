import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { renderWithProviders } from "../../test-utils";

describe("EmptyStatePlaceholder", () => {
  it("renders just the message when nothing else is provided", () => {
    renderWithProviders(<EmptyStatePlaceholder message="Nothing yet" />);
    expect(screen.getByText("Nothing yet")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("renders the optional hint", () => {
    renderWithProviders(
      <EmptyStatePlaceholder message="Nothing yet" hint="Add something" />,
    );
    expect(screen.getByText("Add something")).toBeInTheDocument();
  });

  it("renders and wires the action button", async () => {
    const user = userEvent.setup();
    const onAction = vi.fn();
    renderWithProviders(
      <EmptyStatePlaceholder
        message="Nothing yet"
        actionLabel="Create"
        onAction={onAction}
        actionTestId="empty-create"
      />,
    );
    const button = screen.getByTestId("empty-create");
    expect(button).toHaveTextContent("Create");
    await user.click(button);
    expect(onAction).toHaveBeenCalledOnce();
  });

  it("does not render the action button without an onAction handler", () => {
    renderWithProviders(
      <EmptyStatePlaceholder message="Nothing yet" actionLabel="Create" />,
    );
    expect(screen.queryByText("Create")).not.toBeInTheDocument();
  });

  it("renders a custom icon in place of the default", () => {
    renderWithProviders(
      <EmptyStatePlaceholder
        message="Nothing yet"
        icon={<span data-testid="custom-icon">icon</span>}
      />,
    );
    expect(screen.getByTestId("custom-icon")).toBeInTheDocument();
  });
});
