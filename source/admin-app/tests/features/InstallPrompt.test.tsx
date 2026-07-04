import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useInstallPrompt, toastSuccess } = vi.hoisted(() => ({
  useInstallPrompt: vi.fn(),
  toastSuccess: vi.fn(),
}));
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useInstallPrompt };
});
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast: { success: toastSuccess, error: vi.fn() } };
});

import { InstallPrompt } from "@/features/InstallPrompt";
import { renderWithProviders } from "../test-utils";

describe("InstallPrompt", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders nothing when not installable", () => {
    useInstallPrompt.mockReturnValue({
      isInstallable: false,
      promptInstall: vi.fn(),
    });
    const { container } = renderWithProviders(<InstallPrompt />, {
      route: "/",
    });
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when not on the home route", () => {
    useInstallPrompt.mockReturnValue({
      isInstallable: true,
      promptInstall: vi.fn(),
    });
    const { container } = renderWithProviders(<InstallPrompt />, {
      route: "/devices",
    });
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the prompt and can be dismissed with Later", async () => {
    useInstallPrompt.mockReturnValue({
      isInstallable: true,
      promptInstall: vi.fn(),
    });
    renderWithProviders(<InstallPrompt />, { route: "/" });
    expect(screen.getByText("Install Wardnet")).toBeInTheDocument();
    await userEvent.click(screen.getByText("Later"));
    expect(screen.queryByText("Install Wardnet")).not.toBeInTheDocument();
  });

  it("toasts on accepted install and dismisses", async () => {
    const promptInstall = vi.fn().mockResolvedValue({ outcome: "accepted" });
    useInstallPrompt.mockReturnValue({ isInstallable: true, promptInstall });
    renderWithProviders(<InstallPrompt />, { route: "/" });
    await userEvent.click(screen.getByText("Install"));
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith("Added to home screen"),
    );
    expect(screen.queryByText("Install Wardnet")).not.toBeInTheDocument();
  });

  it("dismisses silently when the browser cancels the prompt", async () => {
    const promptInstall = vi.fn().mockRejectedValue(new Error("cancelled"));
    useInstallPrompt.mockReturnValue({ isInstallable: true, promptInstall });
    renderWithProviders(<InstallPrompt />, { route: "/" });
    await userEvent.click(screen.getByText("Install"));
    await waitFor(() =>
      expect(screen.queryByText("Install Wardnet")).not.toBeInTheDocument(),
    );
    expect(toastSuccess).not.toHaveBeenCalled();
  });
});
