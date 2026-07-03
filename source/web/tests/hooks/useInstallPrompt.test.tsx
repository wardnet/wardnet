import { renderHook, act, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useInstallPrompt } from "../../src/hooks/useInstallPrompt";

function fireBeforeInstallPrompt(choice: {
  outcome: "accepted" | "dismissed";
  platform: string;
}) {
  const event = new Event("beforeinstallprompt") as Event & {
    prompt: () => Promise<void>;
    userChoice: Promise<typeof choice>;
  };
  event.prompt = vi.fn().mockResolvedValue(undefined);
  event.userChoice = Promise.resolve(choice);
  window.dispatchEvent(event);
  return event;
}

describe("useInstallPrompt", () => {
  beforeEach(() => vi.clearAllMocks());

  it("starts non-installable and returns null when prompting early", async () => {
    const { result } = renderHook(() => useInstallPrompt());
    expect(result.current.isInstallable).toBe(false);
    const outcome = await result.current.promptInstall();
    expect(outcome).toBeNull();
  });

  it("captures the event, becomes installable, and resolves the choice", async () => {
    const { result } = renderHook(() => useInstallPrompt());
    let event!: ReturnType<typeof fireBeforeInstallPrompt>;
    act(() => {
      event = fireBeforeInstallPrompt({
        outcome: "accepted",
        platform: "web",
      });
    });
    await waitFor(() => expect(result.current.isInstallable).toBe(true));

    let outcome: { outcome: string } | null = null;
    await act(async () => {
      outcome = await result.current.promptInstall();
    });
    expect(event.prompt).toHaveBeenCalledOnce();
    expect(outcome).toEqual({ outcome: "accepted", platform: "web" });
    // The prompt can only be used once — reverts to non-installable.
    await waitFor(() => expect(result.current.isInstallable).toBe(false));
  });
});
