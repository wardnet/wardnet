import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { WardnetApiError } from "@wardnet/js";
import { Button } from "@wardnet/forge-web/button";
import { Field } from "@wardnet/forge-web/field";
import { Input } from "@wardnet/forge-web/input";
import { useSetup, useAdvanceWizard } from "@/hooks/useSetup";
import { useAuth } from "@/hooks/useAuth";

/** Step 1 — create the first admin account. Unauthenticated. */
export default function Step1Admin() {
  const queryClient = useQueryClient();
  const setup = useSetup();
  const advance = useAdvanceWizard();
  const { login } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);

    if (password.length < 8) {
      setError("Password must be at least 8 characters.");
      return;
    }
    if (password !== confirmPassword) {
      setError("Passwords do not match.");
      return;
    }

    try {
      await setup.mutateAsync({ username, password });
      // Auto-login so the rest of the wizard runs as an authenticated admin.
      await login(username, password);
      // The daemon's setup_admin already advances wizard_step to "network"
      // atomically, but we re-issue advance here to be defensive against
      // server versions that don't yet ship that change.
      await advance.mutateAsync({ to_step: "network" });
      await queryClient.refetchQueries({ queryKey: ["setup", "status"] });
    } catch (err) {
      if (err instanceof WardnetApiError && err.status === 409) {
        // Admin already exists (e.g. operator hard-refreshed mid-flow).
        // Try logging in with the credentials they just typed; if those
        // are right, the wizard's status query will pick up the
        // already-advanced state on refetch.
        try {
          await login(username, password);
          await queryClient.refetchQueries({ queryKey: ["setup", "status"] });
        } catch {
          setError(
            "An admin already exists for this Wardnet, but those credentials don't match. Sign in from /login instead.",
          );
        }
      } else if (err instanceof WardnetApiError) {
        setError(err.body.error);
      } else {
        setError("Unable to connect to daemon. Is it running?");
      }
    }
  }

  return (
    <div>
      <div className="mb-5 flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-ink">Create admin account</h2>
        <p className="text-sm text-ink-3">
          Set up your administrator credentials. You'll use these to sign in to Wardnet.
        </p>
      </div>
      <form onSubmit={handleSubmit} className="flex flex-col gap-5">
        <Field label="Username" htmlFor="username">
          <Input
            id="username"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="username"
            placeholder="admin"
            required
            className="h-12"
          />
        </Field>
        <Field label="Password" htmlFor="password">
          <Input
            id="password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="new-password"
            placeholder="At least 8 characters"
            required
            className="h-12"
          />
        </Field>
        <Field label="Confirm password" htmlFor="confirm-password">
          <Input
            id="confirm-password"
            type="password"
            value={confirmPassword}
            onChange={(e) => setConfirmPassword(e.target.value)}
            autoComplete="new-password"
            placeholder="Re-enter password"
            required
            className="h-12"
          />
        </Field>
        {error && <p className="text-sm text-danger">{error}</p>}
        <Button
          type="submit"
          disabled={setup.isPending || advance.isPending}
          className="h-12 w-full bg-[oklch(0.22_0.12_275)] text-base font-semibold tracking-wide text-white uppercase hover:bg-[oklch(0.28_0.12_275)] dark:bg-accent dark:hover:bg-accent/90"
        >
          {setup.isPending || advance.isPending ? "Creating account…" : "Create account"}
        </Button>
      </form>
    </div>
  );
}
