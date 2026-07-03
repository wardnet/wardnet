import { screen } from "@testing-library/react";
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

const idle = { isOpen: false, phase: "idle", start: vi.fn(), reset: vi.fn(), errorMessage: null };

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
  h.useDaemonStatus.mockReturnValue({ data: { reachable: true, version: "1.2.3" } });
  h.useDdnsStatus.mockReturnValue({ data: { provider: null } });
  h.useTlsStatus.mockReturnValue({ data: null });
  h.useResolutionCheck.mockReturnValue({ data: null });
  h.useRestart.mockReturnValue({ ...idle, start: vi.fn(), reset: vi.fn() });
  h.useReboot.mockReturnValue({ ...idle, start: vi.fn(), reset: vi.fn() });
}

describe("System page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    baseMocks();
  });

  it("renders the daemon metrics and running pill", () => {
    renderWithProviders(<System />);
    expect(screen.getByTestId("system-status-pill")).toHaveTextContent("Running");
    expect(screen.getByText("v1.2.3")).toBeInTheDocument();
    expect(screen.getByText("12.5%")).toBeInTheDocument();
  });

  it("shows the unreachable pill and em dashes when no status", () => {
    h.useSystemStatus.mockReturnValue({ data: undefined });
    h.useDaemonStatus.mockReturnValue({ data: { reachable: false, version: null } });
    renderWithProviders(<System />);
    expect(screen.getByTestId("system-status-pill")).toHaveTextContent("Unreachable");
  });

  it("renders the remote-access section when a DDNS provider is set", () => {
    h.useDdnsStatus.mockReturnValue({ data: { provider: "cloudflare", fqdn: "home.example.net" } });
    h.useTlsStatus.mockReturnValue({ data: { not_after: "2027-01-01T00:00:00Z" } });
    h.useResolutionCheck.mockReturnValue({ data: { verdict: "match" } });
    renderWithProviders(<System />);
    expect(screen.getByText("Remote access")).toBeInTheDocument();
    expect(screen.getByText("home.example.net")).toBeInTheDocument();
    expect(screen.getByText("Reachable")).toBeInTheDocument();
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
    renderWithProviders(<System />);
    await userEvent.click(screen.getByTestId("system-logout"));
    expect(h.logout).toHaveBeenCalledOnce();
    expect(h.unregister).toHaveBeenCalledOnce();
  });

  it("shows the busy overlay while a restart is in flight", () => {
    h.useRestart.mockReturnValue({ ...idle, isOpen: true, phase: "working", start: vi.fn(), reset: vi.fn() });
    renderWithProviders(<System />);
    expect(screen.getByTestId("system-busy-overlay")).toBeInTheDocument();
  });

  it("signs out and resets when a reboot reports ready_signed_out", () => {
    const reset = vi.fn();
    h.useReboot.mockReturnValue({ isOpen: true, phase: "ready_signed_out", start: vi.fn(), reset, errorMessage: null });
    renderWithProviders(<System />);
    expect(h.logout).toHaveBeenCalledOnce();
    expect(h.unregister).toHaveBeenCalledOnce();
    expect(reset).toHaveBeenCalledOnce();
  });

  it("toasts and resets on a failed action", () => {
    const reset = vi.fn();
    h.useRestart.mockReturnValue({ isOpen: true, phase: "failed", start: vi.fn(), reset, errorMessage: "boom" });
    renderWithProviders(<System />);
    expect(h.toastError).toHaveBeenCalledWith("boom");
    expect(reset).toHaveBeenCalledOnce();
  });
});
