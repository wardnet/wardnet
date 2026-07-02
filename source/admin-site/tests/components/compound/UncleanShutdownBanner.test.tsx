import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useSystemStatus, useAcknowledgeShutdown } from "@wardnet/web";
import { UncleanShutdownBanner } from "@/components/compound/UncleanShutdownBanner";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useSystemStatus: vi.fn(),
    useAcknowledgeShutdown: vi.fn(),
  };
});

const mutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(useAcknowledgeShutdown).mockReturnValue({
    mutate,
    isPending: false,
  } as never);
});

function setStatus(last_shutdown: unknown) {
  vi.mocked(useSystemStatus).mockReturnValue({
    data: { last_shutdown },
  } as never);
}

describe("UncleanShutdownBanner", () => {
  it("renders nothing when status is missing", () => {
    vi.mocked(useSystemStatus).mockReturnValue({ data: undefined } as never);
    const { container } = renderWithProviders(<UncleanShutdownBanner />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when the last shutdown was clean", () => {
    setStatus({ state: "clean", at: "2026-01-01T00:00:00Z" });
    const { container } = renderWithProviders(<UncleanShutdownBanner />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when unclean but no timestamp", () => {
    setStatus({ state: "unclean", at: null });
    const { container } = renderWithProviders(<UncleanShutdownBanner />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when acknowledged at-or-after the event", () => {
    setStatus({
      state: "unclean",
      at: "2026-01-01T00:00:00Z",
      acknowledged_at: "2026-01-02T00:00:00Z",
    });
    const { container } = renderWithProviders(<UncleanShutdownBanner />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the banner for an unacknowledged unclean shutdown", () => {
    setStatus({
      state: "unclean",
      at: "2026-01-01T00:00:00Z",
      acknowledged_at: null,
    });
    renderWithProviders(<UncleanShutdownBanner />);
    expect(
      screen.getByText("Wardnet did not shut down cleanly"),
    ).toBeInTheDocument();
  });

  it("shows the banner when the ack predates the event", () => {
    setStatus({
      state: "unclean",
      at: "2026-01-02T00:00:00Z",
      acknowledged_at: "2026-01-01T00:00:00Z",
    });
    renderWithProviders(<UncleanShutdownBanner />);
    expect(
      screen.getByText("Wardnet did not shut down cleanly"),
    ).toBeInTheDocument();
  });

  it("acknowledges when Dismiss is clicked", async () => {
    const user = userEvent.setup();
    setStatus({
      state: "unclean",
      at: "2026-01-01T00:00:00Z",
      acknowledged_at: null,
    });
    renderWithProviders(<UncleanShutdownBanner />);
    await user.click(
      screen.getByRole("button", { name: "Dismiss unclean shutdown banner" }),
    );
    expect(mutate).toHaveBeenCalledTimes(1);
  });
});
