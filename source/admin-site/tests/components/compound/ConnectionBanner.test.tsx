import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import { useDaemonStatus } from "@wardnet/web";
import { ConnectionBanner } from "@/components/compound/ConnectionBanner";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useDaemonStatus: vi.fn() };
});

const mockUseDaemonStatus = vi.mocked(useDaemonStatus);

describe("ConnectionBanner", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders nothing when there is no data", () => {
    mockUseDaemonStatus.mockReturnValue({ data: undefined } as never);
    const { container } = renderWithProviders(<ConnectionBanner />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when the daemon is reachable", () => {
    mockUseDaemonStatus.mockReturnValue({
      data: { reachable: true },
    } as never);
    const { container } = renderWithProviders(<ConnectionBanner />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the alert banner when the daemon is unreachable", () => {
    mockUseDaemonStatus.mockReturnValue({
      data: { reachable: false },
    } as never);
    renderWithProviders(<ConnectionBanner />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(
      screen.getByText(/Unable to connect to the Wardnet daemon/),
    ).toBeInTheDocument();
  });
});
