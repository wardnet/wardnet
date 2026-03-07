import { Routes, Route } from "react-router";
import Layout from "./components/Layout";
import Dashboard from "./pages/Dashboard";
import Devices from "./pages/Devices";
import Tunnels from "./pages/Tunnels";
import Settings from "./pages/Settings";

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Dashboard />} />
        <Route path="devices" element={<Devices />} />
        <Route path="tunnels" element={<Tunnels />} />
        <Route path="settings" element={<Settings />} />
      </Route>
    </Routes>
  );
}
