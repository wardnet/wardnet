import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";

const { userService } = vi.hoisted(() => ({
  userService: {
    availableMethods: vi.fn(),
    startOauth: vi.fn(),
  },
}));
vi.mock("../../src/lib/sdk", () => ({ userService }));

import { OauthSignInButtons } from "../../src/components/OauthSignInButtons";

function provider(name: string, enabled: boolean) {
  // The public sign-in contract is exactly this shape — no client id and no
  // redirect URI, both of which are admin-only.
  return { provider: name, enabled, configured: enabled };
}

function renderButtons(props: Parameters<typeof OauthSignInButtons>[0]) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <OauthSignInButtons {...props} />
    </QueryClientProvider>,
  );
}

describe("OauthSignInButtons", () => {
  const assign = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    Object.defineProperty(window, "location", {
      value: { assign, origin: "https://home.example" },
      configurable: true,
    });
  });

  afterEach(() => vi.restoreAllMocks());

  it("renders nothing when no provider is enabled", async () => {
    // The common case: federated sign-in is opt-in and needs a public
    // hostname. A button that cannot complete a ceremony is worse than none,
    // because the failure happens at the provider where nobody can act on it.
    userService.availableMethods.mockResolvedValue({
      password: true,
      providers: [provider("google", false), provider("github", false)],
    });

    const { container } = renderButtons({ returnTo: "admin" });

    await waitFor(() =>
      expect(userService.availableMethods).toHaveBeenCalled(),
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("renders only the enabled providers", async () => {
    userService.availableMethods.mockResolvedValue({
      password: true,
      providers: [provider("google", true), provider("github", false)],
    });

    renderButtons({ returnTo: "admin" });

    expect(
      await screen.findByTestId("oauth-signin-google"),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("oauth-signin-github")).not.toBeInTheDocument();
  });

  it("starts the ceremony with the caller's intent and navigates to the provider", async () => {
    userService.availableMethods.mockResolvedValue({
      password: true,
      providers: [provider("google", true)],
    });
    userService.startOauth.mockResolvedValue(
      "https://accounts.google.com/o/oauth2/v2/auth?x=1",
    );

    renderButtons({ returnTo: "admin_app", rememberMe: true });
    fireEvent.click(await screen.findByTestId("oauth-signin-google"));

    // `returnTo` and `rememberMe` must reach the *start* call: they are parked
    // on the ceremony because the provider's callback carries no body, and
    // there is deliberately no endpoint that raises `remember_me` afterwards.
    await waitFor(() =>
      expect(userService.startOauth).toHaveBeenCalledWith("google", {
        returnTo: "admin_app",
        rememberMe: true,
      }),
    );

    // A full navigation, not a router push — the destination is a foreign
    // origin.
    await waitFor(() =>
      expect(assign).toHaveBeenCalledWith(
        "https://accounts.google.com/o/oauth2/v2/auth?x=1",
      ),
    );
  });

  it("does not navigate when the ceremony cannot be started", async () => {
    userService.availableMethods.mockResolvedValue({
      password: true,
      providers: [provider("google", true)],
    });
    userService.startOauth.mockRejectedValue(new Error("provider unreachable"));

    renderButtons({ returnTo: "admin" });
    fireEvent.click(await screen.findByTestId("oauth-signin-google"));

    await waitFor(() => expect(userService.startOauth).toHaveBeenCalled());
    // Returning the URL rather than following a redirect is what makes this
    // representable at all: a failure surfaces here instead of as an opaque
    // cross-origin fetch.
    expect(assign).not.toHaveBeenCalled();
  });
});
