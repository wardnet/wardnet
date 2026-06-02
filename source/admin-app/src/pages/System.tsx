import { useAuth } from "@wardnet/wardnet-web";
import { useBiometric } from "@/hooks/useBiometric";
import { useNavigate } from "react-router";

export default function System() {
  const { logout } = useAuth();
  const biometric = useBiometric();
  const navigate = useNavigate();

  function handleLogout() {
    logout();
    biometric.unregister();
    navigate("/login", { replace: true });
  }

  return (
    <div className="flex flex-col gap-4 p-4">
      <h1 className="text-xl font-semibold text-ink">System</h1>
      <p className="text-sm text-ink-3">Coming in a subsequent issue.</p>
      <button
        onClick={handleLogout}
        className="self-start rounded-md border border-danger px-4 py-2 text-sm font-medium text-danger"
      >
        Log out
      </button>
    </div>
  );
}
