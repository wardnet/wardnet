import { useEffect } from "react";
import { Routes, Route, Navigate } from "react-router";
import { AppLayout } from "@/components/layouts/AppLayout";
import { AuthLayout } from "@/components/layouts/AuthLayout";
import { useAuth } from "@/hooks/useAuth";
import { useTheme } from "@/hooks/useTheme";
import { useSetupStatus } from "@/hooks/useSetup";
import Dashboard from "@/pages/Dashboard";
import Devices from "@/pages/Devices";
import Tunnels from "@/pages/Tunnels";
import Settings from "@/pages/Settings";
import Dhcp from "@/pages/Dhcp";
import Dns from "@/pages/Dns";
import AdBlocking from "@/pages/AdBlocking";
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

/** Redirects to /setup if initial setup hasn't been completed. */
function SetupGuard({ children }: { children: React.ReactNode }) {
  const { data, isLoading } = useSetupStatus();

  if (isLoading) return null;
  if (data && !data.setup_completed) return <Navigate to="/setup" replace />;
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
            <AppLayout />
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
          path="tunnels"
          element={
            <AdminRoute>
              <Tunnels />
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
          path="ad-blocking"
          element={
            <AdminRoute>
              <AdBlocking />
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
      </Route>
      <Route path="*" element={<NotFound />} />
    </Routes>
  );
}
