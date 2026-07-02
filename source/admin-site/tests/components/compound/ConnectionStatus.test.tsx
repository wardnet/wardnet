import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import { useDaemonStatus } from "@wardnet/web";
import { ConnectionStatus } from "@/components/compound/ConnectionStatus";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useDaemonStatus: vi.fn() };
});

const mockUseDaemonStatus = vi.mocked(useDaemonStatus);

function dot(): HTMLElement {
  return document.querySelector("span.inline-block") as HTMLElement;
}

describe("ConnectionStatus", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows Connecting while loading", () => {
    mockUseDaemonStatus.mockReturnValue({
      data: undefined,
      isLoading: true,
    } as never);
    renderWithProviders(<ConnectionStatus />);
    expect(screen.getByText("Connecting…")).toBeInTheDocument();
    expect(dot().className).toContain("bg-warn");
  });

  it("shows Connected when reachable", () => {
    mockUseDaemonStatus.mockReturnValue({
      data: { reachable: true },
      isLoading: false,
    } as never);
    renderWithProviders(<ConnectionStatus />);
    expect(screen.getByText("Connected")).toBeInTheDocument();
    expect(dot().className).toContain("bg-accent");
  });

  it("shows Disconnected when not reachable", () => {
    mockUseDaemonStatus.mockReturnValue({
      data: { reachable: false },
      isLoading: false,
    } as never);
    renderWithProviders(<ConnectionStatus />);
    expect(screen.getByText("Disconnected")).toBeInTheDocument();
    expect(dot().className).toContain("bg-danger");
  });

  it("defaults to Disconnected when data is absent and not loading", () => {
    mockUseDaemonStatus.mockReturnValue({
      data: undefined,
      isLoading: false,
    } as never);
    renderWithProviders(<ConnectionStatus />);
    expect(screen.getByText("Disconnected")).toBeInTheDocument();
  });
});
