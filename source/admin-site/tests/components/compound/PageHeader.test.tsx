import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { PageHeader } from "@/components/compound/PageHeader";
import { renderWithProviders } from "../../test-utils";

describe("PageHeader", () => {
  it("renders the title", () => {
    renderWithProviders(<PageHeader title="Devices" />);
    expect(screen.getByTestId("page-title")).toHaveTextContent("Devices");
  });

  it("renders the description when provided", () => {
    renderWithProviders(
      <PageHeader title="Devices" description="Manage your devices" />,
    );
    expect(screen.getByText("Manage your devices")).toBeInTheDocument();
  });

  it("omits the description paragraph when not provided", () => {
    renderWithProviders(<PageHeader title="Devices" />);
    expect(screen.queryByText("Manage your devices")).not.toBeInTheDocument();
  });

  it("renders actions when provided", () => {
    renderWithProviders(
      <PageHeader title="Devices" actions={<button>Add</button>} />,
    );
    expect(screen.getByRole("button", { name: "Add" })).toBeInTheDocument();
  });
});
