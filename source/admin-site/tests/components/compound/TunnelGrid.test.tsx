import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  useTestTunnel,
  useRebuildTunnel,
  useSpeedTestResults,
  useStartSpeedTest,
} from "@wardnet/web";
import type { Tunnel, ProviderInfo } from "@wardnet/js";
import { TunnelGrid } from "@/components/compound/TunnelGrid";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useTestTunnel: vi.fn(),
    useRebuildTunnel: vi.fn(),
    useSpeedTestResults: vi.fn(),
    useStartSpeedTest: vi.fn(),
  };
});

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(useTestTunnel).mockReturnValue({
    mutate: vi.fn(),
    isPending: false,
  } as never);
  vi.mocked(useRebuildTunnel).mockReturnValue({
    mutate: vi.fn(),
    isPending: false,
    variables: undefined,
  } as never);
  vi.mocked(useSpeedTestResults).mockReturnValue({ data: undefined } as never);
  vi.mocked(useStartSpeedTest).mockReturnValue({
    isRunning: false,
    activeTunnelId: null,
    percentage: 0,
    start: vi.fn(),
  } as never);
});

const providers: ProviderInfo[] = [
  { id: "nord", name: "NordVPN" } as ProviderInfo,
];

function makeTunnel(overrides: Partial<Tunnel> = {}): Tunnel {
  return {
    id: "t1",
    label: "My tunnel",
    provider: "nord",
    interface_name: "wg_ward0",
    country_code: "us",
    status: "up",
    endpoint: "1.2.3.4:51820",
    last_handshake: null,
    bytes_tx: 1000,
    bytes_rx: 2000,
    ...overrides,
  } as Tunnel;
}

describe("TunnelGrid", () => {
  it("shows a loading state", () => {
    renderWithProviders(
      <TunnelGrid
        tunnels={[]}
        providers={providers}
        isLoading={true}
        isError={false}
        onDelete={vi.fn()}
      />,
    );
    expect(screen.getByText("Loading tunnels...")).toBeInTheDocument();
  });

  it("shows the empty state with an Add action", async () => {
    const user = userEvent.setup();
    const onAdd = vi.fn();
    renderWithProviders(
      <TunnelGrid
        tunnels={[]}
        providers={providers}
        isLoading={false}
        isError={false}
        onDelete={vi.fn()}
        onAdd={onAdd}
      />,
    );
    expect(screen.getByText("No tunnels configured")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Add tunnel" }));
    expect(onAdd).toHaveBeenCalledTimes(1);
  });

  it("renders null when there are no tunnels and an error occurred", () => {
    const { container } = renderWithProviders(
      <TunnelGrid
        tunnels={[]}
        providers={providers}
        isLoading={false}
        isError={true}
        onDelete={vi.fn()}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("renders a card per tunnel and forwards delete", async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn();
    renderWithProviders(
      <TunnelGrid
        tunnels={[makeTunnel(), makeTunnel({ id: "t2", label: "Second" })]}
        providers={providers}
        isLoading={false}
        isError={false}
        onDelete={onDelete}
      />,
    );
    expect(screen.getByText("My tunnel")).toBeInTheDocument();
    expect(screen.getByText("Second")).toBeInTheDocument();

    const deleteButtons = screen.getAllByRole("button", { name: "Delete" });
    await user.click(deleteButtons[0]);
    expect(onDelete).toHaveBeenCalledWith("t1");
  });

  // Card order is the grid's job. Asserted on the rendered sequence so
  // dropping the sort can't pass silently.
  it("renders tunnel cards alphabetically regardless of the order passed in", () => {
    renderWithProviders(
      <TunnelGrid
        tunnels={[
          makeTunnel({ id: "t1", label: "zeta" }),
          makeTunnel({ id: "t2", label: "Alpha" }),
          makeTunnel({ id: "t3", label: "beta" }),
        ]}
        providers={providers}
        isLoading={false}
        isError={false}
        onDelete={vi.fn()}
      />,
    );
    // Each card is a link wrapping the whole tunnel; assert on href order so
    // the check is about sequence, not card markup.
    expect(
      screen.getAllByRole("link").map((a) => a.getAttribute("href")),
    ).toEqual(
      ["/tunnels/t2", "/tunnels/t3", "/tunnels/t1"], // Alpha, beta, zeta
    );
  });
});
