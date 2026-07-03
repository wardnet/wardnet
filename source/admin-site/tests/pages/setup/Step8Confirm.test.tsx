import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const navigate = vi.fn();
vi.mock("react-router", async (io) => {
  const actual = await io<typeof import("react-router")>();
  return { ...actual, useNavigate: () => navigate };
});

import Step8Confirm from "@/pages/setup/Step8Confirm";
import { renderWithProviders } from "../../test-utils";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("Step8Confirm", () => {
  it("renders the all-set summary", () => {
    renderWithProviders(<Step8Confirm />);
    expect(screen.getByText("All set")).toBeInTheDocument();
  });

  it("navigates to the dashboard on click", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Step8Confirm />);
    await user.click(screen.getByTestId("setup-go-dashboard"));
    expect(navigate).toHaveBeenCalledWith("/");
  });
});
