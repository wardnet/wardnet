import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  isAvailable: vi.fn(),
  isRegistered: vi.fn(),
  register: vi.fn(),
  login: vi.fn(),
}));

// Stub LoginForm so we can drive its onSuccess callback deterministically.
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useAuthStore: () => ({ login: h.login }),
    LoginForm: ({ onSuccess }: { onSuccess: (u: string) => void }) => (
      <button onClick={() => onSuccess("alice")}>do-login</button>
    ),
  };
});
vi.mock("@/hooks/useBiometric", () => ({
  useBiometric: () => ({
    isAvailable: h.isAvailable,
    isRegistered: h.isRegistered,
    register: h.register,
  }),
}));

import Login from "@/pages/Login";
import { renderWithProviders } from "../test-utils";

describe("Login page", () => {
  beforeEach(() => vi.clearAllMocks());

  it("unlocks directly when biometrics are unavailable", async () => {
    h.isAvailable.mockReturnValue(false);
    h.isRegistered.mockReturnValue(false);
    const onUnlock = vi.fn();
    renderWithProviders(<Login onUnlock={onUnlock} />);
    await userEvent.click(screen.getByText("do-login"));
    expect(onUnlock).toHaveBeenCalledOnce();
    expect(screen.queryByText("Enable biometric unlock?")).not.toBeInTheDocument();
  });

  it("unlocks directly when a credential is already registered", async () => {
    h.isAvailable.mockReturnValue(true);
    h.isRegistered.mockReturnValue(true);
    const onUnlock = vi.fn();
    renderWithProviders(<Login onUnlock={onUnlock} />);
    await userEvent.click(screen.getByText("do-login"));
    expect(onUnlock).toHaveBeenCalledOnce();
  });

  it("offers the biometric setup prompt when available and unregistered", async () => {
    h.isAvailable.mockReturnValue(true);
    h.isRegistered.mockReturnValue(false);
    h.register.mockResolvedValue(undefined);
    const onUnlock = vi.fn();
    renderWithProviders(<Login onUnlock={onUnlock} />);
    await userEvent.click(screen.getByText("do-login"));
    expect(screen.getByText("Enable biometric unlock?")).toBeInTheDocument();

    await userEvent.click(screen.getByTestId("biometric-setup-enable"));
    await waitFor(() => expect(onUnlock).toHaveBeenCalledOnce());
    expect(h.register).toHaveBeenCalledWith("alice");
  });

  it("unlocks on decline of the biometric prompt", async () => {
    h.isAvailable.mockReturnValue(true);
    h.isRegistered.mockReturnValue(false);
    const onUnlock = vi.fn();
    renderWithProviders(<Login onUnlock={onUnlock} />);
    await userEvent.click(screen.getByText("do-login"));
    await userEvent.click(screen.getByTestId("biometric-setup-decline"));
    expect(onUnlock).toHaveBeenCalledOnce();
  });
});
