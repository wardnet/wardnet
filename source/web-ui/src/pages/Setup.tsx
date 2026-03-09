import { useState } from "react";
import { useNavigate } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { WardnetApiError } from "@wardnet/js";
import { Button } from "@/components/core/ui/button";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { useSetup } from "@/hooks/useSetup";
import { useAuth } from "@/hooks/useAuth";

/** Initial setup wizard — creates the first admin account. */
export default function Setup() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const setup = useSetup();
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
      // Auto-login with the credentials just created.
      await login(username, password);
      // Ensure the setup status cache is updated before navigating,
      // so SetupGuard sees setup_completed=true and doesn't redirect back.
      await queryClient.refetchQueries({ queryKey: ["setup", "status"] });
      navigate("/");
    } catch (err) {
      if (err instanceof WardnetApiError && err.status === 409) {
        setError("Setup has already been completed.");
      } else if (err instanceof WardnetApiError) {
        setError(err.body.error);
      } else {
        setError("Unable to connect to daemon. Is it running?");
      }
    }
  }

  return (
    <div className="rounded-2xl bg-white/95 p-6 shadow-2xl backdrop-blur-sm dark:bg-card/95">
      <div className="mb-5 flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-foreground">Create Admin Account</h2>
        <p className="text-sm text-muted-foreground">
          Set up your administrator credentials to get started.
        </p>
      </div>
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
            autoComplete="new-password"
            placeholder="At least 8 characters"
            required
            className="h-12"
          />
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="confirm-password" className="text-foreground/70">
            Confirm Password
          </Label>
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
        </div>
        {error && <p className="text-sm text-destructive">{error}</p>}
        <Button
          type="submit"
          disabled={setup.isPending}
          className="h-12 w-full bg-[oklch(0.22_0.12_275)] text-base font-semibold tracking-wide text-white uppercase hover:bg-[oklch(0.28_0.12_275)] dark:bg-primary dark:hover:bg-primary/90"
        >
          {setup.isPending ? "Creating account..." : "Create Account"}
        </Button>
      </form>
    </div>
  );
}
