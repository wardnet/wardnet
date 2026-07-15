import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { InstallPrompt } from "../../src/components/InstallPrompt";
import { renderWithProviders } from "../test-utils";

const { useInstallPrompt, toast } = vi.hoisted(() => ({
  useInstallPrompt: vi.fn(),
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock("../../src/hooks/useInstallPrompt", () => ({ useInstallPrompt }));

vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

const promptInstall = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useInstallPrompt.mockReturnValue({ isInstallable: true, promptInstall });
});

describe("InstallPrompt", () => {
  it("renders nothing when not installable", () => {
    useInstallPrompt.mockReturnValue({ isInstallable: false, promptInstall });
    const { container } = renderWithProviders(<InstallPrompt icon="i.svg" />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing away from the home route", () => {
    const { container } = renderWithProviders(<InstallPrompt icon="i.svg" />, {
      route: "/settings",
    });
    expect(container).toBeEmptyDOMElement();
  });

  it("shows a success toast and dismisses when install is accepted", async () => {
    promptInstall.mockResolvedValue({ outcome: "accepted" });
    renderWithProviders(<InstallPrompt icon="i.svg" />);

    await userEvent.click(screen.getByRole("button", { name: /Install/ }));

    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Added to home screen", {
        position: "top-center",
      }),
    );
    // Dismissed → component removed.
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: /Install/ }),
      ).not.toBeInTheDocument(),
    );
  });

  it("dismisses silently when install throws", async () => {
    promptInstall.mockRejectedValue(new Error("cancelled"));
    renderWithProviders(<InstallPrompt icon="i.svg" />);

    await userEvent.click(screen.getByRole("button", { name: /Install/ }));

    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: /Install/ }),
      ).not.toBeInTheDocument(),
    );
    expect(toast.success).not.toHaveBeenCalled();
  });

  it("dismisses when the user taps Later", async () => {
    renderWithProviders(<InstallPrompt icon="i.svg" />);
    await userEvent.click(screen.getByRole("button", { name: "Later" }));
    expect(
      screen.queryByRole("button", { name: "Later" }),
    ).not.toBeInTheDocument();
  });
});
