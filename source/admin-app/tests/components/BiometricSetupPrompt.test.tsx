import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { register } = vi.hoisted(() => ({ register: vi.fn() }));
vi.mock("@/hooks/useBiometric", () => ({
  useBiometric: () => ({ register }),
}));

import { BiometricSetupPrompt } from "@/components/BiometricSetupPrompt";

describe("BiometricSetupPrompt", () => {
  beforeEach(() => vi.clearAllMocks());

  it("registers the credential and calls onAccept on Enable", async () => {
    register.mockResolvedValue(undefined);
    const onAccept = vi.fn();
    render(
      <BiometricSetupPrompt
        username="alice"
        onAccept={onAccept}
        onDecline={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByTestId("biometric-setup-enable"));
    await waitFor(() => expect(onAccept).toHaveBeenCalledOnce());
    expect(register).toHaveBeenCalledWith("alice");
  });

  it("shows an error and stays open when registration fails", async () => {
    register.mockRejectedValue(new Error("denied"));
    const onAccept = vi.fn();
    render(
      <BiometricSetupPrompt
        username="alice"
        onAccept={onAccept}
        onDecline={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByTestId("biometric-setup-enable"));
    await waitFor(() =>
      expect(screen.getByText(/Biometric setup failed/i)).toBeInTheDocument(),
    );
    expect(onAccept).not.toHaveBeenCalled();
  });

  it("calls onDecline without registering on Not now", async () => {
    const onDecline = vi.fn();
    render(
      <BiometricSetupPrompt
        username="alice"
        onAccept={vi.fn()}
        onDecline={onDecline}
      />,
    );
    await userEvent.click(screen.getByTestId("biometric-setup-decline"));
    expect(onDecline).toHaveBeenCalledOnce();
    expect(register).not.toHaveBeenCalled();
  });
});
