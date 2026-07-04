import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";

import App from "../src/App";

const {
  useOnlineStatus,
  useDaemonStatus,
  useInstallPrompt,
  useMyDevice,
  useSetMyRule,
} = vi.hoisted(() => ({
  useOnlineStatus: vi.fn(),
  useDaemonStatus: vi.fn(),
  useInstallPrompt: vi.fn(),
  useMyDevice: vi.fn(),
  useSetMyRule: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useOnlineStatus,
    useDaemonStatus,
    useInstallPrompt,
    useMyDevice,
    useSetMyRule,
  };
});

// The DNS sync hook opens an SSE stream + IndexedDB — irrelevant to routing.
vi.mock("../src/hooks/useDnsEventsSync", () => ({ useDnsEventsSync: vi.fn() }));
vi.mock("@/hooks/useIpGeolocation", () => ({
  useIpGeolocation: () => ({
    data: undefined,
    isLoading: true,
    isError: false,
    isFetching: false,
    refetch: vi.fn(),
  }),
}));
vi.mock("react-leaflet", () => ({
  MapContainer: ({ children }: { children?: ReactNode }) => (
    <div>{children}</div>
  ),
  TileLayer: () => null,
  Marker: () => null,
  useMapEvents: () => ({ setView: vi.fn(), getZoom: () => 8 }),
}));

function renderApp(route: string) {
  const qc = new QueryClient();
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[route]}>
        <App />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  useOnlineStatus.mockReturnValue({
    isOnline: true,
    isDaemonReachable: true,
    showingLastKnownState: false,
  });
  useDaemonStatus.mockReturnValue({ data: { version: "1.0.0" } });
  useInstallPrompt.mockReturnValue({
    isInstallable: false,
    promptInstall: vi.fn(),
  });
  useMyDevice.mockReturnValue({ data: undefined, isLoading: true });
  useSetMyRule.mockReturnValue({
    mutateAsync: vi.fn(),
    isPending: false,
    isError: false,
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("App", () => {
  it("renders the Home route inside the layout at index", () => {
    renderApp("/");
    // Home shows the loading text while the device query is pending.
    expect(screen.getByText("Loading…")).toBeInTheDocument();
    // The tab bar (from the layout) is present.
    expect(screen.getByRole("link", { name: /Home/ })).toBeInTheDocument();
  });

  it("redirects unknown routes to the index", () => {
    renderApp("/does-not-exist");
    expect(screen.getByRole("link", { name: /Home/ })).toBeInTheDocument();
  });
});
