/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useSystemStatus,
  useUpdateStatus,
  useCheckForUpdates,
  useInstallUpdate,
  useRollbackUpdate,
  useUpdateConfig,
} = vi.hoisted(() => ({
  useSystemStatus: vi.fn(),
  useUpdateStatus: vi.fn(),
  useCheckForUpdates: vi.fn(),
  useInstallUpdate: vi.fn(),
  useRollbackUpdate: vi.fn(),
  useUpdateConfig: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useSystemStatus,
    useUpdateStatus,
    useCheckForUpdates,
    useInstallUpdate,
    useRollbackUpdate,
    useUpdateConfig,
    formatUptime: (s: number) => `up:${s}`,
    formatBytes: (b: number) => `bytes:${b}`,
  };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: any) => <h1>{title}</h1>,
}));

vi.mock("@/components/features/UpdateCard", () => ({
  UpdateCard: (props: any) => (
    <div data-testid="update-card" data-loading={String(props.isLoading)}>
      <button onClick={() => props.onCheck()}>check</button>
      <button onClick={() => props.onInstall()}>install</button>
      <button onClick={() => props.onRollback()}>rollback</button>
      <button onClick={() => props.onToggleAutoUpdate(true)}>toggle</button>
      <button onClick={() => props.onChangeChannel("beta")}>channel</button>
    </div>
  ),
}));

import Settings from "@/pages/Settings";
import { renderWithProviders } from "../test-utils";

const checkMutate = vi.fn();
const installMutate = vi.fn();
const rollbackMutate = vi.fn();
const saveMutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useSystemStatus.mockReturnValue({ data: undefined, isLoading: false });
  useUpdateStatus.mockReturnValue({ data: undefined, isLoading: false });
  useCheckForUpdates.mockReturnValue({ isPending: false, mutate: checkMutate });
  useInstallUpdate.mockReturnValue({ isPending: false, mutate: installMutate });
  useRollbackUpdate.mockReturnValue({
    isPending: false,
    mutate: rollbackMutate,
  });
  useUpdateConfig.mockReturnValue({ isPending: false, mutate: saveMutate });
});

describe("Settings", () => {
  it("shows a loading state for system information", () => {
    useSystemStatus.mockReturnValue({ data: undefined, isLoading: true });
    renderWithProviders(<Settings />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("shows the unable-to-connect fallback when status is missing", () => {
    useSystemStatus.mockReturnValue({ data: null, isLoading: false });
    renderWithProviders(<Settings />);
    expect(
      screen.getByText("Unable to connect to daemon."),
    ).toBeInTheDocument();
  });

  it("renders system information when status is available", () => {
    useSystemStatus.mockReturnValue({
      data: {
        version: "1.2.3-abc",
        release_version: "1.2.3",
        uptime_seconds: 3600,
        device_count: 5,
        tunnel_count: 2,
        db_size_bytes: 1024,
      },
      isLoading: false,
    });
    renderWithProviders(<Settings />);
    expect(screen.getByTestId("system-info")).toBeInTheDocument();
    const version = screen.getByTestId("system-version");
    expect(version).toHaveTextContent("1.2.3");
    expect(version).toHaveAttribute("title", "build: 1.2.3-abc");
    expect(screen.getByText("up:3600")).toBeInTheDocument();
    expect(screen.getByText("5")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByText("bytes:1024")).toBeInTheDocument();
  });

  it("wires the update card callbacks to the mutations", async () => {
    useUpdateStatus.mockReturnValue({
      data: { status: { current: "1.0.0" } },
      isLoading: true,
    });
    const user = userEvent.setup();
    renderWithProviders(<Settings />);
    expect(screen.getByTestId("update-card")).toHaveAttribute(
      "data-loading",
      "true",
    );
    await user.click(screen.getByText("check"));
    await user.click(screen.getByText("install"));
    await user.click(screen.getByText("rollback"));
    await user.click(screen.getByText("toggle"));
    await user.click(screen.getByText("channel"));
    expect(checkMutate).toHaveBeenCalled();
    expect(installMutate).toHaveBeenCalledWith({});
    expect(rollbackMutate).toHaveBeenCalled();
    expect(saveMutate).toHaveBeenCalledWith({ auto_update_enabled: true });
    expect(saveMutate).toHaveBeenCalledWith({ channel: "beta" });
  });

  it("renders the account placeholder card", () => {
    renderWithProviders(<Settings />);
    expect(
      screen.getByText(/Account management will be available/),
    ).toBeInTheDocument();
  });
});
