import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { DashboardLogWidget } from "@/components/features/DashboardLogWidget";
import { renderWithProviders } from "../../test-utils";

vi.mock("@/stores/logStore", () => ({ useLogStore: vi.fn() }));

import { useLogStore } from "@/stores/logStore";

const setFilter = vi.fn();
const clear = vi.fn();
const setPaused = vi.fn();

function setStore(over: Record<string, unknown> = {}) {
  vi.mocked(useLogStore).mockReturnValue({
    entries: [],
    connected: true,
    paused: false,
    skipped: 0,
    filter: { level: "info" },
    setFilter,
    clear,
    setPaused,
    ...over,
  } as unknown as ReturnType<typeof useLogStore>);
}

// Radix Select / DropdownMenu rely on pointer-capture + scroll APIs that
// jsdom does not implement.
beforeAll(() => {
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.hasPointerCapture = vi.fn(() => false);
  Element.prototype.releasePointerCapture = vi.fn();
});

beforeEach(() => {
  vi.clearAllMocks();
  setStore();
});

describe("DashboardLogWidget", () => {
  it("shows the streaming pill when connected", () => {
    renderWithProviders(<DashboardLogWidget />);
    expect(screen.getByText("Streaming")).toBeInTheDocument();
    expect(screen.getByText("Live logs")).toBeInTheDocument();
  });

  it("shows disconnected when not connected", () => {
    setStore({ connected: false });
    renderWithProviders(<DashboardLogWidget />);
    expect(screen.getByText("Disconnected")).toBeInTheDocument();
  });

  it("toggles pause via the desktop button", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DashboardLogWidget />);
    await user.click(screen.getAllByRole("button", { name: /Pause/ })[0]);
    expect(setPaused).toHaveBeenCalledWith(true);
  });

  it("shows Resume when paused", () => {
    setStore({ paused: true });
    renderWithProviders(<DashboardLogWidget />);
    expect(screen.getAllByText("Resume").length).toBeGreaterThan(0);
  });

  it("clears the log via the desktop button", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DashboardLogWidget />);
    await user.click(screen.getAllByRole("button", { name: /Clear/ })[0]);
    expect(clear).toHaveBeenCalled();
  });

  it("changes the level filter via the select", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DashboardLogWidget />);
    await user.click(screen.getByRole("combobox"));
    await user.click(await screen.findByRole("option", { name: "ERROR" }));
    expect(setFilter).toHaveBeenCalledWith({ level: "error" });
  });

  it("toggles pause from the mobile overflow menu", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DashboardLogWidget />);
    await user.click(screen.getByRole("button", { name: "Log actions" }));
    await user.click(await screen.findByRole("menuitem", { name: "Pause" }));
    expect(setPaused).toHaveBeenCalledWith(true);
  });

  it("exposes a download link", () => {
    renderWithProviders(<DashboardLogWidget />);
    const links = screen
      .getAllByRole("link", { name: /Download/ })
      .map((l) => l.getAttribute("href"));
    expect(links).toContain("/api/system/logs/download");
  });
});
