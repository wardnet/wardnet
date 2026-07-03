import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { navigate } = vi.hoisted(() => ({ navigate: vi.fn() }));

vi.mock("react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-router")>();
  return { ...actual, useNavigate: () => navigate };
});

import NotFound from "@/pages/NotFound";
import { renderWithProviders } from "../test-utils";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("NotFound", () => {
  it("renders the 404 empty block with heading and copy", () => {
    renderWithProviders(<NotFound />);
    expect(screen.getByTestId("notfound-page")).toBeInTheDocument();
    expect(screen.getByText("404")).toBeInTheDocument();
    expect(screen.getByText("Page not found")).toBeInTheDocument();
    expect(
      screen.getByText(/doesn't exist or has been moved/),
    ).toBeInTheDocument();
  });

  it("navigates to the dashboard when the button is clicked", async () => {
    const user = userEvent.setup();
    renderWithProviders(<NotFound />);
    await user.click(screen.getByTestId("notfound-home"));
    expect(navigate).toHaveBeenCalledWith("/");
  });
});
