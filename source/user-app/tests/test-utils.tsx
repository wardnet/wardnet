import type { ReactElement, ReactNode } from "react";
import { render, type RenderResult } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import type { Device } from "@wardnet/js";

/**
 * Fresh React Query client with retries disabled so failed queries/mutations
 * settle on the first attempt (no default backoff wait in tests).
 */
export function makeQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
}

/**
 * Render a component inside the providers every User-PWA page assumes: a fresh
 * React Query client and a MemoryRouter for `<Link>`/`useLocation`.
 */
export function renderWithProviders(
  ui: ReactElement,
  { route = "/" }: { route?: string } = {},
): RenderResult {
  const queryClient = makeQueryClient();
  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <MemoryRouter initialEntries={[route]}>{children}</MemoryRouter>
      </QueryClientProvider>
    );
  }
  return render(ui, { wrapper: Wrapper });
}

/** Build a Device with sensible defaults; override only what a test needs. */
export function makeDevice(overrides: Partial<Device> = {}): Device {
  return {
    id: "dev-1",
    mac: "AA:BB:CC:DD:EE:FF",
    name: "My Laptop",
    hostname: null,
    manufacturer: null,
    device_type: "laptop",
    first_seen: "2026-01-01T00:00:00Z",
    last_seen: "2026-01-01T00:00:00Z",
    last_ip: "10.232.1.10",
    admin_locked: false,
    zone_id: "zone-1",
    dns_capture_enabled: true,
    dns_capture_cap_count: 10000,
    dns_capture_cap_days: 7,
    dhcp_status: "lease",
    current_rule: null,
    connection_mode: "lan",
    ...overrides,
  };
}

/** Wrapper factory for `renderHook`. */
export function createQueryWrapper() {
  const queryClient = makeQueryClient();
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
  };
}
