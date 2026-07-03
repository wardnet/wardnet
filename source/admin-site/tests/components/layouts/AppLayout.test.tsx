import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useAuth } = vi.hoisted(() => ({ useAuth: vi.fn() }));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useAuth };
});

// Stub the compound children (owned/tested elsewhere) so this test isolates
// the layout shell.
vi.mock("@/components/compound/Sidebar", () => ({
  Sidebar: () => <nav>sidebar</nav>,
}));
vi.mock("@/components/compound/MobileMenu", () => ({
  MobileMenu: () => <button>menu</button>,
}));
vi.mock("@/components/compound/ConnectionBanner", () => ({
  ConnectionBanner: () => <div>connection-banner</div>,
}));
vi.mock("@/components/compound/UncleanShutdownBanner", () => ({
  UncleanShutdownBanner: () => <div>shutdown-banner</div>,
}));

import { AppLayout } from "@/components/layouts/AppLayout";

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<AppLayout />}>
          <Route path="*" element={<div>page body</div>} />
        </Route>
      </Routes>
    </MemoryRouter>,
  );
}

describe("AppLayout", () => {
  beforeEach(() => vi.clearAllMocks());

  it("derives the breadcrumb from the path and renders admin chrome", () => {
    useAuth.mockReturnValue({ isAdmin: true });
    renderAt("/devices");
    expect(screen.getByText("Devices")).toBeInTheDocument();
    expect(screen.getByText("menu")).toBeInTheDocument();
    expect(screen.getByText("shutdown-banner")).toBeInTheDocument();
    expect(screen.getByText("page body")).toBeInTheDocument();
  });

  it("shows the Dashboard crumb at root and hides admin-only chrome for non-admins", () => {
    useAuth.mockReturnValue({ isAdmin: false });
    renderAt("/");
    expect(screen.getByText("Dashboard")).toBeInTheDocument();
    expect(screen.queryByText("menu")).not.toBeInTheDocument();
    expect(screen.queryByText("shutdown-banner")).not.toBeInTheDocument();
  });
});
