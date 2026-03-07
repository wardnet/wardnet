import { NavLink, Outlet } from "react-router";

const links = [
  { to: "/", label: "Dashboard" },
  { to: "/devices", label: "Devices" },
  { to: "/tunnels", label: "Tunnels" },
  { to: "/settings", label: "Settings" },
];

export default function Layout() {
  return (
    <div className="flex min-h-screen bg-gray-950 text-gray-100">
      <aside className="w-56 shrink-0 border-r border-gray-800 bg-gray-900 p-4">
        <h1 className="mb-6 text-xl font-bold tracking-tight">Wardnet</h1>
        <nav className="flex flex-col gap-1">
          {links.map((link) => (
            <NavLink
              key={link.to}
              to={link.to}
              end={link.to === "/"}
              className={({ isActive }) =>
                `rounded px-3 py-2 text-sm font-medium transition-colors ${
                  isActive
                    ? "bg-gray-800 text-white"
                    : "text-gray-400 hover:bg-gray-800/50 hover:text-gray-200"
                }`
              }
            >
              {link.label}
            </NavLink>
          ))}
        </nav>
      </aside>
      <main className="flex-1 p-6">
        <Outlet />
      </main>
    </div>
  );
}
