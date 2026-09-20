/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  navigate,
  useAuthStore,
  searchParams,
  oauthButtonProps,
  locationState,
} = vi.hoisted(() => ({
  navigate: vi.fn(),
  useAuthStore: vi.fn(),
  searchParams: { current: new URLSearchParams() },
  oauthButtonProps: { current: null as any },
  locationState: { current: null as { from?: string } | null },
}));

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
      state: locationState.current,
    }),
    useSearchParams: () => [searchParams.current, vi.fn()],
  };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useAuthStore,
    // Captured rather than rendered, so the test can assert what intent the
    // page hands to a federated sign-in.
    OauthSignInButtons: (props: any) => {
      oauthButtonProps.current = props;
      return <div data-testid="oauth-buttons" />;
    },
  };
});

import Login from "@/pages/Login";
import { renderWithProviders } from "../test-utils";

describe("Login", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    searchParams.current = new URLSearchParams();
    oauthButtonProps.current = null;
    locationState.current = null;
    useAuthStore.mockReturnValue({
      login: vi.fn().mockResolvedValue(undefined),
    });
  });

  it("renders the login form with the rememberMe checkbox", () => {
    renderWithProviders(<Login />);
    expect(screen.getByRole("checkbox")).toBeInTheDocument();
  });

  it("navigates to the dashboard when no from state is present", async () => {
    renderWithProviders(<Login />);
    await userEvent.type(screen.getByTestId("login-username"), "admin");
    await userEvent.type(screen.getByTestId("login-password"), "pw");
    await userEvent.click(screen.getByTestId("login-submit"));
    expect(navigate).toHaveBeenCalledWith("/");
  });

  it("navigates back to the attempted deep-link on success", async () => {
    locationState.current = { from: "/devices/dev-1" };
    renderWithProviders(<Login />);
    await userEvent.type(screen.getByTestId("login-username"), "admin");
    await userEvent.type(screen.getByTestId("login-password"), "pw");
    await userEvent.click(screen.getByTestId("login-submit"));
    expect(navigate).toHaveBeenCalledWith("/devices/dev-1");
  });

  it("says what went wrong when a federated sign-in failed", () => {
    // The callback's only channel is this query parameter. Without a reader, a
    // declined consent and a broken box are the same silent bounce.
    searchParams.current = new URLSearchParams("oauth_error=access_denied");
    renderWithProviders(<Login />);

    expect(screen.getByTestId("oauth-error")).toHaveTextContent(
      /nobody has linked it to a Wardnet user yet/,
    );
  });

  it("still says something for a code it does not recognise", () => {
    searchParams.current = new URLSearchParams("oauth_error=something_new");
    renderWithProviders(<Login />);

    expect(screen.getByTestId("oauth-error")).toHaveTextContent(
      "That sign-in attempt did not work. Please try again.",
    );
  });

  it("does not reach the prototype chain for a hostile code", () => {
    // `?oauth_error=constructor` against a plain object would resolve to a
    // function and blow up the render; the lookup is a Map for that reason.
    searchParams.current = new URLSearchParams("oauth_error=constructor");
    renderWithProviders(<Login />);

    expect(screen.getByTestId("oauth-error")).toHaveTextContent(
      "That sign-in attempt did not work. Please try again.",
    );
  });

  it("shows no error banner on an ordinary visit", () => {
    renderWithProviders(<Login />);
    expect(screen.queryByTestId("oauth-error")).not.toBeInTheDocument();
  });

  it("carries the Remember me choice into a federated sign-in", async () => {
    renderWithProviders(<Login />);

    // Off by default…
    expect(oauthButtonProps.current.rememberMe).toBe(false);

    await userEvent.click(screen.getByRole("checkbox"));

    // …and ticking it must reach the ceremony's *start*, because `remember_me`
    // is parked there and there is deliberately no endpoint that raises it
    // afterwards. Losing it here would silently give federated users a short
    // session they cannot upgrade.
    expect(oauthButtonProps.current.rememberMe).toBe(true);
  });
});
