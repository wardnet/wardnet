import { useState } from "react";
import { WardnetApiError } from "@wardnet/js";
import { Button } from "@wardnet/ui";
import { Field } from "@wardnet/ui";
import { Form, Validator } from "@wardnet/ui";
import { Input } from "@wardnet/ui";
import { Text } from "@wardnet/ui";

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
  /**
   * Called when the "Remember me" checkbox changes.
   *
   * Exists so a caller can apply the *same* intent to a federated sign-in
   * started next to this form. `remember_me` is parked on the OAuth ceremony
   * at its start and cannot be raised afterwards, so a checkbox whose value
   * never left this component would silently give federated users a short
   * session they could not upgrade.
   */
  onRememberMeChange?: (rememberMe: boolean) => void;
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
  onRememberMeChange,
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
      className="login-form"
    >
      <Field label="Username" htmlFor="username" name="username">
        <Input
          id="username"
          data-testid="login-username"
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
          data-testid="login-password"
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
        <label className="login-form__remember">
          <input
            type="checkbox"
            checked={rememberMeChecked}
            onChange={(e) => {
              setRememberMeChecked(e.target.checked);
              onRememberMeChange?.(e.target.checked);
            }}
            className="login-form__checkbox"
          />
          Remember me for 30 days
        </label>
      )}

      {formError && (
        <Text
          as="p"
          role="alert"
          data-testid="login-error"
          className="login-form__error"
        >
          {formError}
        </Text>
      )}
      <Button
        type="submit"
        disabled={loading}
        data-testid="login-submit"
        className="login-form__submit"
      >
        {loading ? "Signing in…" : "Log in"}
      </Button>
      <Text as="p" className="login-form__hint">
        Credentials are set during initial daemon setup.
      </Text>
    </Form>
  );
}
