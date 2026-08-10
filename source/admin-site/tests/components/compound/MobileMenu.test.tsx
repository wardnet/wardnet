import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MobileMenu } from "@/components/compound/MobileMenu";
import { renderWithProviders } from "../../test-utils";

const onLogout = vi.fn();

function menuProps() {
  return {
    isAdmin: true,
    onLogout,
    version: undefined,
    connectionLoading: false,
    connected: false,
    updateAvailable: false,
    latestVersion: null,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  onLogout.mockResolvedValue(true);
});

describe("MobileMenu", () => {
  it("renders the trigger button", () => {
    renderWithProviders(<MobileMenu {...menuProps()} />);
    expect(screen.getByTestId("mobile-menu-trigger")).toBeInTheDocument();
    expect(screen.getByText("Toggle menu")).toBeInTheDocument();
  });

  it("opens the drawer with the sidebar when the trigger is clicked", async () => {
    const user = userEvent.setup();
    renderWithProviders(<MobileMenu {...menuProps()} />);
    await user.click(screen.getByTestId("mobile-menu-trigger"));
    await waitFor(() => {
      expect(screen.getByText("Navigation")).toBeInTheDocument();
    });
    // Sidebar's admin nav is now visible.
    expect(screen.getByTestId("nav-dashboard")).toBeInTheDocument();
  });

  it("closes the drawer when a sidebar nav item is clicked", async () => {
    const user = userEvent.setup();
    renderWithProviders(<MobileMenu {...menuProps()} />);
    await user.click(screen.getByTestId("mobile-menu-trigger"));
    const navItem = await screen.findByTestId("nav-devices");
    // Clicking a nav link fires Sidebar's onNavigate → setOpen(false).
    await user.click(navItem);
    await waitFor(() => {
      expect(screen.queryByText("Navigation")).not.toBeInTheDocument();
    });
  });
});
