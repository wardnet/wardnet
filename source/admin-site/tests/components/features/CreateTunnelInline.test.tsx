import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { CreateTunnelInline } from "@/components/features/CreateTunnelInline";
import { renderWithProviders } from "../../test-utils";

// The tab bodies pull in their own hook-heavy forms; stub them so this test
// stays focused on the CreateTunnelInline chrome and tab wiring.
vi.mock("@/components/features/ManualTunnelTab", () => ({
  ManualTunnelTab: ({ onSuccess }: { onSuccess: () => void }) => (
    <button type="button" onClick={onSuccess}>
      manual-tab-body
    </button>
  ),
}));
vi.mock("@/components/features/ProviderTunnelTab", () => ({
  ProviderTunnelTab: ({ onSuccess }: { onSuccess: () => void }) => (
    <button type="button" onClick={onSuccess}>
      provider-tab-body
    </button>
  ),
}));

describe("CreateTunnelInline", () => {
  it("renders inside a Card by default and Cancel calls onClose", async () => {
    const onClose = vi.fn();
    renderWithProviders(<CreateTunnelInline onClose={onClose} />);

    expect(screen.getByText("Add WireGuard tunnel")).toBeInTheDocument();
    expect(screen.getByText("manual-tab-body")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("switches to the provider tab", async () => {
    renderWithProviders(<CreateTunnelInline onClose={vi.fn()} />);

    await userEvent.click(screen.getByRole("tab", { name: "Provider" }));
    expect(screen.getByText("provider-tab-body")).toBeInTheDocument();
  });

  it("renders the embedded layout without the outer Card", async () => {
    const onClose = vi.fn();
    renderWithProviders(<CreateTunnelInline onClose={onClose} embedded />);

    expect(
      screen.getByRole("heading", { name: "Add WireGuard tunnel" }),
    ).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
