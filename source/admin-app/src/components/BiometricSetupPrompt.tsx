import { useState } from "react";
import { Card, Text } from "@wardnet/web";
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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 px-4">
      <Card className="w-full max-w-sm flex flex-col gap-4">
        <div>
          <Text as="h2" size="base" weight="semibold" className="text-ink">Enable biometric unlock?</Text>
          <Text as="p" size="sm" className="mt-1 text-ink-3">
            Use FaceID, Touch ID, or your device fingerprint to unlock the app on future opens
            without typing your password.
          </Text>
        </div>

        {error && <Text as="p" size="sm" className="text-danger">{error}</Text>}

        <div className="flex gap-3">
          <button
            onClick={handleAccept}
            disabled={loading}
            data-testid="biometric-setup-enable"
            className="flex-1 rounded-md bg-accent px-4 py-2 text-sm font-medium text-accent-ink disabled:opacity-50"
          >
            {loading ? "Setting up…" : "Enable"}
          </button>
          <button
            onClick={onDecline}
            disabled={loading}
            data-testid="biometric-setup-decline"
            className="flex-1 rounded-md border border-line px-4 py-2 text-sm font-medium text-ink disabled:opacity-50"
          >
            Not now
          </button>
        </div>
      </Card>
    </div>
  );
}
