import { useState } from "react";
import { useNavigate } from "react-router";
import { WardnetApiError } from "@wardnet/js";
import { Button } from "@wardnet/forge-web/button";
import { Field } from "@wardnet/forge-web/field";
import { Form, Validator } from "@wardnet/forge-web/form";
import { Input } from "@wardnet/forge-web/input";
import { useAuth } from "@wardnet/wardnet-web";

/**
 * Admin login page — rendered inside AuthLayout's branded hero.
 *
 * Uses the Forge `<Form>` + `<Validator>` pair: per-field rules are
 * declared inline and the form surfaces every failure on submit (not
 * just the first). AuthLayout already hosts a `<Card>`, so this page
 * renders only the form contents (no wrapper div, no inner card).
 */
export default function Login() {
  const navigate = useNavigate();
  const { login } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function handleSubmit(values: { username: string; password: string }) {
    setFormError(null);
    setLoading(true);
    try {
      await login(values.username, values.password);
      navigate("/");
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
    <Form values={{ username, password }} onSubmit={handleSubmit} className="flex flex-col gap-5">
      <Field label="Username" htmlFor="username" name="username">
        <Input
          id="username"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoComplete="username"
          placeholder="admin"
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
      <p className="text-center text-xs text-ink-3">
        Credentials are set during initial daemon setup.
      </p>
      <Button type="submit" disabled={loading} className="w-full">
        {loading ? "Signing in…" : "Log in"}
      </Button>
    </Form>
  );
}
