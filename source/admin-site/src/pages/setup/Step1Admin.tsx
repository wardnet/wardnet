import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { WardnetApiError } from "@wardnet/js";
import { Button } from "@wardnet/web";
import { Heading, Text } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Form, Validator } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { useSetup, useAdvanceWizard } from "@wardnet/web";
import { useAuth } from "@wardnet/web";

interface AdminFormValues extends Record<string, unknown> {
  username: string;
  password: string;
  confirmPassword: string;
}

/** Step 1 — create the first admin account. Unauthenticated. */
export default function Step1Admin() {
  const queryClient = useQueryClient();
  const setup = useSetup();
  const advance = useAdvanceWizard();
  const { login } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [formError, setFormError] = useState<string | null>(null);

  async function handleSubmit(values: AdminFormValues) {
    setFormError(null);
    try {
      await setup.mutateAsync({
        username: values.username,
        password: values.password,
      });
      // Auto-login so the rest of the wizard runs as an authenticated admin.
      await login(values.username, values.password);
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
          await login(values.username, values.password);
          await queryClient.refetchQueries({ queryKey: ["setup", "status"] });
        } catch {
          setFormError(
            "An admin already exists for this Wardnet, but those credentials don't match. Sign in from /login instead.",
          );
        }
      } else if (err instanceof WardnetApiError) {
        setFormError(err.body.error);
      } else {
        setFormError("Unable to connect to daemon. Is it running?");
      }
    }
  }

  return (
    <div>
      <div className="mb-5 flex flex-col gap-1">
        <Heading level={2} size="3xl" className="text-ink">
          Create admin account
        </Heading>
        <Text as="p" size="sm" className="mt-1 text-ink-3">
          Set up your administrator credentials. You'll use these to sign in to
          Wardnet.
        </Text>
      </div>
      <Form
        values={{ username, password, confirmPassword }}
        onSubmit={handleSubmit}
        className="flex flex-col gap-5"
      >
        <Field label="Username" htmlFor="username" name="username">
          <Input
            id="username"
            data-testid="setup-username"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="username"
            placeholder="admin"
          />
        </Field>
        <Validator
          name="username"
          rule="required"
          message="Username is required."
        />

        <Field label="Password" htmlFor="password" name="password">
          <Input
            id="password"
            type="password"
            data-testid="setup-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="new-password"
            placeholder="At least 8 characters"
          />
        </Field>
        <Validator
          name="password"
          rule="required"
          message="Password is required."
        />
        <Validator
          name="password"
          validate={(v) =>
            typeof v === "string" && v.length > 0 && v.length < 8
              ? "Password must be at least 8 characters."
              : null
          }
        />

        <Field
          label="Confirm password"
          htmlFor="confirm-password"
          name="confirmPassword"
        >
          <Input
            id="confirm-password"
            type="password"
            data-testid="setup-confirm-password"
            value={confirmPassword}
            onChange={(e) => setConfirmPassword(e.target.value)}
            autoComplete="new-password"
            placeholder="Re-enter password"
          />
        </Field>
        <Validator
          name="confirmPassword"
          rule="required"
          message="Please confirm your password."
        />
        <Validator
          name="confirmPassword"
          validate={(v) => (v !== password ? "Passwords do not match." : null)}
        />

        {formError && (
          <Text as="p" role="alert" size="sm" className="text-danger">
            {formError}
          </Text>
        )}
        <Button
          type="submit"
          disabled={setup.isPending || advance.isPending}
          data-testid="setup-admin-submit"
          className="w-full"
        >
          {setup.isPending || advance.isPending
            ? "Creating account…"
            : "Create account"}
        </Button>
      </Form>
    </div>
  );
}
