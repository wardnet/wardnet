import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "@/components/compound/Sidebar";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useAuth: vi.fn(),
    useDaemonStatus: vi.fn(),
    useUpdateStatus: vi.fn(),
  };
});

import { useAuth, useDaemonStatus, useUpdateStatus } from "@wardnet/web";

const mockAuth = vi.mocked(useAuth);
const mockDaemon = vi.mocked(useDaemonStatus);
const mockUpdate = vi.mocked(useUpdateStatus);
const logout = vi.fn();

beforeEach(() => {
  logout.mockClear();
  mockAuth.mockReturnValue({ isAdmin: true, logout } as never);
  mockDaemon.mockReturnValue({
    data: { version: "1.2.3", reachable: true },
  } as never);
  mockUpdate.mockReturnValue({
    data: { status: { update_available: false, latest_version: null } },
  } as never);
});

describe("Sidebar", () => {
  it("renders admin navigation, version, and admin footer links", () => {
    renderWithProviders(<Sidebar />);
    expect(screen.getByTestId("nav-dashboard")).toBeInTheDocument();
    expect(screen.getByTestId("nav-devices")).toBeInTheDocument();
    expect(screen.getByTestId("nav-power")).toBeInTheDocument();
    expect(screen.getByText("v1.2.3")).toBeInTheDocument();
    expect(screen.getByText("API docs")).toBeInTheDocument();
    expect(screen.getByText("Sign out")).toBeInTheDocument();
  });

  it("shows the update banner when an update is available", () => {
    mockUpdate.mockReturnValue({
      data: { status: { update_available: true, latest_version: "9.9.9" } },
    } as never);
    renderWithProviders(<Sidebar />);
    expect(screen.getByText("Update available: v9.9.9")).toBeInTheDocument();
  });

  it("logs out and navigates when Sign out is clicked", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    renderWithProviders(<Sidebar onNavigate={onNavigate} />);
    await user.click(screen.getByText("Sign out"));
    expect(logout).toHaveBeenCalledOnce();
    expect(onNavigate).toHaveBeenCalledOnce();
  });

  it("renders the sign-in link and no nav for non-admins", () => {
    mockAuth.mockReturnValue({ isAdmin: false, logout } as never);
    renderWithProviders(<Sidebar />);
    expect(screen.getByText("Sign in as admin")).toBeInTheDocument();
    expect(screen.queryByTestId("nav-dashboard")).not.toBeInTheDocument();
  });

  it("omits the version caption when the daemon reports none", () => {
    mockDaemon.mockReturnValue({ data: { reachable: false } } as never);
    renderWithProviders(<Sidebar />);
    expect(screen.queryByText(/^v/)).not.toBeInTheDocument();
  });
});
