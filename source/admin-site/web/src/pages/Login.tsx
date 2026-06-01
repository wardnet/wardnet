import { useNavigate } from "react-router";
import { LoginForm } from "@wardnet/wardnet-web";

export default function Login() {
  const navigate = useNavigate();
  return <LoginForm rememberMe="checkbox" onSuccess={() => navigate("/")} />;
}
