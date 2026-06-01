import { useEffect, useState } from "react";
import { useBiometric } from "@/hooks/useBiometric";

interface Props {
  onSuccess: () => void;
  onUsePassword: () => void;
}

/**
 * Full-screen overlay shown before routing on every app open when a biometric
 * credential is registered. Immediately triggers the WebAuthn authentication
 * ceremony on mount.
 *
 * - On success → calls `onSuccess`.
 * - On failure → shows a retry button (the browser's WebAuthn UI surfaces
 *   the specific error; we just show that a retry is available).
 * - "Use password instead" link → `onUsePassword` (escape hatch, no lock-out).
 */
export function BiometricGate({ onSuccess, onUsePassword }: Props) {
  const biometric = useBiometric();
  const [error, setError] = useState(false);

  async function attempt() {
    setError(false);
    try {
      await biometric.authenticate();
      onSuccess();
    } catch {
      setError(true);
    }
  }

  useEffect(() => {
    attempt();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-6 bg-bg px-4 text-ink">
      <div className="flex flex-col items-center gap-2 text-center">
        <p className="text-lg font-semibold">Verify it's you</p>
        <p className="text-sm text-ink-3">Use your device biometrics to unlock Wardnet Admin</p>
      </div>

      {error && (
        <div className="flex flex-col items-center gap-3">
          <p className="text-sm text-danger">Biometric verification failed. Try again.</p>
          <button
            onClick={attempt}
            className="rounded-md bg-accent px-4 py-2 text-sm font-medium text-accent-ink"
          >
            Retry
          </button>
        </div>
      )}

      <button
        onClick={onUsePassword}
        className="text-xs text-ink-3 underline underline-offset-2"
      >
        Use password instead
      </button>
    </div>
  );
}
