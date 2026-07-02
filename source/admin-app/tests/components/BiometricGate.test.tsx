import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { authenticate } = vi.hoisted(() => ({ authenticate: vi.fn() }));
vi.mock("@/hooks/useBiometric", () => ({
  useBiometric: () => ({ authenticate }),
}));

import { BiometricGate } from "@/components/BiometricGate";

describe("BiometricGate", () => {
  beforeEach(() => vi.clearAllMocks());

  it("calls onSuccess when the ceremony succeeds on mount", async () => {
    authenticate.mockResolvedValue(undefined);
    const onSuccess = vi.fn();
    render(<BiometricGate onSuccess={onSuccess} onUsePassword={vi.fn()} />);
    await waitFor(() => expect(onSuccess).toHaveBeenCalledOnce());
  });

  it("shows a retry button on failure and re-attempts on click", async () => {
    authenticate.mockRejectedValueOnce(new Error("nope")).mockResolvedValueOnce(undefined);
    const onSuccess = vi.fn();
    render(<BiometricGate onSuccess={onSuccess} onUsePassword={vi.fn()} />);

    await waitFor(() =>
      expect(screen.getByText(/verification failed/i)).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(onSuccess).toHaveBeenCalledOnce());
  });

  it("invokes onUsePassword from the escape-hatch link", async () => {
    authenticate.mockResolvedValue(undefined);
    const onUsePassword = vi.fn();
    render(<BiometricGate onSuccess={vi.fn()} onUsePassword={onUsePassword} />);
    await userEvent.click(screen.getByText("Use password instead"));
    expect(onUsePassword).toHaveBeenCalledOnce();
  });
});
