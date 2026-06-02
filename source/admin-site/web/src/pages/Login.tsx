import { useNavigate } from "react-router";
import { LoginForm, useAuthStore } from "@wardnet/wardnet-web";

export default function Login() {
  const navigate = useNavigate();
  const { login } = useAuthStore();
  return <LoginForm login={login} rememberMe="checkbox" onSuccess={() => navigate("/")} />;
}
