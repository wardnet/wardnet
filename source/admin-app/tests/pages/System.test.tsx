import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  useSystemStatus: vi.fn(),
  useDaemonStatus: vi.fn(),
  useDdnsStatus: vi.fn(),
  useTlsStatus: vi.fn(),
  useResolutionCheck: vi.fn(),
  useRestart: vi.fn(),
  useReboot: vi.fn(),
  usePushNotifications: vi.fn(),
  useRecentNotifications: vi.fn(),
  useClearNotifications: vi.fn(),
  logout: vi.fn(),
  unregister: vi.fn(),
  toastError: vi.fn(),
}));
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useSystemStatus: h.useSystemStatus,
    useDaemonStatus: h.useDaemonStatus,
    useDdnsStatus: h.useDdnsStatus,
    useTlsStatus: h.useTlsStatus,
    useResolutionCheck: h.useResolutionCheck,
    useRestart: h.useRestart,
    useReboot: h.useReboot,
    usePushNotifications: h.usePushNotifications,
    useRecentNotifications: h.useRecentNotifications,
    useClearNotifications: h.useClearNotifications,
    useAuth: () => ({ logout: h.logout }),
  };
});
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast: { success: vi.fn(), error: h.toastError } };
});
vi.mock("@/hooks/useBiometric", () => ({
  useBiometric: () => ({ unregister: h.unregister }),
}));
vi.mock("@/context/OnlineStatusContext", () => ({
  useOnlineStatusContext: () => ({ showingLastKnownState: false }),
}));

import System from "@/pages/System";
import { renderWithProviders } from "../test-utils";

const idle = {
  isOpen: false,
  phase: "idle",
  start: vi.fn(),
  reset: vi.fn(),
  errorMessage: null,
};

function baseMocks() {
  h.useSystemStatus.mockReturnValue({
    data: {
      uptime_seconds: 3600,
      cpu_usage_percent: 12.5,
      memory_used_bytes: 500,
      memory_total_bytes: 1000,
      disk_free_bytes: 400,
      disk_total_bytes: 1000,
    },
  });
  h.useDaemonStatus.mockReturnValue({
    data: { reachable: true, version: "1.2.3" },
  });
  h.useDdnsStatus.mockReturnValue({ data: { provider: null } });
  h.useTlsStatus.mockReturnValue({ data: null });
  h.useResolutionCheck.mockReturnValue({ data: null });
  h.useRestart.mockReturnValue({ ...idle, start: vi.fn(), reset: vi.fn() });
  h.useReboot.mockReturnValue({ ...idle, start: vi.fn(), reset: vi.fn() });
  h.usePushNotifications.mockReturnValue({
    state: "prompt",
    isBusy: false,
    subscribe: vi.fn(),
    unsubscribe: vi.fn(),
  });
  h.useRecentNotifications.mockReturnValue({ data: [] });
  h.useClearNotifications.mockReturnValue({
    mutate: vi.fn(),
    isPending: false,
  });
}

describe("System page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    baseMocks();
  });

  it("renders the daemon metrics and running pill", () => {
    renderWithProviders(<System />);
    expect(screen.getByTestId("system-status-pill")).toHaveTextContent(
      "Running",
    );
    expect(screen.getByText("v1.2.3")).toBeInTheDocument();
    expect(screen.getByText("12.5%")).toBeInTheDocument();
  });

  it("shows the unreachable pill and em dashes when no status", () => {
    h.useSystemStatus.mockReturnValue({ data: undefined });
    h.useDaemonStatus.mockReturnValue({
      data: { reachable: false, version: null },
    });
    renderWithProviders(<System />);
    expect(screen.getByTestId("system-status-pill")).toHaveTextContent(
      "Unreachable",
    );
  });

  it("renders the remote-access section when a DDNS provider is set", () => {
    h.useDdnsStatus.mockReturnValue({
      data: { provider: "cloudflare", fqdn: "home.example.net" },
    });
    h.useTlsStatus.mockReturnValue({
      data: { not_after: "2027-01-01T00:00:00Z" },
    });
    h.useResolutionCheck.mockReturnValue({ data: { verdict: "match" } });
    renderWithProviders(<System />);
    expect(screen.getByText("Remote access")).toBeInTheDocument();
    expect(screen.getByText("home.example.net")).toBeInTheDocument();
    expect(screen.getByText("Reachable")).toBeInTheDocument();
  });

  it("subscribes to push notifications from the toggle", async () => {
    const subscribe = vi.fn();
    h.usePushNotifications.mockReturnValue({
      state: "prompt",
      isBusy: false,
      subscribe,
      unsubscribe: vi.fn(),
    });
    renderWithProviders(<System />);
    await userEvent.click(screen.getByTestId("system-notifications-toggle"));
    expect(subscribe).toHaveBeenCalledOnce();
  });

  it("unsubscribes when the toggle is on and clicked", async () => {
    const unsubscribe = vi.fn();
    h.usePushNotifications.mockReturnValue({
      state: "subscribed",
      isBusy: false,
      subscribe: vi.fn(),
      unsubscribe,
    });
    renderWithProviders(<System />);
    await userEvent.click(screen.getByTestId("system-notifications-toggle"));
    expect(unsubscribe).toHaveBeenCalledOnce();
  });

  it("disables the toggle and explains when push is unsupported", () => {
    h.usePushNotifications.mockReturnValue({
      state: "unsupported",
      isBusy: false,
      subscribe: vi.fn(),
      unsubscribe: vi.fn(),
    });
    renderWithProviders(<System />);
    expect(screen.getByTestId("system-notifications-toggle")).toBeDisabled();
    expect(
      screen.getByText("Notifications are not supported in this browser."),
    ).toBeInTheDocument();
  });

  it("tells iOS browser-tab users to install the app when push is unsupported", () => {
    h.usePushNotifications.mockReturnValue({
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
      renderWithProviders(<System />);
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
    h.usePushNotifications.mockReturnValue({
      state: "denied",
      isBusy: false,
      subscribe: vi.fn(),
      unsubscribe: vi.fn(),
    });
    renderWithProviders(<System />);
    expect(screen.getByTestId("system-notifications-toggle")).toBeDisabled();
    expect(
      screen.getByText("Notifications are blocked in your browser settings."),
    ).toBeInTheDocument();
  });

  it("disables the toggle while a subscribe/unsubscribe is in flight", () => {
    h.usePushNotifications.mockReturnValue({
      state: "prompt",
      isBusy: true,
      subscribe: vi.fn(),
      unsubscribe: vi.fn(),
    });
    renderWithProviders(<System />);
    expect(screen.getByTestId("system-notifications-toggle")).toBeDisabled();
  });

  it("disables Clear while the mutation is pending", () => {
    h.useRecentNotifications.mockReturnValue({
      data: [
        {
          id: "n1",
          kind: "rule_request_created",
          title: "Rule request",
          body: "Phone asked to allow blocked.example.",
          created_at: "2026-07-03T00:00:00Z",
        },
      ],
    });
    h.useClearNotifications.mockReturnValue({
      mutate: vi.fn(),
      isPending: true,
    });
    renderWithProviders(<System />);
    expect(screen.getByTestId("system-notifications-clear")).toBeDisabled();
    // The rule-request kind renders its own pill label.
    expect(screen.getByText("Request")).toBeInTheDocument();
  });

  it("renders the notification feed with a clear action", async () => {
    const clear = vi.fn();
    h.useRecentNotifications.mockReturnValue({
      data: [
        {
          id: "n1",
          kind: "new_device_quarantined",
          title: "New device",
          body: "New device Phone joined, in Guest. Approve in the app.",
          url: "/devices",
          subject_id: "d1",
          created_at: "2026-07-03T00:00:00Z",
        },
        {
          id: "n2",
          kind: "tunnel_offline",
          title: "Tunnel offline",
          body: "Sweden #12 went offline.",
          url: "/tunnels",
          created_at: "2026-07-02T00:00:00Z",
        },
      ],
    });
    h.useClearNotifications.mockReturnValue({
      mutate: clear,
      isPending: false,
    });
    renderWithProviders(<System />);

    const feed = screen.getByTestId("system-notifications-feed");
    expect(feed).toBeInTheDocument();
    // "New device" appears as both the kind pill and the title.
    expect(screen.getAllByText("New device").length).toBeGreaterThanOrEqual(1);
    expect(
      screen.getByText(
        "New device Phone joined, in Guest. Approve in the app.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("Sweden #12 went offline.")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("system-notifications-clear"));
    expect(clear).toHaveBeenCalledOnce();
  });

  it("hides the feed when there are no notifications", () => {
    renderWithProviders(<System />);
    expect(
      screen.queryByTestId("system-notifications-feed"),
    ).not.toBeInTheDocument();
  });

  it("confirms and starts a daemon restart", async () => {
    const start = vi.fn();
    h.useRestart.mockReturnValue({ ...idle, start, reset: vi.fn() });
    renderWithProviders(<System />);
    await userEvent.click(screen.getByTestId("system-restart-daemon"));
    expect(screen.getByText("Restart daemon?")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("confirm-dialog-confirm"));
    expect(start).toHaveBeenCalledOnce();
  });

  it("confirms and starts a device reboot", async () => {
    const start = vi.fn();
    h.useReboot.mockReturnValue({ ...idle, start, reset: vi.fn() });
    renderWithProviders(<System />);
    await userEvent.click(screen.getByTestId("system-reboot-device"));
    expect(screen.getByText("Reboot device?")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("confirm-dialog-confirm"));
    expect(start).toHaveBeenCalledOnce();
  });

  it("logs out, clears biometrics on the logout action", async () => {
    h.logout.mockResolvedValue(undefined);
    renderWithProviders(<System />);
    await userEvent.click(screen.getByTestId("system-logout"));
    expect(h.logout).toHaveBeenCalledOnce();
    await waitFor(() => expect(h.unregister).toHaveBeenCalledOnce());
  });

  it("keeps the biometric gate until the session revoke settles", async () => {
    let resolveLogout!: () => void;
    h.logout.mockImplementation(
      () => new Promise<void>((resolve) => (resolveLogout = resolve)),
    );
    renderWithProviders(<System />);
    await userEvent.click(screen.getByTestId("system-logout"));
    expect(h.logout).toHaveBeenCalledOnce();
    // The local gate must not drop while the server-side logout is pending.
    expect(h.unregister).not.toHaveBeenCalled();
    resolveLogout();
    await waitFor(() => expect(h.unregister).toHaveBeenCalledOnce());
  });

  it("shows the busy overlay while a restart is in flight", () => {
    h.useRestart.mockReturnValue({
      ...idle,
      isOpen: true,
      phase: "working",
      start: vi.fn(),
      reset: vi.fn(),
    });
    renderWithProviders(<System />);
    expect(screen.getByTestId("system-busy-overlay")).toBeInTheDocument();
  });

  it("signs out and resets when a reboot reports ready_signed_out", () => {
    const reset = vi.fn();
    h.useReboot.mockReturnValue({
      isOpen: true,
      phase: "ready_signed_out",
      start: vi.fn(),
      reset,
      errorMessage: null,
    });
    renderWithProviders(<System />);
    expect(h.logout).toHaveBeenCalledOnce();
    expect(h.unregister).toHaveBeenCalledOnce();
    expect(reset).toHaveBeenCalledOnce();
  });

  it("toasts and resets on a failed action", () => {
    const reset = vi.fn();
    h.useRestart.mockReturnValue({
      isOpen: true,
      phase: "failed",
      start: vi.fn(),
      reset,
      errorMessage: "boom",
    });
    renderWithProviders(<System />);
    expect(h.toastError).toHaveBeenCalledWith("boom");
    expect(reset).toHaveBeenCalledOnce();
  });
});
