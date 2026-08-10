import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { ConnectionBanner } from "@/components/compound/ConnectionBanner";
import { renderWithProviders } from "../../test-utils";

describe("ConnectionBanner", () => {
  it("renders nothing while the status is unknown", () => {
    const { container } = renderWithProviders(
      <ConnectionBanner reachable={undefined} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when the daemon is reachable", () => {
    const { container } = renderWithProviders(<ConnectionBanner reachable />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the alert banner when the daemon is unreachable", () => {
    renderWithProviders(<ConnectionBanner reachable={false} />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(
      screen.getByText(/Unable to connect to the Wardnet daemon/),
    ).toBeInTheDocument();
  });
});
