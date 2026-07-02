/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { navigate, useAuthStore } = vi.hoisted(() => ({
  navigate: vi.fn(),
  useAuthStore: vi.fn(),
}));

let locationState: { from?: string } | null = null;

vi.mock("react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-router")>();
  return {
    ...actual,
    useNavigate: () => navigate,
    useLocation: () => ({
      pathname: "/login",
      search: "",
      hash: "",
      key: "default",
      state: locationState,
    }),
  };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useAuthStore,
    // Stub LoginForm: expose onSuccess via a button so we can drive the
    // post-login navigation branch.
    LoginForm: ({ onSuccess, rememberMe }: any) => (
      <div data-testid="login-form" data-remember={rememberMe}>
        <button onClick={() => onSuccess()}>submit</button>
      </div>
    ),
  };
});

import Login from "@/pages/Login";
import { renderWithProviders } from "../test-utils";

const login = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  locationState = null;
  useAuthStore.mockReturnValue({ login });
});

describe("Login", () => {
  it("renders the login form with rememberMe checkbox", () => {
    renderWithProviders(<Login />);
    const form = screen.getByTestId("login-form");
    expect(form).toBeInTheDocument();
    expect(form).toHaveAttribute("data-remember", "checkbox");
  });

  it("navigates to the dashboard when no from state is present", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Login />);
    await user.click(screen.getByRole("button", { name: "submit" }));
    expect(navigate).toHaveBeenCalledWith("/");
  });

  it("navigates back to the attempted deep-link on success", async () => {
    locationState = { from: "/devices/dev-1" };
    const user = userEvent.setup();
    renderWithProviders(<Login />);
    await user.click(screen.getByRole("button", { name: "submit" }));
    expect(navigate).toHaveBeenCalledWith("/devices/dev-1");
  });
});
