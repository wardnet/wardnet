import type { ReactElement, ReactNode } from "react";
import { render, type RenderResult } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import type { Device } from "@wardnet/js";

/**
 * Render a component inside the providers every admin-site page assumes:
 * a fresh React Query client (retries off so failed mutations settle
 * immediately in tests) and a MemoryRouter for `useNavigate`/`<Link>`.
 */
export function renderWithProviders(ui: ReactElement): RenderResult {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <MemoryRouter>{children}</MemoryRouter>
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
    manufacturer_source: null,
    is_randomized: false,
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
    // Unmanaged by default: that is what a freshly discovered device is, and
    // it keeps `managed: true` an explicit opt-in in the tests that need it.
    managed: false,
    ...overrides,
  };
}
