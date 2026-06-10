import { useEffect, useRef } from "react";
import { Routes, Route, Navigate } from "react-router";
import { AppLayout } from "@/components/layouts/AppLayout";
import { ErrorBoundary } from "@/components/core/ErrorBoundary";
import { AuthLayout } from "@/components/layouts/AuthLayout";
import { useAuth } from "@wardnet/web";
import { useTheme } from "@/hooks/useTheme";
import { useSetupStatus } from "@wardnet/web";
import Dashboard from "@/pages/Dashboard";
import Devices from "@/pages/Devices";
import DeviceDetail from "@/pages/DeviceDetail";
import Tunnels from "@/pages/Tunnels";
import TunnelDetail from "@/pages/TunnelDetail";
import Settings from "@/pages/Settings";
import Power from "@/pages/Power";
import Backups from "@/pages/Backups";
import Dhcp from "@/pages/Dhcp";
import Dns from "@/pages/Dns";
import DnsLogs from "@/pages/DnsLogs";
import DnsFilter from "@/pages/DnsFilter";
import DnsFilterProfile from "@/pages/DnsFilterProfile";
import DnsFilterProfileNew from "@/pages/DnsFilterProfileNew";
import MyDevice from "@/pages/MyDevice";
import Login from "@/pages/Login";
import Setup from "@/pages/Setup";
import NotFound from "@/pages/NotFound";

/** Route guard that redirects to /login if not admin. */
function AdminRoute({ children }: { children: React.ReactNode }) {
  const { isAdmin, isChecking } = useAuth();

  if (isChecking) return null;
  if (!isAdmin) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

/**
 * Redirects to /setup whenever the wizard hasn't finished.
 *
 * `setup_completed` is now derived from `wizard_step === "completed"`,
 * so the boolean check still works for clients that haven't been
 * updated, but the multi-step wizard re-routes the operator back to
 * /setup after each step until they reach the final confirmation.
 *
 * When setup transitions to completed and a `wardnet_returnTo` value
 * is stored in sessionStorage (written by main.tsx on load), the guard
 * redirects back to that surface instead of staying on the admin-site.
 */
function SetupGuard({ children }: { children: React.ReactNode }) {
  const { data, isLoading } = useSetupStatus();
  const didRedirectRef = useRef(false);

  useEffect(() => {
    if (data?.wizard_step === "completed" && !didRedirectRef.current) {
      const returnTo = sessionStorage.getItem("wardnet_returnTo");
      if (returnTo) {
        didRedirectRef.current = true;
        sessionStorage.removeItem("wardnet_returnTo");
        window.location.replace(returnTo);
      }
    }
  }, [data?.wizard_step]);

  if (isLoading) return null;
  if (data && data.wizard_step !== "completed")
    return <Navigate to="/setup" replace />;
  return <>{children}</>;
}

/** Renders admin dashboard or self-service page based on auth state. */
function Home() {
  const { isAdmin, isChecking } = useAuth();

  if (isChecking) return null;
  return isAdmin ? <Dashboard /> : <MyDevice />;
}

export default function App() {
  useTheme();

  const { checkAuth } = useAuth();

  useEffect(() => {
    checkAuth();
  }, [checkAuth]);

  return (
    <Routes>
      <Route element={<AuthLayout />}>
        <Route path="setup" element={<Setup />} />
        <Route
          path="login"
          element={
            <SetupGuard>
              <Login />
            </SetupGuard>
          }
        />
      </Route>
      <Route
        element={
          <SetupGuard>
            <ErrorBoundary>
              <AppLayout />
            </ErrorBoundary>
          </SetupGuard>
        }
      >
        <Route index element={<Home />} />
        <Route
          path="devices"
          element={
            <AdminRoute>
              <Devices />
            </AdminRoute>
          }
        />
        <Route
          path="devices/:id"
          element={
            <AdminRoute>
              <DeviceDetail />
            </AdminRoute>
          }
        />
        <Route
          path="tunnels"
          element={
            <AdminRoute>
              <Tunnels />
            </AdminRoute>
          }
        />
        <Route
          path="tunnels/:id"
          element={
            <AdminRoute>
              <TunnelDetail />
            </AdminRoute>
          }
        />
        <Route
          path="dhcp"
          element={
            <AdminRoute>
              <Dhcp />
            </AdminRoute>
          }
        />
        <Route
          path="dns"
          element={
            <AdminRoute>
              <Dns />
            </AdminRoute>
          }
        />
        <Route
          path="dns/logs"
          element={
            <AdminRoute>
              <DnsLogs />
            </AdminRoute>
          }
        />
        <Route
          path="dns/filter"
          element={
            <AdminRoute>
              <DnsFilter />
            </AdminRoute>
          }
        />
        <Route
          path="dns/filter/profiles/new"
          element={
            <AdminRoute>
              <DnsFilterProfileNew />
            </AdminRoute>
          }
        />
        <Route
          path="dns/filter/profiles/:id"
          element={
            <AdminRoute>
              <DnsFilterProfile />
            </AdminRoute>
          }
        />
        <Route
          path="settings"
          element={
            <AdminRoute>
              <Settings />
            </AdminRoute>
          }
        />
        <Route
          path="power"
          element={
            <AdminRoute>
              <Power />
            </AdminRoute>
          }
        />
        <Route
          path="backups"
          element={
            <AdminRoute>
              <Backups />
            </AdminRoute>
          }
        />
      </Route>
      <Route path="*" element={<NotFound />} />
    </Routes>
  );
}
