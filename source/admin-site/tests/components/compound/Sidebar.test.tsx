import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "@/components/compound/Sidebar";
import { renderWithProviders } from "../../test-utils";

const onLogout = vi.fn();

function sidebarProps(over: Partial<Parameters<typeof Sidebar>[0]> = {}) {
  return {
    isAdmin: true,
    onLogout,
    version: "1.2.3" as string | undefined,
    connectionLoading: false,
    connected: true,
    updateAvailable: false,
    latestVersion: null as string | null,
    ...over,
  };
}

beforeEach(() => {
  onLogout.mockReset();
  onLogout.mockResolvedValue(true);
});

describe("Sidebar", () => {
  it("renders admin navigation, version, and admin footer links", () => {
    renderWithProviders(<Sidebar {...sidebarProps()} />);
    expect(screen.getByTestId("nav-dashboard")).toBeInTheDocument();
    expect(screen.getByTestId("nav-devices")).toBeInTheDocument();
    expect(screen.getByTestId("nav-power")).toBeInTheDocument();
    expect(screen.getByText("v1.2.3")).toBeInTheDocument();
    expect(screen.getByText("API docs")).toBeInTheDocument();
    expect(screen.getByText("Sign out")).toBeInTheDocument();
  });

  it("shows the update banner when an update is available", () => {
    renderWithProviders(
      <Sidebar
        {...sidebarProps({ updateAvailable: true, latestVersion: "9.9.9" })}
      />,
    );
    expect(screen.getByText("Update available: v9.9.9")).toBeInTheDocument();
  });

  it("logs out and navigates when Sign out is clicked", async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    renderWithProviders(
      <Sidebar {...sidebarProps()} onNavigate={onNavigate} />,
    );
    await user.click(screen.getByText("Sign out"));
    expect(onLogout).toHaveBeenCalledOnce();
    await waitFor(() => expect(onNavigate).toHaveBeenCalledOnce());
  });

  it("renders the sign-in link and no nav for non-admins", () => {
    renderWithProviders(<Sidebar {...sidebarProps({ isAdmin: false })} />);
    expect(screen.getByText("Sign in as admin")).toBeInTheDocument();
    expect(screen.queryByTestId("nav-dashboard")).not.toBeInTheDocument();
  });

  it("omits the version caption when the daemon reports none", () => {
    renderWithProviders(
      <Sidebar {...sidebarProps({ version: undefined, connected: false })} />,
    );
    expect(screen.queryByText(/^v/)).not.toBeInTheDocument();
  });
});
