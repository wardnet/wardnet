import { useEffect, useState } from "react";
import { Navigate, Outlet, Route, Routes, useNavigate } from "react-router";
import { useAuth, useSetupStatus } from "@wardnet/wardnet-web";
import { useBiometric } from "@/hooks/useBiometric";
import { OnlineStatusProvider } from "@/context/OnlineStatusContext";
import { BiometricGate } from "@/components/BiometricGate";
import { AppLayout } from "@/layouts/AppLayout";
import { AuthLayout } from "@/layouts/AuthLayout";
import Login from "@/pages/Login";
import Dashboard from "@/pages/Dashboard";
import Devices from "@/pages/Devices";
import Tunnels from "@/pages/Tunnels";
import Dns from "@/pages/Dns";
import System from "@/pages/System";

/** Redirects to /login if the user has no active session. */
function AdminRoute() {
  const { isAdmin, isChecking } = useAuth();

  if (isChecking) return null;
  if (!isAdmin) return <Navigate to="/login" replace />;
  return <Outlet />;
}

/**
 * Blocks navigation until setup is complete.
 *
 * When the wizard hasn't finished, shows a full-screen interstitial that
 * redirects to admin-site with a `returnTo` pointing back here.
 */
function SetupGuard({ children }: { children: React.ReactNode }) {
  const { data, isLoading } = useSetupStatus();

  if (isLoading) return null;

  if (data && data.wizard_step !== "completed") {
    const adminSiteUrl = `${window.location.origin}/admin/?returnTo=/admin-app/`;
    return (
      <div className="flex min-h-screen flex-col items-center justify-center gap-6 bg-bg px-4 text-ink">
        <p className="text-lg font-semibold">Initial setup required</p>
        <p className="text-sm text-ink-3 text-center max-w-xs">
          Complete the Wardnet setup wizard before using the admin app.
        </p>
        <a
          href={adminSiteUrl}
          className="rounded-md bg-accent px-4 py-2 text-sm font-medium text-accent-ink"
        >
          Go to setup
        </a>
      </div>
    );
  }

  return <>{children}</>;
}

export default function App() {
  const { checkAuth, refresh } = useAuth();
  const biometric = useBiometric();
  const navigate = useNavigate();

  const [unlocked, setUnlocked] = useState(false);
  const [biometricReady, setBiometricReady] = useState(false);

  useEffect(() => {
    async function boot() {
      // Probe auth status and slide any remember-me session forward before the
      // biometric gate renders. The gate is a client-side presence check only
      // (see useBiometric.ts); the server session remains the enforced auth
      // boundary. Awaiting both ensures AdminRoute has accurate state when the
      // gate resolves.
      await Promise.all([checkAuth(), refresh()]);
      setBiometricReady(true);
    }
    boot();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const showBiometricGate =
    biometricReady && biometric.isRegistered() && !unlocked;

  if (!biometricReady) return null;

  if (showBiometricGate) {
    return (
      <BiometricGate
        onSuccess={() => setUnlocked(true)}
        onUsePassword={() => {
          setUnlocked(true);
          navigate("/login", { replace: true });
        }}
      />
    );
  }

  return (
    <OnlineStatusProvider>
    <SetupGuard>
      <Routes>
        <Route element={<AuthLayout />}>
          <Route path="login" element={<Login onUnlock={() => setUnlocked(true)} />} />
        </Route>
        <Route element={<AdminRoute />}>
          <Route element={<AppLayout />}>
            <Route index element={<Dashboard />} />
            <Route path="devices" element={<Devices />} />
            <Route path="tunnels" element={<Tunnels />} />
            <Route path="dns" element={<Dns />} />
            <Route path="system" element={<System />} />
          </Route>
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </SetupGuard>
    </OnlineStatusProvider>
  );
}
