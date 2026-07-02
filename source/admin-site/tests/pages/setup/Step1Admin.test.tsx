import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WardnetApiError } from "@wardnet/js";

const { useSetup, useAdvanceWizard, useAuth } = vi.hoisted(() => ({
  useSetup: vi.fn(),
  useAdvanceWizard: vi.fn(),
  useAuth: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useSetup, useAdvanceWizard, useAuth };
});

import Step1Admin from "@/pages/setup/Step1Admin";
import { renderWithProviders } from "../../test-utils";

const setupMutate = vi.fn();
const advanceMutate = vi.fn();
const login = vi.fn();

function apiError(status: number, message = "boom"): WardnetApiError {
  return new WardnetApiError(status, "err", { error: message });
}

async function fillValidForm(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByTestId("setup-username"), "admin");
  await user.type(screen.getByTestId("setup-password"), "password123");
  await user.type(screen.getByTestId("setup-confirm-password"), "password123");
}

beforeEach(() => {
  vi.clearAllMocks();
  setupMutate.mockResolvedValue(undefined);
  advanceMutate.mockResolvedValue(undefined);
  login.mockResolvedValue(undefined);
  useSetup.mockReturnValue({ mutateAsync: setupMutate, isPending: false });
  useAdvanceWizard.mockReturnValue({
    mutateAsync: advanceMutate,
    isPending: false,
  });
  useAuth.mockReturnValue({ login });
});

describe("Step1Admin", () => {
  it("renders the create-account form", () => {
    renderWithProviders(<Step1Admin />);
    expect(screen.getByText("Create admin account")).toBeInTheDocument();
    expect(screen.getByTestId("setup-admin-submit")).toHaveTextContent(
      "Create account",
    );
  });

  it("shows pending label and disables submit while creating", () => {
    useSetup.mockReturnValue({ mutateAsync: setupMutate, isPending: true });
    renderWithProviders(<Step1Admin />);
    const btn = screen.getByTestId("setup-admin-submit");
    expect(btn).toBeDisabled();
    expect(btn).toHaveTextContent("Creating account…");
  });

  it("submits: creates admin, logs in, advances to network", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Step1Admin />);
    await fillValidForm(user);
    await user.click(screen.getByTestId("setup-admin-submit"));

    await waitFor(() =>
      expect(setupMutate).toHaveBeenCalledWith({
        username: "admin",
        password: "password123",
      }),
    );
    expect(login).toHaveBeenCalledWith("admin", "password123");
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "network" });
  });

  it("handles a 409 by logging in with the typed credentials (no error)", async () => {
    setupMutate.mockRejectedValueOnce(apiError(409));
    const user = userEvent.setup();
    renderWithProviders(<Step1Admin />);
    await fillValidForm(user);
    await user.click(screen.getByTestId("setup-admin-submit"));

    await waitFor(() =>
      expect(login).toHaveBeenCalledWith("admin", "password123"),
    );
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows a credentials-mismatch error when the 409 re-login fails", async () => {
    setupMutate.mockRejectedValueOnce(apiError(409));
    login.mockRejectedValueOnce(new Error("nope"));
    const user = userEvent.setup();
    renderWithProviders(<Step1Admin />);
    await fillValidForm(user);
    await user.click(screen.getByTestId("setup-admin-submit"));

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        /credentials don't match/i,
      ),
    );
  });

  it("surfaces a non-409 API error message", async () => {
    setupMutate.mockRejectedValueOnce(apiError(400, "bad password"));
    const user = userEvent.setup();
    renderWithProviders(<Step1Admin />);
    await fillValidForm(user);
    await user.click(screen.getByTestId("setup-admin-submit"));

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("bad password"),
    );
  });

  it("surfaces a generic connection error for non-API failures", async () => {
    setupMutate.mockRejectedValueOnce(new Error("network"));
    const user = userEvent.setup();
    renderWithProviders(<Step1Admin />);
    await fillValidForm(user);
    await user.click(screen.getByTestId("setup-admin-submit"));

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        /Unable to connect to daemon/i,
      ),
    );
  });
});
