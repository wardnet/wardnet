import type { ReactElement, ReactNode } from "react";
import { render, type RenderResult } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import type { Device, TunnelSummary } from "@wardnet/js";

/**
 * Wrapper factory for `renderHook`/`render` that supplies a fresh React
 * Query client with retries disabled — failed queries/mutations settle
 * on the first attempt so tests don't wait out the default backoff.
 */
export function createQueryWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
  };
}

/**
 * Render a component inside the providers the shared components assume:
 * a fresh React Query client (retries off) plus a MemoryRouter so the
 * `<Link>`-using components (e.g. RoutingSelector) render. `route` sets the
 * router's initial location for components that branch on it (InstallPrompt
 * only shows on the home route).
 */
export function renderWithProviders(
  ui: ReactElement,
  { route = "/" }: { route?: string } = {},
): RenderResult {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <MemoryRouter initialEntries={[route]}>{children}</MemoryRouter>
      </QueryClientProvider>
    );
  }
  return render(ui, { wrapper: Wrapper });
}

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
    connection_mode: "lan",
    ...overrides,
  };
}

/** Build a TunnelSummary with sensible defaults. */
export function makeTunnel(
  overrides: Partial<TunnelSummary> = {},
): TunnelSummary {
  return {
    id: "tun-1",
    label: "Tunnel 1",
    status: "up",
    country_code: "US",
    ...overrides,
  } as TunnelSummary;
}
