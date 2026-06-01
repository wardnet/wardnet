import { useState } from "react";
import { useNavigate } from "react-router";
import { WardnetApiError } from "@wardnet/js";
import { Button } from "@wardnet/forge-web/button";
import { Field } from "@wardnet/forge-web/field";
import { Form, Validator } from "@wardnet/forge-web/form";
import { Input } from "@wardnet/forge-web/input";
import { useAuth } from "@wardnet/wardnet-web";
import { useBiometric } from "@/hooks/useBiometric";
import { BiometricSetupPrompt } from "@/components/BiometricSetupPrompt";

interface Props {
  /** Called after a successful login to mark this session as unlocked. */
  onUnlock: () => void;
}

/**
 * Admin login page for the mobile admin PWA.
 *
 * Always sends `rememberMe: true` — admin-app sessions are always 30 days.
 * After a successful login, if the device supports biometrics and no
 * credential is registered yet, shows the `<BiometricSetupPrompt>`.
 */
export default function Login({ onUnlock }: Props) {
  const navigate = useNavigate();
  const { login } = useAuth();
  const biometric = useBiometric();

  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [showBiometricSetup, setShowBiometricSetup] = useState(false);

  async function handleSubmit(values: { username: string; password: string }) {
    setFormError(null);
    setLoading(true);
    try {
      await login(values.username, values.password, true);

      if (biometric.isAvailable() && !biometric.isRegistered()) {
        setShowBiometricSetup(true);
      } else {
        onUnlock();
        navigate("/", { replace: true });
      }
    } catch (err) {
      if (err instanceof WardnetApiError && err.status === 401) {
        setFormError("Invalid username or password.");
      } else {
        setFormError("Unable to connect to daemon. Is it running?");
      }
    } finally {
      setLoading(false);
    }
  }

  if (showBiometricSetup) {
    return (
      <BiometricSetupPrompt
        username={username}
        onAccept={() => {
          onUnlock();
          navigate("/", { replace: true });
        }}
        onDecline={() => {
          onUnlock();
          navigate("/", { replace: true });
        }}
      />
    );
  }

  return (
    <Form values={{ username, password }} onSubmit={handleSubmit} className="flex flex-col gap-5">
      <Field label="Username" htmlFor="username" name="username">
        <Input
          id="username"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoComplete="username"
          placeholder="admin"
          inputMode="text"
        />
      </Field>
      <Validator name="username" rule="required" message="Username is required." />

      <Field label="Password" htmlFor="password" name="password">
        <Input
          id="password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="current-password"
          placeholder="••••••••"
        />
      </Field>
      <Validator name="password" rule="required" message="Password is required." />

      {formError && <p className="text-sm text-danger">{formError}</p>}
      <Button type="submit" disabled={loading} className="w-full">
        {loading ? "Signing in…" : "Log in"}
      </Button>
    </Form>
  );
}
