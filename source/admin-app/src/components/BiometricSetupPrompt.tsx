import { useState } from "react";
import { useBiometric } from "@/hooks/useBiometric";

interface Props {
  username: string;
  onAccept: () => void;
  onDecline: () => void;
}

/**
 * Modal shown once after the first successful login when the device supports
 * biometrics and no credential is registered yet.
 *
 * - Accept: registers the credential, then calls `onAccept`.
 * - Decline: calls `onDecline` without registering.
 */
export function BiometricSetupPrompt({ username, onAccept, onDecline }: Props) {
  const biometric = useBiometric();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleAccept() {
    setLoading(true);
    setError(null);
    try {
      await biometric.register(username);
      onAccept();
    } catch {
      setError("Biometric setup failed. You can enable it later from settings.");
      setLoading(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 px-4">
      <div className="w-full max-w-sm rounded-xl bg-surface p-6 shadow-xl flex flex-col gap-4">
        <div>
          <h2 className="text-base font-semibold text-ink">Enable biometric unlock?</h2>
          <p className="mt-1 text-sm text-ink-3">
            Use FaceID, Touch ID, or your device fingerprint to unlock the app on future opens
            without typing your password.
          </p>
        </div>

        {error && <p className="text-sm text-danger">{error}</p>}

        <div className="flex gap-3">
          <button
            onClick={handleAccept}
            disabled={loading}
            className="flex-1 rounded-md bg-accent px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
          >
            {loading ? "Setting up…" : "Enable"}
          </button>
          <button
            onClick={onDecline}
            disabled={loading}
            className="flex-1 rounded-md border border-ink-5 px-4 py-2 text-sm font-medium text-ink disabled:opacity-50"
          >
            Not now
          </button>
        </div>
      </div>
    </div>
  );
}
