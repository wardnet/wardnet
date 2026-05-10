import { useState } from "react";
import { useNavigate } from "react-router";
import { WardnetApiError } from "@wardnet/js";
import { Button } from "@wardnet/forge-web/button";
import { Field } from "@wardnet/forge-web/field";
import { Input } from "@wardnet/forge-web/input";
import { useAuth } from "@/hooks/useAuth";

/**
 * Admin login page — rendered inside AuthLayout's branded hero.
 *
 * AuthLayout already hosts a Forge `<Card>`, so this page renders only the
 * form contents (no wrapper div, no inner card). Adding one here would
 * produce a double-card stack on `--bg`.
 */
export default function Login() {
  const navigate = useNavigate();
  const { login } = useAuth();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setLoading(true);

    try {
      await login(username, password);
      navigate("/");
    } catch (err) {
      if (err instanceof WardnetApiError && err.status === 401) {
        setError("Invalid username or password.");
      } else {
        setError("Unable to connect to daemon. Is it running?");
      }
    } finally {
      setLoading(false);
    }
  }

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-5">
      <Field label="Username" htmlFor="username">
        <Input
          id="username"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoComplete="username"
          placeholder="admin"
          required
        />
      </Field>
      <Field label="Password" htmlFor="password">
        <Input
          id="password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="current-password"
          placeholder="••••••••"
          required
        />
      </Field>
      {error && <p className="text-sm text-danger">{error}</p>}
      <p className="text-center text-xs text-ink-3">
        Credentials are set during initial daemon setup.
      </p>
      <Button type="submit" disabled={loading} className="w-full">
        {loading ? "Signing in…" : "Log in"}
      </Button>
    </form>
  );
}
