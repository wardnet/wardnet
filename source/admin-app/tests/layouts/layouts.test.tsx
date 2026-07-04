import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useDaemonStatus, useInstallPrompt, onlineCtx } = vi.hoisted(() => ({
  useDaemonStatus: vi.fn(),
  useInstallPrompt: vi.fn(),
  onlineCtx: {
    value: {
      isOnline: true,
      isDaemonReachable: true,
      showingLastKnownState: false,
    },
  },
}));
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useDaemonStatus, useInstallPrompt };
});
vi.mock("@/context/OnlineStatusContext", () => ({
  useOnlineStatusContext: () => onlineCtx.value,
}));

import { AppLayout } from "@/layouts/AppLayout";
import { AuthLayout } from "@/layouts/AuthLayout";

function renderRouted(element: React.ReactElement, child: string) {
  const client = new QueryClient();
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route element={element}>
            <Route index element={<div>{child}</div>} />
          </Route>
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("AppLayout", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useInstallPrompt.mockReturnValue({
      isInstallable: false,
      promptInstall: vi.fn(),
    });
    onlineCtx.value = {
      isOnline: true,
      isDaemonReachable: true,
      showingLastKnownState: false,
    };
  });

  it("renders header, tabs and the routed outlet when online", () => {
    useDaemonStatus.mockReturnValue({ data: { version: "9.9.9" } });
    renderRouted(<AppLayout />, "PAGE-CONTENT");
    expect(screen.getByText("PAGE-CONTENT")).toBeInTheDocument();
    expect(screen.getByTestId("tab-home")).toBeInTheDocument();
    // Online → no connection banner.
    expect(
      screen.queryByText(/No connection to wardnet daemon/i),
    ).not.toBeInTheDocument();
  });

  it("shows the offline banner when not online", () => {
    useDaemonStatus.mockReturnValue({ data: null });
    onlineCtx.value = {
      isOnline: false,
      isDaemonReachable: false,
      showingLastKnownState: true,
    };
    renderRouted(<AppLayout />, "X");
    expect(
      screen.getByText(/No connection to wardnet daemon/i),
    ).toBeInTheDocument();
  });

  it("shows the reconnecting banner when online but daemon unreachable", () => {
    useDaemonStatus.mockReturnValue({ data: null });
    onlineCtx.value = {
      isOnline: true,
      isDaemonReachable: false,
      showingLastKnownState: false,
    };
    renderRouted(<AppLayout />, "X");
    expect(
      screen.getByText(/Reconnecting to wardnet daemon/i),
    ).toBeInTheDocument();
  });
});

describe("AuthLayout", () => {
  it("renders the tagline and the routed outlet", () => {
    render(
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route element={<AuthLayout />}>
            <Route index element={<div>LOGIN-FORM</div>} />
          </Route>
        </Routes>
      </MemoryRouter>,
    );
    expect(
      screen.getByText("Sign in to manage your network"),
    ).toBeInTheDocument();
    expect(screen.getByText("LOGIN-FORM")).toBeInTheDocument();
  });
});
