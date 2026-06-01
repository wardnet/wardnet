import { useAuth } from "@wardnet/wardnet-web";
import { useBiometric } from "@/hooks/useBiometric";
import { useNavigate } from "react-router";

/**
 * Dashboard stub — placeholder until subsequent issues flesh out the content.
 *
 * The logout button here implements the full reset: server session +
 * biometric credential + navigate to /login.
 */
export default function Dashboard() {
  const { logout } = useAuth();
  const biometric = useBiometric();
  const navigate = useNavigate();

  function handleLogout() {
    logout();
    biometric.unregister();
    navigate("/login", { replace: true });
  }

  return (
    <div className="flex flex-col gap-6 p-4">
      <h1 className="text-xl font-semibold text-ink">Dashboard</h1>
      <p className="text-sm text-ink-3">More content coming in subsequent issues.</p>
      <button
        onClick={handleLogout}
        className="self-start rounded-md border border-danger px-4 py-2 text-sm font-medium text-danger"
      >
        Log out
      </button>
    </div>
  );
}
