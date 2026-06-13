import { useLocation, useNavigate } from "react-router";
import { LoginForm, useAuthStore } from "@wardnet/web";

export default function Login() {
  const navigate = useNavigate();
  const location = useLocation();
  const { login } = useAuthStore();
  // AdminRoute stashes the attempted path in location.state.from when it
  // bounces an unauthenticated deep-link here; return there after login,
  // falling back to the dashboard.
  const from = (location.state as { from?: string } | null)?.from ?? "/";
  return (
    <LoginForm
      login={login}
      rememberMe="checkbox"
      onSuccess={() => navigate(from)}
    />
  );
}
