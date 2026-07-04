import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const navigate = vi.fn();
vi.mock("react-router", async (io) => {
  const actual = await io<typeof import("react-router")>();
  return { ...actual, useNavigate: () => navigate };
});

import StepDone from "@/pages/setup/StepDone";
import { renderWithProviders } from "../../test-utils";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("StepDone", () => {
  it("renders the all-set summary", () => {
    renderWithProviders(<StepDone />);
    expect(screen.getByText("All set")).toBeInTheDocument();
  });

  it("navigates to the dashboard on click", async () => {
    const user = userEvent.setup();
    renderWithProviders(<StepDone />);
    await user.click(screen.getByTestId("setup-go-dashboard"));
    expect(navigate).toHaveBeenCalledWith("/");
  });
});
