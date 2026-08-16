/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useOauthProviders, useConfigureOauthProvider, useClearOauthProvider } =
  vi.hoisted(() => ({
    useOauthProviders: vi.fn(),
    useConfigureOauthProvider: vi.fn(),
    useClearOauthProvider: vi.fn(),
  }));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useOauthProviders,
    useConfigureOauthProvider,
    useClearOauthProvider,
  };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: any) => <h1>{title}</h1>,
}));

vi.mock("@/components/compound/ConfirmDialog", () => ({
  ConfirmDialog: ({ open, title, onConfirm }: any) =>
    open ? (
      <div data-testid="confirm">
        <span>{title}</span>
        <button onClick={onConfirm}>confirm</button>
      </div>
    ) : null,
}));

import SignInMethods from "@/pages/SignInMethods";
import { renderWithProviders } from "../test-utils";

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

const REDIRECT = "https://home.example/api/auth/oauth/google/callback";

function provider(over: Partial<Record<string, unknown>> = {}) {
  return {
    provider: "google",
    client_id: "the-id",
    enabled: true,
    configured: true,
    redirect_uri: REDIRECT,
    ...over,
  };
}

describe("SignInMethods", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useOauthProviders.mockReturnValue({ data: [provider()], isLoading: false });
    useConfigureOauthProvider.mockReturnValue(mutation());
    useClearOauthProvider.mockReturnValue(mutation());
  });

  it("always shows the password as the unremovable floor", () => {
    renderWithProviders(<SignInMethods />);
    expect(screen.getByText("Wardnet password")).toBeInTheDocument();
    expect(screen.getByText("Always on")).toBeInTheDocument();
  });

  it("hands over the redirect URI to register", () => {
    // The screen's real job: every household registers this by hand, and a
    // single character of drift fails at the provider.
    renderWithProviders(<SignInMethods />);
    expect(screen.getByText(REDIRECT)).toBeInTheDocument();
  });

  it("explains the blocker when the box has no public address", () => {
    useOauthProviders.mockReturnValue({
      data: [provider({ redirect_uri: null })],
      isLoading: false,
    });
    renderWithProviders(<SignInMethods />);

    expect(screen.getByText(/no public address yet/)).toBeInTheDocument();
    // And the toggle stays out of reach — enabling could not possibly work.
    expect(screen.getByRole("switch")).toBeDisabled();
  });

  it("shows exactly one status pill", () => {
    // `enabled` is the stored flag and `configured` is "a secret is present",
    // so an on-but-unconfigured provider must not claim both at once.
    useOauthProviders.mockReturnValue({
      data: [provider({ enabled: true, configured: false })],
      isLoading: false,
    });
    renderWithProviders(<SignInMethods />);

    expect(screen.getByText("Not set up")).toBeInTheDocument();
    expect(screen.queryByText("On")).not.toBeInTheDocument();
  });

  it("resubmits the stored client id when toggling, so it is not erased", async () => {
    const configure = mutation();
    useConfigureOauthProvider.mockReturnValue(configure);
    useOauthProviders.mockReturnValue({
      data: [provider({ enabled: false })],
      isLoading: false,
    });
    renderWithProviders(<SignInMethods />);

    await userEvent.click(screen.getByRole("switch"));

    expect(configure.mutate).toHaveBeenCalledWith({
      provider: "google",
      body: { client_id: "the-id", client_secret: null, enabled: true },
    });
  });

  it("keeps the stored secret when the field is left blank", async () => {
    const configure = mutation();
    useConfigureOauthProvider.mockReturnValue(configure);
    renderWithProviders(<SignInMethods />);

    await userEvent.click(
      screen.getByRole("button", { name: "Update credentials" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    // The form cannot read the secret back, so blank must mean "keep", never
    // "erase".
    expect(configure.mutate).toHaveBeenCalledWith(
      {
        provider: "google",
        body: { client_id: "the-id", client_secret: null, enabled: true },
      },
      expect.anything(),
    );
  });

  it("sends a newly typed secret", async () => {
    const configure = mutation();
    useConfigureOauthProvider.mockReturnValue(configure);
    renderWithProviders(<SignInMethods />);

    await userEvent.click(
      screen.getByRole("button", { name: "Update credentials" }),
    );
    await userEvent.type(screen.getByLabelText("Client secret"), "s3cr3t");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(configure.mutate).toHaveBeenCalledWith(
      expect.objectContaining({
        body: expect.objectContaining({ client_secret: "s3cr3t" }),
      }),
      expect.anything(),
    );
  });

  it("clears a provider only after confirmation", async () => {
    const clear = mutation();
    useClearOauthProvider.mockReturnValue(clear);
    renderWithProviders(<SignInMethods />);

    await userEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(clear.mutate).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "confirm" }));
    expect(clear.mutate).toHaveBeenCalledWith("google");
  });

  it("offers Set up rather than Remove for an unconfigured provider", () => {
    useOauthProviders.mockReturnValue({
      data: [provider({ configured: false, client_id: null })],
      isLoading: false,
    });
    renderWithProviders(<SignInMethods />);

    expect(screen.getByRole("button", { name: "Set up" })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Remove" }),
    ).not.toBeInTheDocument();
  });

  it("surfaces a load failure", () => {
    useOauthProviders.mockReturnValue({
      data: undefined,
      isLoading: false,
      error: new Error("boom"),
    });
    renderWithProviders(<SignInMethods />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });
});
