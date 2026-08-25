import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useDaemonStatus } = vi.hoisted(() => ({
  useDaemonStatus: vi.fn(),
}));
vi.mock("../../src/hooks/useDaemonStatus", () => ({ useDaemonStatus }));

import { ConnectionGate } from "../../src/components/ConnectionGate";

function child() {
  return <div data-testid="app">app</div>;
}

describe("ConnectionGate", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows a quiet splash while the first probe is in flight", () => {
    useDaemonStatus.mockReturnValue({
      data: undefined,
      isLoading: true,
      isFetching: true,
      refetch: vi.fn(),
    });
    render(<ConnectionGate>{child()}</ConnectionGate>);
    expect(screen.getByLabelText("Connecting to Wardnet")).toBeInTheDocument();
    expect(screen.queryByTestId("app")).not.toBeInTheDocument();
  });

  it("renders children once the daemon is reachable", () => {
    useDaemonStatus.mockReturnValue({
      data: { reachable: true },
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    });
    render(<ConnectionGate>{child()}</ConnectionGate>);
    expect(screen.getByTestId("app")).toBeInTheDocument();
  });

  it("shows the error interstitial and retries on click when unreachable", async () => {
    const user = userEvent.setup();
    const refetch = vi.fn();
    useDaemonStatus.mockReturnValue({
      data: { reachable: false },
      isLoading: false,
      isFetching: false,
      refetch,
    });
    render(<ConnectionGate>{child()}</ConnectionGate>);
    expect(screen.getByRole("alert")).toHaveTextContent("Can’t reach Wardnet");
    await user.click(screen.getByRole("button", { name: /Retry/ }));
    expect(refetch).toHaveBeenCalledOnce();
  });

  it("shows the retrying spinner and label while a retry is in flight", () => {
    useDaemonStatus.mockReturnValue({
      data: { reachable: false },
      isLoading: false,
      isFetching: true,
      refetch: vi.fn(),
    });
    render(<ConnectionGate>{child()}</ConnectionGate>);
    const button = screen.getByRole("button", { name: /Retrying…/ });
    expect(button).toBeDisabled();
  });

  it("stays unblocked after a single successful connection", () => {
    useDaemonStatus.mockReturnValue({
      data: { reachable: true },
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    });
    const { rerender } = render(<ConnectionGate>{child()}</ConnectionGate>);
    expect(screen.getByTestId("app")).toBeInTheDocument();
    // Daemon drops after we've connected once — the latch keeps children mounted.
    useDaemonStatus.mockReturnValue({
      data: { reachable: false },
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    });
    rerender(<ConnectionGate>{child()}</ConnectionGate>);
    expect(screen.getByTestId("app")).toBeInTheDocument();
  });

  it("shows the premium-required interstitial when reachable but not entitled", () => {
    useDaemonStatus.mockReturnValue({
      data: { reachable: true, entitled: false },
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    });
    render(<ConnectionGate>{child()}</ConnectionGate>);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "This app requires wardnet Premium",
    );
    expect(screen.queryByTestId("app")).not.toBeInTheDocument();
  });

  it("re-blocks on losing entitlement mid-session, unlike the reachability latch", () => {
    useDaemonStatus.mockReturnValue({
      data: { reachable: true, entitled: true },
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    });
    const { rerender } = render(<ConnectionGate>{child()}</ConnectionGate>);
    expect(screen.getByTestId("app")).toBeInTheDocument();

    useDaemonStatus.mockReturnValue({
      data: { reachable: true, entitled: false },
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    });
    rerender(<ConnectionGate>{child()}</ConnectionGate>);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "This app requires wardnet Premium",
    );
    expect(screen.queryByTestId("app")).not.toBeInTheDocument();
  });

  it("does not block on entitled: null (unreachable, unknown state)", () => {
    useDaemonStatus.mockReturnValue({
      data: { reachable: true, entitled: null },
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    });
    render(<ConnectionGate>{child()}</ConnectionGate>);
    expect(screen.getByTestId("app")).toBeInTheDocument();
  });

  // biome-ignore lint/security/noSecrets: identifier-shaped string, not a credential — the entropy heuristic misfires on long CamelCase names
  it("premiumGated={false} stays unblocked even when not entitled (admin-site)", () => {
    useDaemonStatus.mockReturnValue({
      data: { reachable: true, entitled: false },
      isLoading: false,
      isFetching: false,
      refetch: vi.fn(),
    });
    render(<ConnectionGate premiumGated={false}>{child()}</ConnectionGate>);
    expect(screen.getByTestId("app")).toBeInTheDocument();
  });
});
