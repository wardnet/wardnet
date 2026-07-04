import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { DeviceRuleRequest } from "@wardnet/js";

import Settings from "../../src/pages/Settings";
import { makeDevice, renderWithProviders } from "../test-utils";

const {
  useMyDevice,
  useMyRuleRequests,
  useSetMyCaptureEnabled,
  usePushNotifications,
} = vi.hoisted(() => ({
  useMyDevice: vi.fn(),
  useMyRuleRequests: vi.fn(),
  useSetMyCaptureEnabled: vi.fn(),
  usePushNotifications: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useMyDevice,
    useMyRuleRequests,
    useSetMyCaptureEnabled,
    usePushNotifications,
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
});
