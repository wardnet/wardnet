/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { navigate, useRedeemEnrolment, searchParams } = vi.hoisted(() => ({
  navigate: vi.fn(),
  useRedeemEnrolment: vi.fn(),
  searchParams: { current: new URLSearchParams() },
}));

vi.mock("react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-router")>();
  return {
    ...actual,
    useNavigate: () => navigate,
    useSearchParams: () => [searchParams.current, vi.fn()],
  };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useRedeemEnrolment };
});

import Redeem from "@/pages/Redeem";
import { renderWithProviders } from "../test-utils";

/** A `useMutation`-shaped handle whose `mutate` we can drive. */
function mutation(impl?: (vars: any, opts: any) => void) {
  return {
    mutate: vi.fn(impl ?? (() => {})),
    mutateAsync: vi.fn(),
    reset: vi.fn(),
    isPending: false,
    isError: false,
    error: null,
  };
}

describe("Redeem", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    searchParams.current = new URLSearchParams();
    useRedeemEnrolment.mockReturnValue(mutation());
  });

  it("pre-fills the code from the URL so a link can be sent instead of 32 characters", () => {
    searchParams.current = new URLSearchParams("token=demo-token");
    renderWithProviders(<Redeem />);

    expect(screen.getByTestId("redeem-token")).toHaveValue("demo-token");
  });

  it("still accepts a code typed by hand", async () => {
    // A token passed by message or read aloud has no link to click, so the
    // field must stay editable rather than being locked to the query string.
    const redeem = mutation();
    useRedeemEnrolment.mockReturnValue(redeem);
    renderWithProviders(<Redeem />);

    await userEvent.type(screen.getByTestId("redeem-token"), "typed-token");
    await userEvent.type(screen.getByTestId("redeem-password"), "a-password");
    await userEvent.type(screen.getByTestId("redeem-confirm"), "a-password");
    await userEvent.click(screen.getByTestId("redeem-submit"));

    expect(redeem.mutate).toHaveBeenCalledWith(
      { token: "typed-token", password: "a-password" },
      expect.anything(),
    );
  });

  it("refuses a mismatched confirmation without spending the token", async () => {
    // Checked client-side on purpose: a mistyped confirmation is a typing
    // mistake, and the invitation is single-use — spending it on one would
    // cost the person their only way in.
    const redeem = mutation();
    useRedeemEnrolment.mockReturnValue(redeem);
    renderWithProviders(<Redeem />);

    await userEvent.type(screen.getByTestId("redeem-token"), "tok");
    await userEvent.type(screen.getByTestId("redeem-password"), "a-password");
    await userEvent.type(screen.getByTestId("redeem-confirm"), "different");
    await userEvent.click(screen.getByTestId("redeem-submit"));

    expect(redeem.mutate).not.toHaveBeenCalled();
    expect(screen.getByTestId("redeem-error")).toHaveTextContent(
      "Those passwords do not match.",
    );
  });

  it("confirms success and offers the way on to sign-in", async () => {
    useRedeemEnrolment.mockReturnValue(
      mutation((_vars, opts) => opts.onSuccess({ display_name: "Bruno" })),
    );
    renderWithProviders(<Redeem />);

    await userEvent.type(screen.getByTestId("redeem-token"), "tok");
    await userEvent.type(screen.getByTestId("redeem-password"), "a-password");
    await userEvent.type(screen.getByTestId("redeem-confirm"), "a-password");
    await userEvent.click(screen.getByTestId("redeem-submit"));

    expect(screen.getByText(/Your password is set, Bruno/)).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Go to sign in" }),
    );
    expect(navigate).toHaveBeenCalledWith("/login");
  });

  it("gives one message for every kind of bad invitation", async () => {
    // Unknown, expired and already-redeemed are indistinguishable by design —
    // telling them apart would disclose which guesses were once real tokens.
    useRedeemEnrolment.mockReturnValue(
      mutation((_vars, opts) => opts.onError(new Error("unauthorized"))),
    );
    renderWithProviders(<Redeem />);

    await userEvent.type(screen.getByTestId("redeem-token"), "spent");
    await userEvent.type(screen.getByTestId("redeem-password"), "a-password");
    await userEvent.type(screen.getByTestId("redeem-confirm"), "a-password");
    await userEvent.click(screen.getByTestId("redeem-submit"));

    expect(screen.getByTestId("redeem-error")).toHaveTextContent(
      "That invitation is not valid",
    );
  });
});
