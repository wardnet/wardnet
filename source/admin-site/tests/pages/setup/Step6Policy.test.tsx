import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useAdvanceWizard, useSetDefaultPolicy, useDefaultPolicy, useTunnels } =
  vi.hoisted(() => ({
    useAdvanceWizard: vi.fn(),
    useSetDefaultPolicy: vi.fn(),
    useDefaultPolicy: vi.fn(),
    useTunnels: vi.fn(),
  }));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useAdvanceWizard,
    useSetDefaultPolicy,
    useDefaultPolicy,
    useTunnels,
    RoutingSelector: ({
      value,
      onChange,
      tunnels,
    }: {
      value: unknown;
      onChange: (t: { type: "tunnel"; tunnel_id: string }) => void;
      tunnels: unknown[];
    }) => (
      <div data-testid="routing-selector">
        <span data-testid="rs-value">{JSON.stringify(value)}</span>
        <span data-testid="rs-tunnel-count">{tunnels.length}</span>
        <button
          type="button"
          onClick={() => onChange({ type: "tunnel", tunnel_id: "t-1" })}
        >
          pick-tunnel
        </button>
      </div>
    ),
  };
});

import Step6Policy from "@/pages/setup/Step6Policy";
import { renderWithProviders } from "../../test-utils";

const advanceMutate = vi.fn();
const setDefaultMutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  advanceMutate.mockResolvedValue(undefined);
  setDefaultMutate.mockResolvedValue(undefined);
  useAdvanceWizard.mockReturnValue({
    mutateAsync: advanceMutate,
    isPending: false,
  });
  useSetDefaultPolicy.mockReturnValue({
    mutateAsync: setDefaultMutate,
    isPending: false,
  });
  useDefaultPolicy.mockReturnValue({ data: { policy: "direct" } });
  useTunnels.mockReturnValue({ data: { tunnels: [] } });
});

describe("Step6Policy", () => {
  it("shows the no-tunnels hint and defaults to direct", () => {
    renderWithProviders(<Step6Policy />);
    expect(
      screen.getByText(/No tunnels configured — defaulting to direct/),
    ).toBeInTheDocument();
    expect(screen.getByTestId("rs-value")).toHaveTextContent(
      '{"type":"direct"}',
    );
  });

  it("hides the hint when tunnels exist and derives from persisted policy", () => {
    useTunnels.mockReturnValue({ data: { tunnels: [{ id: "t-1" }] } });
    useDefaultPolicy.mockReturnValue({ data: { policy: "t-1" } });
    renderWithProviders(<Step6Policy />);
    expect(screen.queryByText(/No tunnels configured/)).not.toBeInTheDocument();
    expect(screen.getByTestId("rs-value")).toHaveTextContent(
      '{"type":"tunnel","tunnel_id":"t-1"}',
    );
    expect(screen.getByTestId("rs-tunnel-count")).toHaveTextContent("1");
  });

  it("saves direct policy then advances on continue", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Step6Policy />);
    await user.click(screen.getByTestId("setup-policy-continue"));
    await waitFor(() =>
      expect(setDefaultMutate).toHaveBeenCalledWith("direct"),
    );
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "remote_access" });
  });

  it("uses the operator override tunnel id when they pick one", async () => {
    useTunnels.mockReturnValue({ data: { tunnels: [{ id: "t-1" }] } });
    const user = userEvent.setup();
    renderWithProviders(<Step6Policy />);
    await user.click(screen.getByRole("button", { name: "pick-tunnel" }));
    await user.click(screen.getByTestId("setup-policy-continue"));
    await waitFor(() => expect(setDefaultMutate).toHaveBeenCalledWith("t-1"));
  });

  it("shows 'Saving…' while pending", () => {
    useSetDefaultPolicy.mockReturnValue({
      mutateAsync: setDefaultMutate,
      isPending: true,
    });
    renderWithProviders(<Step6Policy />);
    expect(screen.getByTestId("setup-policy-continue")).toHaveTextContent(
      "Saving…",
    );
  });
});
