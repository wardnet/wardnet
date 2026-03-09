import { useState } from "react";
import { useNavigate } from "react-router";
import { WardnetApiError } from "@wardnet/js";
import { Button } from "@/components/core/ui/button";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { useAuth } from "@/hooks/useAuth";

/** Admin login page — rendered inside AuthLayout's branded hero. */
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
    <div className="rounded-2xl bg-white/95 p-6 shadow-2xl backdrop-blur-sm dark:bg-card/95">
      <form onSubmit={handleSubmit} className="flex flex-col gap-5">
        <div className="flex flex-col gap-2">
          <Label htmlFor="username" className="text-foreground/70">
            Username
          </Label>
          <Input
            id="username"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="username"
            placeholder="admin"
            required
            className="h-12"
          />
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="password" className="text-foreground/70">
            Password
          </Label>
          <Input
            id="password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="current-password"
            placeholder="••••••••"
            required
            className="h-12"
          />
        </div>
        {error && <p className="text-sm text-destructive">{error}</p>}
        <p className="text-center text-xs text-muted-foreground">
          Credentials are set during initial daemon setup.
        </p>
        <Button
          type="submit"
          disabled={loading}
          className="h-12 w-full bg-[oklch(0.22_0.12_275)] text-base font-semibold tracking-wide text-white uppercase hover:bg-[oklch(0.28_0.12_275)] dark:bg-primary dark:hover:bg-primary/90"
        >
          {loading ? "Signing in..." : "Log in"}
        </Button>
      </form>
    </div>
  );
}
