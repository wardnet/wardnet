import { useState } from "react";
import { WardnetApiError } from "@wardnet/js";
import { Button } from "../primitives/button";
import { Field } from "../primitives/field";
import { Form, Validator } from "../primitives/form";
import { Input } from "../primitives/input";

interface LoginFormProps {
  /** Injected login action — callers supply this from their auth store or hook. */
  login: (
    username: string,
    password: string,
    rememberMe?: boolean,
  ) => Promise<void>;
  /**
   * Controls the "Remember me" behaviour:
   * - `"checkbox"` — shows an unchecked checkbox the user can toggle (admin-site)
   * - `true`       — always remember me, no checkbox shown (admin-app)
   * - `false`      — never remember me, no checkbox shown (default)
   */
  rememberMe?: boolean | "checkbox";
  /** Called with the username after a successful login. Navigation is the caller's responsibility. */
  onSuccess?: (username: string) => void;
}

/**
 * Shared admin login form used by both admin-site and admin-app.
 *
 * Pure presentation — all business logic is injected via props. The caller
 * owns the login action and post-login navigation via `onSuccess`.
 */
export function LoginForm({
  login,
  rememberMe = false,
  onSuccess,
}: LoginFormProps) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [rememberMeChecked, setRememberMeChecked] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const effectiveRememberMe =
    rememberMe === "checkbox" ? rememberMeChecked : (rememberMe as boolean);

  async function handleSubmit(values: { username: string; password: string }) {
    setFormError(null);
    setLoading(true);
    try {
      await login(values.username, values.password, effectiveRememberMe);
      onSuccess?.(values.username);
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

  return (
    <Form
      values={{ username, password }}
      onSubmit={handleSubmit}
      className="flex flex-col gap-5"
    >
      <Field label="Username" htmlFor="username" name="username">
        <Input
          id="username"
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
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="current-password"
          placeholder="••••••••"
        />
      </Field>
      <Validator
        name="password"
        rule="required"
        message="Password is required."
      />

      {rememberMe === "checkbox" && (
        <label className="flex items-center gap-2 text-sm text-ink-2 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={rememberMeChecked}
            onChange={(e) => setRememberMeChecked(e.target.checked)}
            className="accent-accent"
          />
          Remember me for 30 days
        </label>
      )}

      {formError && (
        <p role="alert" className="text-sm text-danger">
          {formError}
        </p>
      )}
      <Button type="submit" disabled={loading} className="w-full">
        {loading ? "Signing in…" : "Log in"}
      </Button>
      <p className="text-xs text-ink-3">
        Credentials are set during initial daemon setup.
      </p>
    </Form>
  );
}
