import { Navigate, Route, Routes } from "react-router";
import { OnlineStatusProvider } from "@/context/OnlineStatusContext";
import { AppLayout } from "@/layouts/AppLayout";
import Home from "@/pages/Home";
import Stats from "@/pages/Stats";
import Settings from "@/pages/Settings";

/**
 * User PWA shell.
 *
 * The User PWA is the device-keyed, non-admin self-service surface (see
 * CONTEXT.md). Identity is the device on the LAN — there is no login or admin
 * session, so the shell wires only the React Query client, the online-status
 * context, and the mobile layout. Each tab is a placeholder until its feature
 * stage (#590–#594) fills it in.
 */
export default function App() {
  return (
    <OnlineStatusProvider>
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
