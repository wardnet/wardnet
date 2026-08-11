import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { DeviceRuleRequest } from "@wardnet/js";

import Settings from "../../src/pages/Settings";
import { makeDevice, renderWithProviders } from "../test-utils";
import { FakeResizeObserver } from "../helpers/resizeObserver";

const {
  useMyDevice,
  useMyRuleRequests,
  useSetMyCaptureEnabled,
  usePushNotifications,
  usePrivateDnsMe,
} = vi.hoisted(() => ({
  useMyDevice: vi.fn(),
  useMyRuleRequests: vi.fn(),
  useSetMyCaptureEnabled: vi.fn(),
  usePushNotifications: vi.fn(),
  usePrivateDnsMe: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useMyDevice,
    useMyRuleRequests,
    useSetMyCaptureEnabled,
    usePushNotifications,
    usePrivateDnsMe,
  };
});

const captureMutate = vi.fn();

function req(overrides: Partial<DeviceRuleRequest> = {}): DeviceRuleRequest {
  return {
    id: "r1",
    device_id: "dev-1",
    kind: "block",
    domain: "ads.example.com",
    reason: null,
    status: "pending",
    created_at: "2026-07-01T00:00:00Z",
    decided_at: null,
    decided_by: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useMyRuleRequests.mockReturnValue({ data: [], isLoading: false });
  useSetMyCaptureEnabled.mockReturnValue({
    mutate: captureMutate,
    isPending: false,
    isError: false,
    error: null,
  });
  usePushNotifications.mockReturnValue({
    state: "prompt",
    isBusy: false,
    subscribe: vi.fn(),
    unsubscribe: vi.fn(),
  });
  // Default: feature off, so the Private DNS card sits in its quiet state and
  // doesn't add a second "Loading…" to the other cards' assertions.
  usePrivateDnsMe.mockReturnValue({
    data: { enabled: false, granted: false, hostname: null },
    isLoading: false,
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Settings page", () => {
  it("shows a loading state", () => {
    useMyDevice.mockReturnValue({ data: undefined, isLoading: true });
    renderWithProviders(<Settings />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("shows the not-detected fallback when there is no device", () => {
    useMyDevice.mockReturnValue({
      data: { device: null },
      isLoading: false,
    });
    renderWithProviders(<Settings />);
    expect(screen.getByText("Device not detected")).toBeInTheDocument();
  });

  it("renders the capture toggle reflecting the device flag and retention caps", () => {
    useMyDevice.mockReturnValue({
      data: {
        device: makeDevice({
          dns_capture_enabled: true,
          dns_capture_cap_count: 10000,
          dns_capture_cap_days: 1,
        }),
      },
      isLoading: false,
    });
    renderWithProviders(<Settings />);
    expect(screen.getByTestId("capture-toggle")).toBeChecked();
    expect(screen.getByText("10,000")).toBeInTheDocument();
    // Singular "day" branch when the cap is 1.
    expect(screen.getByText(/day on the gateway/)).toBeInTheDocument();
  });

  it("toggles capture off via the mutation", async () => {
    useMyDevice.mockReturnValue({
      data: { device: makeDevice({ dns_capture_enabled: true }) },
      isLoading: false,
    });
    renderWithProviders(<Settings />);
    await userEvent.click(screen.getByTestId("capture-toggle"));
    expect(captureMutate).toHaveBeenCalledWith(false);
  });

  it("shows an error alert when the capture mutation fails", () => {
    useSetMyCaptureEnabled.mockReturnValue({
      mutate: captureMutate,
      isPending: false,
      isError: true,
      error: new Error("nope"),
    });
    useMyDevice.mockReturnValue({
      data: { device: makeDevice() },
      isLoading: false,
    });
    renderWithProviders(<Settings />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("lists my rule requests when present", () => {
    useMyDevice.mockReturnValue({
      data: { device: makeDevice() },
      isLoading: false,
    });
    useMyRuleRequests.mockReturnValue({
      data: [
        req({ domain: "block.me", kind: "block" }),
        req({ id: "r2", domain: "allow.me", kind: "allow" }),
      ],
      isLoading: false,
    });
    renderWithProviders(<Settings />);
    const list = screen.getByTestId("my-requests");
    expect(list).toHaveTextContent("block.me");
    expect(list).toHaveTextContent("Block request");
    expect(list).toHaveTextContent("allow.me");
    expect(list).toHaveTextContent("Allow request");
  });

  it("hides the requests card when there are none", () => {
    useMyDevice.mockReturnValue({
      data: { device: makeDevice() },
      isLoading: false,
    });
    renderWithProviders(<Settings />);
    expect(screen.queryByTestId("my-requests")).not.toBeInTheDocument();
  });

  describe("notifications card", () => {
    beforeEach(() => {
      useMyDevice.mockReturnValue({
        data: { device: makeDevice() },
        isLoading: false,
      });
    });

    it("subscribes from the toggle and explains what will be sent", async () => {
      const subscribe = vi.fn();
      usePushNotifications.mockReturnValue({
        state: "prompt",
        isBusy: false,
        subscribe,
        unsubscribe: vi.fn(),
      });
      renderWithProviders(<Settings />);
      expect(
        screen.getByText(/locks or changes your device's routing/),
      ).toBeInTheDocument();
      await userEvent.click(screen.getByTestId("notifications-toggle"));
      expect(subscribe).toHaveBeenCalledOnce();
    });

    it("unsubscribes when the toggle is on and clicked", async () => {
      const unsubscribe = vi.fn();
      usePushNotifications.mockReturnValue({
        state: "subscribed",
        isBusy: false,
        subscribe: vi.fn(),
        unsubscribe,
      });
      renderWithProviders(<Settings />);
      const toggle = screen.getByTestId("notifications-toggle");
      expect(toggle).toBeChecked();
      await userEvent.click(toggle);
      expect(unsubscribe).toHaveBeenCalledOnce();
    });

    it("disables the toggle and explains when push is unsupported", () => {
      usePushNotifications.mockReturnValue({
        state: "unsupported",
        isBusy: false,
        subscribe: vi.fn(),
        unsubscribe: vi.fn(),
      });
      renderWithProviders(<Settings />);
      expect(screen.getByTestId("notifications-toggle")).toBeDisabled();
      expect(
        screen.getByText("Notifications are not supported in this browser."),
      ).toBeInTheDocument();
    });

    it("tells iOS browser-tab users to install the app when push is unsupported", () => {
      usePushNotifications.mockReturnValue({
        state: "unsupported",
        isBusy: false,
        subscribe: vi.fn(),
        unsubscribe: vi.fn(),
      });
      const originalUa = navigator.userAgent;
      Object.defineProperty(navigator, "userAgent", {
        value:
          "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15",
        configurable: true,
      });
      try {
        renderWithProviders(<Settings />);
        expect(
          screen.getByText(
            "Install the app to your Home Screen to enable notifications.",
          ),
        ).toBeInTheDocument();
      } finally {
        Object.defineProperty(navigator, "userAgent", {
          value: originalUa,
          configurable: true,
        });
      }
    });

    it("disables the toggle and explains when notifications are blocked", () => {
      usePushNotifications.mockReturnValue({
        state: "denied",
        isBusy: false,
        subscribe: vi.fn(),
        unsubscribe: vi.fn(),
      });
      renderWithProviders(<Settings />);
      expect(screen.getByTestId("notifications-toggle")).toBeDisabled();
      expect(
        screen.getByText("Notifications are blocked in your browser settings."),
      ).toBeInTheDocument();
    });

    it("disables the toggle while a subscribe/unsubscribe is in flight", () => {
      usePushNotifications.mockReturnValue({
        state: "prompt",
        isBusy: true,
        subscribe: vi.fn(),
        unsubscribe: vi.fn(),
      });
      renderWithProviders(<Settings />);
      expect(screen.getByTestId("notifications-toggle")).toBeDisabled();
    });
  });

  // Private DNS card (#916). Grants are admin-only in v1, so the two ungranted
  // states are informational — the member is told who to ask, not shown a
  // broken control.
  describe("private dns card", () => {
    beforeEach(() => {
      useMyDevice.mockReturnValue({
        data: { device: makeDevice() },
        isLoading: false,
      });
    });

    it("shows a loading state while this device's grant is fetched", () => {
      usePrivateDnsMe.mockReturnValue({ data: undefined, isLoading: true });
      renderWithProviders(<Settings />);
      const card = screen.getByTestId("private-dns-card");
      expect(card).toHaveTextContent("Loading…");
    });

    it("reports a fetch failure without asserting anything about the network", () => {
      usePrivateDnsMe.mockReturnValue({
        data: undefined,
        isLoading: false,
        isError: true,
      });
      renderWithProviders(<Settings />);
      // A local fetch failure says nothing about whether the admin enabled the
      // feature — claiming otherwise would send the member to their admin over
      // a transient error.
      expect(
        screen.getByText(/Couldn't check this device's Private DNS status/),
      ).toBeInTheDocument();
      expect(
        screen.queryByText(/isn't enabled on your network/),
      ).not.toBeInTheDocument();
    });

    it("keeps showing cached instructions when a refetch fails", () => {
      // React Query retains the last good data across a failed refetch, and
      // this hook refetches on focus — off-LAN (the roaming case the feature
      // exists for) that would otherwise wipe working setup steps every time
      // the member foregrounds the app.
      usePrivateDnsMe.mockReturnValue({
        data: {
          enabled: true,
          granted: true,
          hostname: "tok.abc.my.wardnet.services",
        },
        isLoading: false,
        isError: true,
      });
      renderWithProviders(<Settings />);
      expect(screen.getByTestId("private-dns-hostname")).toHaveTextContent(
        "tok.abc.my.wardnet.services",
      );
      expect(
        screen.queryByText(/Couldn't check this device's Private DNS status/),
      ).not.toBeInTheDocument();
    });

    it("treats a granted device with no hostname as transient, not ungranted", () => {
      // `/private-dns/me` resolves the domain lazily and degrades to a null
      // hostname on a DDNS hiccup, so granted-without-hostname is reachable.
      usePrivateDnsMe.mockReturnValue({
        data: { enabled: true, granted: true, hostname: null },
        isLoading: false,
      });
      renderWithProviders(<Settings />);
      expect(
        screen.getByText(/hostname isn't available right now/),
      ).toBeInTheDocument();
      expect(
        screen.queryByText(/hasn't been granted Private DNS yet/),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("private-dns-instructions"),
      ).not.toBeInTheDocument();
    });

    it("tells the member to ask an admin when the feature is off", () => {
      usePrivateDnsMe.mockReturnValue({
        data: { enabled: false, granted: false, hostname: null },
        isLoading: false,
      });
      renderWithProviders(<Settings />);
      expect(
        screen.getByText(/Private DNS isn't enabled on your network yet/),
      ).toBeInTheDocument();
      expect(
        screen.queryByTestId("private-dns-instructions"),
      ).not.toBeInTheDocument();
    });

    it("tells the member to ask for a grant when enabled but not granted", () => {
      usePrivateDnsMe.mockReturnValue({
        data: { enabled: true, granted: false, hostname: null },
        isLoading: false,
      });
      renderWithProviders(<Settings />);
      expect(
        screen.getByText(/hasn't been granted Private DNS yet/),
      ).toBeInTheDocument();
      expect(
        screen.queryByTestId("private-dns-instructions"),
      ).not.toBeInTheDocument();
    });

    describe("push deep-link scrolling", () => {
      let scrollIntoView: Mock<(arg?: boolean | ScrollIntoViewOptions) => void>;
      let original: typeof Element.prototype.scrollIntoView;

      beforeEach(() => {
        original = Element.prototype.scrollIntoView;
        // jsdom doesn't implement scrollIntoView, so assign rather than spyOn.
        scrollIntoView = vi.fn();
        Element.prototype.scrollIntoView = scrollIntoView;
        FakeResizeObserver.reset();
        vi.stubGlobal("ResizeObserver", FakeResizeObserver);
      });

      afterEach(() => {
        vi.unstubAllGlobals();
        // A raw prototype assignment isn't undone by restoreAllMocks, and a
        // pushed fragment would otherwise leak into every later test — the
        // component reads `window.location.hash` as well as the router's.
        Element.prototype.scrollIntoView = original;
        const url = new URL(window.location.href);
        url.hash = "";
        window.history.pushState(null, "", url);
      });

      it("scrolls itself into view when the push deep link targets it", () => {
        // The card is last of four, so the `private_dns_granted` push appends
        // `#private-dns`; react-router does no hash scrolling of its own.
        renderWithProviders(<Settings />, { route: "/settings#private-dns" });
        expect(scrollIntoView).toHaveBeenCalledWith({
          behavior: "smooth",
          block: "start",
        });
      });

      it("does not scroll when Settings is opened without the fragment", () => {
        renderWithProviders(<Settings />, { route: "/settings" });
        expect(scrollIntoView).not.toHaveBeenCalled();
      });

      it("scrolls on a fragment-only navigation that only fires hashchange", () => {
        // The SW's `existing.navigate(…#private-dns)` on an already-open app is
        // a same-document navigation: `hashchange` fires, `popstate` does not,
        // so BrowserRouter's hash never updates. Without the listener the card
        // would sit off-screen in exactly the flow the push exists for.
        renderWithProviders(<Settings />, { route: "/settings" });
        expect(scrollIntoView).not.toHaveBeenCalled();

        const url = new URL(window.location.href);
        url.hash = "#private-dns";
        window.history.pushState(null, "", url);
        window.dispatchEvent(new HashChangeEvent("hashchange"));

        expect(scrollIntoView).toHaveBeenCalledWith({
          behavior: "smooth",
          block: "start",
        });
      });

      it("holds the card in view while the cards above it settle", () => {
        // The bug (#1176): the scroll fired once at mount, landing on this
        // card's one-line "Loading…" branch, and the three data-driven cards
        // above then expanded and pushed it back below the fold. The observer
        // watches the container the cards share, so their growth re-scrolls.
        usePrivateDnsMe.mockReturnValue({ data: undefined, isLoading: true });
        renderWithProviders(<Settings />, { route: "/settings#private-dns" });
        expect(scrollIntoView).toHaveBeenCalledTimes(1);

        const card = screen.getByTestId("private-dns-card");
        expect(FakeResizeObserver.last.targets).toEqual([
          card,
          card.parentElement,
        ]);

        FakeResizeObserver.last.fire();
        expect(scrollIntoView).toHaveBeenCalledTimes(2);
      });

      it("re-scrolls once this card's own grant query resolves", () => {
        usePrivateDnsMe.mockReturnValue({ data: undefined, isLoading: true });
        const { rerender } = renderWithProviders(<Settings />, {
          route: "/settings#private-dns",
        });
        expect(scrollIntoView).toHaveBeenCalledTimes(1);

        // Swapping "Loading…" for the setup steps changes what the user came
        // for, so the landing position is re-taken even if the layout around
        // the card happens not to move.
        usePrivateDnsMe.mockReturnValue({
          data: {
            enabled: true,
            granted: true,
            hostname: "tok.abc.my.wardnet.services",
          },
          isLoading: false,
        });
        rerender(<Settings />);

        expect(scrollIntoView).toHaveBeenCalledTimes(2);
      });
    });

    it("shows setup instructions without a QR once granted", async () => {
      usePrivateDnsMe.mockReturnValue({
        data: {
          enabled: true,
          granted: true,
          hostname: "tok.abc.my.wardnet.services",
        },
        isLoading: false,
      });
      renderWithProviders(<Settings />);

      expect(screen.getByTestId("private-dns-hostname")).toHaveTextContent(
        "tok.abc.my.wardnet.services",
      );
      expect(
        screen.getByTestId("private-dns-copy-hostname"),
      ).toBeInTheDocument();
      expect(
        screen.getByTestId("private-dns-profile-link"),
      ).toBeInTheDocument();

      // The phone reading this *is* the target device, so a QR would ask it to
      // scan its own screen (#916). Wait for the iOS copy that replaces it, so
      // a late-resolving QR image can't slip past the negative assertion.
      expect(
        await screen.findByText(/Tap the link above to download the profile/),
      ).toBeInTheDocument();
      expect(
        screen.queryByAltText("Private DNS configuration profile QR code"),
      ).not.toBeInTheDocument();
    });
  });
});
