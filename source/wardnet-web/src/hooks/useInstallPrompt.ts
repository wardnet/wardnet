import { useCallback, useEffect, useState } from "react";

interface BeforeInstallPromptEvent extends Event {
  prompt(): Promise<void>;
  userChoice: Promise<InstallPromptResult>;
}

export interface InstallPromptResult {
  outcome: "accepted" | "dismissed";
  platform: string;
}

/**
 * Captures the browser's `beforeinstallprompt` event so it can be triggered
 * on demand (e.g. from an "Install app" button) rather than shown immediately.
 *
 * `isInstallable` is `true` only on browsers that fire `beforeinstallprompt`
 * (Chromium-based); it is always `false` on Safari / Firefox.
 */
export function useInstallPrompt() {
  const [promptEvent, setPromptEvent] =
    useState<BeforeInstallPromptEvent | null>(null);

  useEffect(() => {
    const handler = (e: Event) => {
      e.preventDefault();
      setPromptEvent(e as BeforeInstallPromptEvent);
    };
    window.addEventListener("beforeinstallprompt", handler);
    return () => window.removeEventListener("beforeinstallprompt", handler);
  }, []);

  /**
   * Shows the browser's native install dialog.
   * Returns the user's choice, or `null` if the prompt is not available.
   * The prompt can only be shown once per `beforeinstallprompt` event, so
   * `isInstallable` reverts to `false` after this is called.
   */
  const promptInstall =
    useCallback(async (): Promise<InstallPromptResult | null> => {
      if (!promptEvent) return null;
      // Capture before the first await — the state update from a re-fired
      // `beforeinstallprompt` could replace `promptEvent` while we await.
      const captured = promptEvent;
      await captured.prompt();
      const result = await captured.userChoice;
      setPromptEvent(null);
      return result;
    }, [promptEvent]);

  return {
    /** `true` when the browser has signalled that the app can be installed. */
    isInstallable: promptEvent !== null,
    promptInstall,
  };
}
