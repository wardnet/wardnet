import { Navigate, Route, Routes } from "react-router";
import { OnlineStatusProvider } from "@/context/OnlineStatusContext";
import { AppLayout } from "@/layouts/AppLayout";
import Home from "@/pages/Home";
import Stats from "@/pages/Stats";
import Settings from "@/pages/Settings";
import { useDnsEventsSync } from "@/hooks/useDnsEventsSync";

/**
 * User PWA shell.
 *
 * The User PWA is the device-keyed, non-admin self-service surface (see
 * CONTEXT.md). Identity is the device on the LAN — there is no login or admin
 * session, so the shell wires only the React Query client, the online-status
 * context, and the mobile layout. Routing mounts the Home, Stats and Settings
 * tabs, with any unknown path redirected back to Home.
 */
function DnsSync() {
  useDnsEventsSync();
  return null;
}

export default function App() {
  return (
    <OnlineStatusProvider>
      <DnsSync />
      <Routes>
        <Route element={<AppLayout />}>
          <Route index element={<Home />} />
          <Route path="stats" element={<Stats />} />
          <Route path="settings" element={<Settings />} />
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </OnlineStatusProvider>
  );
}
