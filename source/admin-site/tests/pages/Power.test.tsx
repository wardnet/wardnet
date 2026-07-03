/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { navigate, useReboot, useRestart, useShutdown, useAuthStore } =
  vi.hoisted(() => ({
    navigate: vi.fn(),
    useReboot: vi.fn(),
    useRestart: vi.fn(),
    useShutdown: vi.fn(),
    useAuthStore: vi.fn(),
  }));

vi.mock("react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-router")>();
  return { ...actual, useNavigate: () => navigate };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useReboot, useRestart, useShutdown, useAuthStore };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: any) => <h1>{title}</h1>,
}));
vi.mock("@/components/features/PowerCard", () => ({
  PowerCard: ({ onReboot, onShutdown, onRestartDaemon, busy }: any) => (
    <div data-testid="power-card" data-busy={String(busy)}>
      <button onClick={onRestartDaemon}>restart-daemon</button>
      <button onClick={onReboot}>reboot</button>
      <button onClick={onShutdown}>shutdown</button>
    </div>
  ),
}));
vi.mock("@/components/features/RestartProgressDialog", () => ({
  RestartProgressDialog: ({ open, onDismiss, onSignIn }: any) =>
    open ? (
      <div data-testid="restart-dialog">
        <button onClick={onDismiss}>dismiss</button>
        <button onClick={onSignIn}>sign-in</button>
      </div>
    ) : null,
}));
vi.mock("@/components/features/ShutdownProgressDialog", () => ({
  ShutdownProgressDialog: ({ open, onDismiss }: any) =>
    open ? (
      <div data-testid="shutdown-dialog">
        <button onClick={onDismiss}>shutdown-dismiss</button>
      </div>
    ) : null,
}));

import Power from "@/pages/Power";
import { renderWithProviders } from "../test-utils";

const logout = vi.fn();

function opState(overrides: Record<string, unknown> = {}) {
  return {
    isOpen: false,
    phase: "idle",
    startedAt: null,
    errorMessage: null,
    start: vi.fn(),
    reset: vi.fn(),
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useAuthStore.mockImplementation((sel: any) => sel({ logout }));
  useRestart.mockReturnValue(opState());
  useReboot.mockReturnValue(opState());
  useShutdown.mockReturnValue(opState());
});

describe("Power", () => {
  it("renders the power card without dialogs open", () => {
    renderWithProviders(<Power />);
    expect(screen.getByText("Power")).toBeInTheDocument();
    expect(screen.getByTestId("power-card")).toHaveAttribute(
      "data-busy",
      "false",
    );
    expect(screen.queryByTestId("restart-dialog")).not.toBeInTheDocument();
  });

  it("marks the card busy when any op is in flight", () => {
    useReboot.mockReturnValue(opState({ isOpen: true }));
    renderWithProviders(<Power />);
    expect(screen.getByTestId("power-card")).toHaveAttribute(
      "data-busy",
      "true",
    );
  });

  it("starts each power operation from the card buttons", async () => {
    const restart = opState();
    const reboot = opState();
    const shutdown = opState();
    useRestart.mockReturnValue(restart);
    useReboot.mockReturnValue(reboot);
    useShutdown.mockReturnValue(shutdown);
    const user = userEvent.setup();
    renderWithProviders(<Power />);
    await user.click(screen.getByText("restart-daemon"));
    await user.click(screen.getByText("reboot"));
    await user.click(screen.getByText("shutdown"));
    expect(restart.start).toHaveBeenCalled();
    expect(reboot.start).toHaveBeenCalled();
    expect(shutdown.start).toHaveBeenCalled();
  });

  it("dismiss and sign-in reset the restart flow and route to login", async () => {
    const restart = opState({ isOpen: true });
    useRestart.mockReturnValue(restart);
    const user = userEvent.setup();
    renderWithProviders(<Power />);
    await user.click(screen.getByText("dismiss"));
    expect(restart.reset).toHaveBeenCalledTimes(1);
    await user.click(screen.getByText("sign-in"));
    expect(logout).toHaveBeenCalled();
    expect(restart.reset).toHaveBeenCalledTimes(2);
    expect(navigate).toHaveBeenCalledWith("/login");
  });

  it("dismiss and sign-in handle the reboot flow", async () => {
    const reboot = opState({ isOpen: true });
    useReboot.mockReturnValue(reboot);
    const user = userEvent.setup();
    renderWithProviders(<Power />);
    // Two restart dialogs render (restart + reboot share the component);
    // the reboot one is the only open dialog here.
    await user.click(screen.getByText("dismiss"));
    expect(reboot.reset).toHaveBeenCalledTimes(1);
    await user.click(screen.getByText("sign-in"));
    expect(logout).toHaveBeenCalled();
    expect(navigate).toHaveBeenCalledWith("/login");
  });

  it("dismisses the shutdown dialog", async () => {
    const shutdown = opState({ isOpen: true });
    useShutdown.mockReturnValue(shutdown);
    const user = userEvent.setup();
    renderWithProviders(<Power />);
    await user.click(screen.getByText("shutdown-dismiss"));
    expect(shutdown.reset).toHaveBeenCalled();
  });
});
