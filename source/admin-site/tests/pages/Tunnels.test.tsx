import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useTunnels,
  useProviders,
  useDeleteTunnel,
  useTestTunnel,
  useRebuildTunnel,
  useStartSpeedTest,
  useSpeedTestResultsList,
} = vi.hoisted(() => ({
  useTunnels: vi.fn(),
  useProviders: vi.fn(),
  useDeleteTunnel: vi.fn(),
  useTestTunnel: vi.fn(),
  useRebuildTunnel: vi.fn(),
  useStartSpeedTest: vi.fn(),
  useSpeedTestResultsList: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useTunnels,
    useProviders,
    useDeleteTunnel,
    useTestTunnel,
    useRebuildTunnel,
    useStartSpeedTest,
    useSpeedTestResultsList,
  };
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

// The grid is mocked to a thin harness that surfaces the page's wiring: it
// exposes the per-tunnel callbacks and the derived in-flight ids so the tests
// assert what the page hoisted, not the grid's own rendering.
vi.mock("@/components/compound/TunnelGrid", () => ({
  TunnelGrid: ({
    tunnels,
    isLoading,
    isError,
    onDelete,
    onAdd,
    onTest,
    onRebuild,
    onSpeedTest,
    testingId,
    rebuildingId,
    speedTestRunningId,
    testOutcomes,
  }: {
    tunnels: Array<{ id: string }>;
    isLoading?: boolean;
    isError?: boolean;
    onDelete: (id: string) => void;
    onAdd?: () => void;
    onTest: (id: string) => void;
    onRebuild: (id: string) => void;
    onSpeedTest: (id: string) => void;
    testingId: string | null;
    rebuildingId: string | null;
    speedTestRunningId: string | null;
    testOutcomes: Record<string, { result: unknown; error: string | null }>;
  }) => (
    <div data-testid="grid">
      <span data-testid="grid-state">
        {isLoading ? "loading" : isError ? "error" : `count:${tunnels.length}`}
      </span>
      <span data-testid="testing-id">{testingId ?? "-"}</span>
      <span data-testid="rebuilding-id">{rebuildingId ?? "-"}</span>
      <span data-testid="speedtest-id">{speedTestRunningId ?? "-"}</span>
      <span data-testid="outcome-keys">
        {Object.keys(testOutcomes).join(",")}
      </span>
      {onAdd && (
        <button onClick={() => onAdd()} data-testid="grid-add">
          grid-add
        </button>
      )}
      {tunnels.map((t) => (
        <div key={t.id}>
          <button onClick={() => onDelete(t.id)}>{`delete-${t.id}`}</button>
          <button onClick={() => onTest(t.id)}>{`test-${t.id}`}</button>
          <button onClick={() => onRebuild(t.id)}>{`rebuild-${t.id}`}</button>
          <button onClick={() => onSpeedTest(t.id)}>{`speed-${t.id}`}</button>
        </div>
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

const deleteMutate = vi.fn();
const testMutate = vi.fn();
const rebuildMutate = vi.fn();
const startSpeed = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useProviders.mockReturnValue({ data: { providers: [] } });
  useDeleteTunnel.mockReturnValue({ mutate: deleteMutate });
  useTestTunnel.mockReturnValue({
    mutate: testMutate,
    isPending: false,
    variables: undefined,
  });
  useRebuildTunnel.mockReturnValue({
    mutate: rebuildMutate,
    isPending: false,
    variables: undefined,
  });
  useStartSpeedTest.mockReturnValue({
    start: startSpeed,
    activeTunnelId: null,
    percentage: 0,
    isRunning: false,
  });
  useSpeedTestResultsList.mockReturnValue({});
  useTunnels.mockReturnValue({
    data: { tunnels: [] },
    isLoading: false,
    isError: false,
  });
});

const oneTunnel = {
  data: { tunnels: [{ id: "t1", label: "VPN", status: "up" }] },
  isLoading: false,
  isError: false,
};

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
    useTunnels.mockReturnValue(oneTunnel);
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
    useTunnels.mockReturnValue(oneTunnel);
    const user = userEvent.setup();
    renderWithProviders(<Tunnels />);
    await user.click(screen.getByText("delete-t1"));
    expect(screen.getByTestId("confirm-desc")).toHaveTextContent("VPN");
    await user.click(screen.getByText("confirm"));
    expect(deleteMutate).toHaveBeenCalledWith("t1");
    expect(screen.queryByTestId("confirm")).not.toBeInTheDocument();
  });

  it("cancels the delete dialog without mutating", async () => {
    useTunnels.mockReturnValue(oneTunnel);
    const user = userEvent.setup();
    renderWithProviders(<Tunnels />);
    await user.click(screen.getByText("delete-t1"));
    await user.click(screen.getByText("cancel"));
    expect(deleteMutate).not.toHaveBeenCalled();
    expect(screen.queryByTestId("confirm")).not.toBeInTheDocument();
  });

  it("hoists the connectivity test and records its outcome", async () => {
    useTunnels.mockReturnValue(oneTunnel);
    testMutate.mockImplementation((_id, opts) =>
      opts.onSuccess({
        result: {
          tunnel_id: "t1",
          exit_ip: "9.9.9.9",
          country_code: "us",
          latency_ms: 42,
        },
      }),
    );
    const user = userEvent.setup();
    renderWithProviders(<Tunnels />);
    await user.click(screen.getByText("test-t1"));
    expect(testMutate).toHaveBeenCalledWith("t1", expect.any(Object));
    // The page stored the outcome and passed it back down keyed by tunnel id.
    expect(screen.getByTestId("outcome-keys")).toHaveTextContent("t1");
  });

  it("records a failed connectivity test as an error outcome", async () => {
    useTunnels.mockReturnValue(oneTunnel);
    testMutate.mockImplementation((_id, opts) =>
      opts.onError(new Error("unreachable")),
    );
    const user = userEvent.setup();
    renderWithProviders(<Tunnels />);
    await user.click(screen.getByText("test-t1"));
    expect(screen.getByTestId("outcome-keys")).toHaveTextContent("t1");
  });

  it("derives the in-flight testing id from the shared mutation", () => {
    useTunnels.mockReturnValue(oneTunnel);
    useTestTunnel.mockReturnValue({
      mutate: testMutate,
      isPending: true,
      variables: "t1",
    });
    renderWithProviders(<Tunnels />);
    expect(screen.getByTestId("testing-id")).toHaveTextContent("t1");
  });

  it("hoists the rebuild mutation to the grid callback", async () => {
    useTunnels.mockReturnValue(oneTunnel);
    const user = userEvent.setup();
    renderWithProviders(<Tunnels />);
    await user.click(screen.getByText("rebuild-t1"));
    expect(rebuildMutate).toHaveBeenCalledWith("t1");
  });

  it("derives the rebuilding id from the shared mutation", () => {
    useTunnels.mockReturnValue(oneTunnel);
    useRebuildTunnel.mockReturnValue({
      mutate: rebuildMutate,
      isPending: true,
      variables: "t1",
    });
    renderWithProviders(<Tunnels />);
    expect(screen.getByTestId("rebuilding-id")).toHaveTextContent("t1");
  });

  it("starts a speed test and fetches results for the tested tunnel", async () => {
    useTunnels.mockReturnValue(oneTunnel);
    const user = userEvent.setup();
    renderWithProviders(<Tunnels />);
    await user.click(screen.getByText("speed-t1"));
    expect(startSpeed).toHaveBeenCalledWith("t1");
    // The tunnel joins the set the page fetches speed-test results for.
    expect(useSpeedTestResultsList).toHaveBeenLastCalledWith(["t1"]);
  });

  it("stops fetching speed-test results for a tunnel that no longer exists", async () => {
    useTunnels.mockReturnValue(oneTunnel);
    const user = userEvent.setup();
    const { rerender } = renderWithProviders(<Tunnels />);
    await user.click(screen.getByText("speed-t1"));
    expect(useSpeedTestResultsList).toHaveBeenLastCalledWith(["t1"]);
    // The tunnel is deleted; its speed-test query must drop out rather than
    // keep refetching against a tunnel that is gone.
    useTunnels.mockReturnValue({
      data: { tunnels: [] },
      isLoading: false,
      isError: false,
    });
    rerender(<Tunnels />);
    expect(useSpeedTestResultsList).toHaveBeenLastCalledWith([]);
  });

  it("derives the running speed-test id from the shared hook", () => {
    useTunnels.mockReturnValue(oneTunnel);
    useStartSpeedTest.mockReturnValue({
      start: startSpeed,
      activeTunnelId: "t1",
      percentage: 30,
      isRunning: true,
    });
    renderWithProviders(<Tunnels />);
    expect(screen.getByTestId("speedtest-id")).toHaveTextContent("t1");
  });
});
