import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useAdvanceWizard, useTunnels } = vi.hoisted(() => ({
  useAdvanceWizard: vi.fn(),
  useTunnels: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useAdvanceWizard, useTunnels };
});

vi.mock("@/components/features/CreateTunnelInline", () => ({
  CreateTunnelInline: ({ embedded }: { embedded?: boolean }) => (
    <div data-testid="create-tunnel-inline">inline:{String(embedded)}</div>
  ),
}));

import StepTunnel from "@/pages/setup/StepTunnel";
import { renderWithProviders } from "../../test-utils";

const advanceMutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useAdvanceWizard.mockReturnValue({ mutate: advanceMutate, isPending: false });
  useTunnels.mockReturnValue({ data: { tunnels: [] } });
});

describe("StepTunnel", () => {
  it("renders the empty state with Add/Skip", () => {
    renderWithProviders(<StepTunnel />);
    expect(screen.getByText(/No tunnels yet/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Add tunnel" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("setup-tunnel-skip")).toHaveTextContent(
      "Skip for now",
    );
  });

  it("renders singular tunnel-configured summary + Continue", () => {
    useTunnels.mockReturnValue({ data: { tunnels: [{ id: "t1" }] } });
    renderWithProviders(<StepTunnel />);
    expect(screen.getByText(/1 tunnel configured/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Add another tunnel" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("setup-tunnel-skip")).toHaveTextContent(
      "Continue",
    );
  });

  it("pluralizes the tunnel count", () => {
    useTunnels.mockReturnValue({
      data: { tunnels: [{ id: "t1" }, { id: "t2" }] },
    });
    renderWithProviders(<StepTunnel />);
    expect(screen.getByText(/2 tunnels configured/)).toBeInTheDocument();
  });

  it("shows the inline create form when Add is clicked", async () => {
    const user = userEvent.setup();
    renderWithProviders(<StepTunnel />);
    await user.click(screen.getByRole("button", { name: "Add tunnel" }));
    expect(screen.getByTestId("create-tunnel-inline")).toHaveTextContent(
      "inline:true",
    );
  });

  it("returns to the overview from the inline form's footer Back", async () => {
    const user = userEvent.setup();
    renderWithProviders(<StepTunnel />);
    await user.click(screen.getByRole("button", { name: "Add tunnel" }));
    await user.click(
      screen.getByRole("button", { name: "Back to tunnel overview" }),
    );
    expect(
      screen.queryByTestId("create-tunnel-inline"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Add tunnel" }),
    ).toBeInTheDocument();
  });

  it("advances to policy on skip", async () => {
    const user = userEvent.setup();
    renderWithProviders(<StepTunnel />);
    await user.click(screen.getByTestId("setup-tunnel-skip"));
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "policy" });
  });

  it("shows 'Saving…' while advance is pending", () => {
    useAdvanceWizard.mockReturnValue({
      mutate: advanceMutate,
      isPending: true,
    });
    renderWithProviders(<StepTunnel />);
    expect(screen.getByTestId("setup-tunnel-skip")).toHaveTextContent(
      "Saving…",
    );
  });
});
