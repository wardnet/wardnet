import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { UncleanShutdownBanner } from "@/components/compound/UncleanShutdownBanner";
import { renderWithProviders } from "../../test-utils";
import type { SystemStatusResponse } from "@wardnet/js";

const onDismiss = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
});

function renderBanner(last_shutdown: unknown) {
  const status =
    last_shutdown === null
      ? undefined
      : ({ last_shutdown } as unknown as SystemStatusResponse);
  return renderWithProviders(
    <UncleanShutdownBanner
      status={status}
      onDismiss={onDismiss}
      dismissPending={false}
    />,
  );
}

describe("UncleanShutdownBanner", () => {
  it("renders nothing when status is missing", () => {
    const { container } = renderBanner(null);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when the last shutdown was clean", () => {
    const { container } = renderBanner({
      state: "clean",
      at: "2026-01-01T00:00:00Z",
    });
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when unclean but no timestamp", () => {
    const { container } = renderBanner({ state: "unclean", at: null });
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when acknowledged at-or-after the event", () => {
    const { container } = renderBanner({
      state: "unclean",
      at: "2026-01-01T00:00:00Z",
      acknowledged_at: "2026-01-02T00:00:00Z",
    });
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the banner for an unacknowledged unclean shutdown", () => {
    renderBanner({
      state: "unclean",
      at: "2026-01-01T00:00:00Z",
      acknowledged_at: null,
    });
    expect(
      screen.getByText("Wardnet did not shut down cleanly"),
    ).toBeInTheDocument();
  });

  it("shows the banner when the ack predates the event", () => {
    renderBanner({
      state: "unclean",
      at: "2026-01-02T00:00:00Z",
      acknowledged_at: "2026-01-01T00:00:00Z",
    });
    expect(
      screen.getByText("Wardnet did not shut down cleanly"),
    ).toBeInTheDocument();
  });

  it("acknowledges when Dismiss is clicked", async () => {
    const user = userEvent.setup();
    renderBanner({
      state: "unclean",
      at: "2026-01-01T00:00:00Z",
      acknowledged_at: null,
    });
    await user.click(
      screen.getByRole("button", { name: "Dismiss unclean shutdown banner" }),
    );
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });
});
