import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useTunnels, useProviders, useDeleteTunnel } = vi.hoisted(() => ({
  useTunnels: vi.fn(),
  useProviders: vi.fn(),
  useDeleteTunnel: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useTunnels, useProviders, useDeleteTunnel };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({
    title,
    actions,
  }: {
    title: ReactNode;
    actions?: ReactNode;
  }) => (
    <div>
      <h1>{title}</h1>
      {actions}
    </div>
  ),
}));
vi.mock("@/components/compound/TunnelGrid", () => ({
  TunnelGrid: ({
    tunnels,
    isLoading,
    isError,
    onDelete,
    onAdd,
  }: {
    tunnels: Array<{ id: string }>;
    isLoading?: boolean;
    isError?: boolean;
    onDelete: (id: string) => void;
    onAdd?: () => void;
  }) => (
    <div data-testid="grid">
      <span data-testid="grid-state">
        {isLoading ? "loading" : isError ? "error" : `count:${tunnels.length}`}
      </span>
      {onAdd && (
        <button onClick={() => onAdd()} data-testid="grid-add">
          grid-add
        </button>
      )}
      {tunnels.map((t) => (
        <button key={t.id} onClick={() => onDelete(t.id)}>
          {`delete-${t.id}`}
        </button>
      ))}
    </div>
  ),
}));
vi.mock("@/components/compound/ConfirmDialog", () => ({
  ConfirmDialog: ({
    open,
    onOpenChange,
    onConfirm,
    description,
  }: {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    onConfirm: () => void;
    description?: ReactNode;
  }) =>
    open ? (
      <div data-testid="confirm">
        <span data-testid="confirm-desc">{description}</span>
        <button onClick={() => onConfirm()}>confirm</button>
        <button onClick={() => onOpenChange(false)}>cancel</button>
      </div>
    ) : null,
}));
vi.mock("@/components/features/CreateTunnelInline", () => ({
  CreateTunnelInline: ({ onClose }: { onClose: () => void }) => (
    <div data-testid="create-inline">
      <button onClick={() => onClose()}>close-inline</button>
    </div>
  ),
}));

import Tunnels from "@/pages/Tunnels";
import { renderWithProviders } from "../test-utils";

const mutate = vi.fn();
beforeEach(() => {
  vi.clearAllMocks();
  useProviders.mockReturnValue({ data: { providers: [] } });
  useDeleteTunnel.mockReturnValue({ mutate });
  useTunnels.mockReturnValue({
    data: { tunnels: [] },
    isLoading: false,
    isError: false,
  });
});

describe("Tunnels", () => {
  it("renders empty grid with no Add button in header", () => {
    renderWithProviders(<Tunnels />);
    expect(screen.getByTestId("grid-state")).toHaveTextContent("count:0");
    expect(
      screen.queryByRole("button", { name: "Add tunnel" }),
    ).not.toBeInTheDocument();
  });

  it("shows loading and error states", () => {
    useTunnels.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    });
    const { unmount } = renderWithProviders(<Tunnels />);
    expect(screen.getByTestId("grid-state")).toHaveTextContent("loading");
    unmount();

    useTunnels.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
    });
    renderWithProviders(<Tunnels />);
    expect(screen.getByTestId("grid-state")).toHaveTextContent("error");
  });

  it("opens the inline create form via the header Add button", async () => {
    useTunnels.mockReturnValue({
      data: { tunnels: [{ id: "t1", label: "VPN", status: "up" }] },
      isLoading: false,
      isError: false,
    });
    const user = userEvent.setup();
    renderWithProviders(<Tunnels />);
    await user.click(screen.getByRole("button", { name: "Add tunnel" }));
    expect(screen.getByTestId("create-inline")).toBeInTheDocument();
    // Add button hidden while creating.
    expect(
      screen.queryByRole("button", { name: "Add tunnel" }),
    ).not.toBeInTheDocument();
    // Closing the inline form restores the Add button.
    await user.click(screen.getByText("close-inline"));
    expect(
      screen.getByRole("button", { name: "Add tunnel" }),
    ).toBeInTheDocument();
  });

  it("opens the inline create form via the grid Add (empty state)", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Tunnels />);
    await user.click(screen.getByTestId("grid-add"));
    expect(screen.getByTestId("create-inline")).toBeInTheDocument();
  });

  it("confirms and deletes a tunnel", async () => {
    useTunnels.mockReturnValue({
      data: { tunnels: [{ id: "t1", label: "VPN", status: "up" }] },
      isLoading: false,
      isError: false,
    });
    const user = userEvent.setup();
    renderWithProviders(<Tunnels />);
    await user.click(screen.getByText("delete-t1"));
    expect(screen.getByTestId("confirm-desc")).toHaveTextContent("VPN");
    await user.click(screen.getByText("confirm"));
    expect(mutate).toHaveBeenCalledWith("t1");
    expect(screen.queryByTestId("confirm")).not.toBeInTheDocument();
  });

  it("cancels the delete dialog without mutating", async () => {
    useTunnels.mockReturnValue({
      data: { tunnels: [{ id: "t1", label: "VPN", status: "up" }] },
      isLoading: false,
      isError: false,
    });
    const user = userEvent.setup();
    renderWithProviders(<Tunnels />);
    await user.click(screen.getByText("delete-t1"));
    await user.click(screen.getByText("cancel"));
    expect(mutate).not.toHaveBeenCalled();
    expect(screen.queryByTestId("confirm")).not.toBeInTheDocument();
  });
});
