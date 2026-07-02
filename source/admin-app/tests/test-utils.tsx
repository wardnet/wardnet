import type { ReactElement, ReactNode } from "react";
import { render, renderHook, type RenderResult } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import type { Device, Tunnel } from "@wardnet/js";

function makeClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
}

/**
 * Render a component inside the providers every admin-app screen assumes:
 * a fresh React Query client (retries off so failed mutations settle
 * immediately in tests) and a MemoryRouter for `useNavigate`/`<Link>`.
 */
export function renderWithProviders(
  ui: ReactElement,
  { route = "/" }: { route?: string } = {},
): RenderResult {
  const queryClient = makeClient();
  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <MemoryRouter initialEntries={[route]}>{children}</MemoryRouter>
      </QueryClientProvider>
    );
  }
  return render(ui, { wrapper: Wrapper });
}

/** A wrapper factory for `renderHook` (QueryClientProvider + MemoryRouter). */
export function createWrapper() {
  const queryClient = makeClient();
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <MemoryRouter>{children}</MemoryRouter>
      </QueryClientProvider>
    );
  };
}

export { renderHook };

/** Build a Device with sensible defaults; override only what a test cares about. */
export function makeDevice(overrides: Partial<Device> = {}): Device {
  return {
    id: "dev-1",
    mac: "AA:BB:CC:DD:EE:FF",
    name: null,
    hostname: null,
    manufacturer: null,
    device_type: "unknown",
    first_seen: "2026-01-01T00:00:00Z",
    last_seen: "2026-01-01T00:00:00Z",
    last_ip: "10.232.1.10",
    admin_locked: false,
    zone_id: "zone-1",
    dns_capture_enabled: false,
    dns_capture_cap_count: 0,
    dns_capture_cap_days: 0,
    dhcp_status: "lease",
    current_rule: null,
    ...overrides,
  };
}

/** Build a Tunnel with sensible defaults; override only what a test cares about. */
export function makeTunnel(overrides: Partial<Tunnel> = {}): Tunnel {
  return {
    id: "tun-1",
    label: "US East",
    country_code: "US",
    provider: "mullvad",
    interface_name: "wg_ward0",
    endpoint: "1.2.3.4:51820",
    status: "up",
    last_handshake: "2026-01-01T00:00:00Z",
    bytes_tx: 1000,
    bytes_rx: 2000,
    created_at: "2026-01-01T00:00:00Z",
    override_default_dns: false,
    server_selector: null,
    resolved_server_name: null,
    endpoint_resolved_at: null,
    ...overrides,
  };
}
