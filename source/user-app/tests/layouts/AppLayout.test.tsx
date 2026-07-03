import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router";

import { AppLayout } from "../../src/layouts/AppLayout";
import { OnlineStatusProvider } from "../../src/context/OnlineStatusContext";

const { useOnlineStatus, useDaemonStatus, useInstallPrompt } = vi.hoisted(
  () => ({
    useOnlineStatus: vi.fn(),
    useDaemonStatus: vi.fn(),
    useInstallPrompt: vi.fn(),
  }),
);

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useOnlineStatus, useDaemonStatus, useInstallPrompt };
});

function renderLayout() {
  const qc = new QueryClient();
  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={["/"]}>
          <OnlineStatusProvider>{children}</OnlineStatusProvider>
        </MemoryRouter>
      </QueryClientProvider>
    );
  }
  return (
    <Wrapper>
      <Routes>
        <Route element={<AppLayout />}>
          <Route index element={<div>home body</div>} />
        </Route>
      </Routes>
    </Wrapper>
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  useDaemonStatus.mockReturnValue({ data: { version: "1.2.3" } });
  useInstallPrompt.mockReturnValue({
    isInstallable: false,
    promptInstall: vi.fn(),
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("AppLayout", () => {
  it("renders the outlet, tab bar and no banner when online", () => {
    useOnlineStatus.mockReturnValue({
      isOnline: true,
      isDaemonReachable: true,
      showingLastKnownState: false,
    });
    render(renderLayout());
    expect(screen.getByText("home body")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Home/ })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows the offline banner when not online", () => {
    useOnlineStatus.mockReturnValue({
      isOnline: false,
      isDaemonReachable: false,
      showingLastKnownState: true,
    });
    render(renderLayout());
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("shows the reconnecting banner when the daemon is unreachable", () => {
    useOnlineStatus.mockReturnValue({
      isOnline: true,
      isDaemonReachable: false,
      showingLastKnownState: false,
    });
    render(renderLayout());
    expect(
      screen.getByText(/Reconnecting to wardnet daemon/),
    ).toBeInTheDocument();
  });
});
