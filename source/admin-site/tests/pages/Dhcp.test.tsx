import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useDhcpStatus,
  useDhcpConfig,
  useDhcpLeases,
  useDhcpReservations,
  useToggleDhcp,
  useRevokeLease,
  useDeleteReservation,
  useCreateReservation,
  useUpdateDhcpConfig,
  usePreviewDhcpConfig,
  useDnsConfig,
  useDevices,
  navigate,
} = vi.hoisted(() => ({
  useDhcpStatus: vi.fn(),
  useDhcpConfig: vi.fn(),
  useDhcpLeases: vi.fn(),
  useDhcpReservations: vi.fn(),
  useToggleDhcp: vi.fn(),
  useRevokeLease: vi.fn(),
  useDeleteReservation: vi.fn(),
  useCreateReservation: vi.fn(),
  useUpdateDhcpConfig: vi.fn(),
  usePreviewDhcpConfig: vi.fn(),
  useDnsConfig: vi.fn(),
  useDevices: vi.fn(),
  navigate: vi.fn(),
}));

vi.mock("react-router", async (io) => {
  const actual = await io<typeof import("react-router")>();
  return { ...actual, useNavigate: () => navigate };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDhcpStatus,
    useDhcpConfig,
    useDhcpLeases,
    useDhcpReservations,
    useToggleDhcp,
    useRevokeLease,
    useDeleteReservation,
    useCreateReservation,
    useUpdateDhcpConfig,
    usePreviewDhcpConfig,
    useDnsConfig,
    useDevices,
  };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: { title: ReactNode }) => <h1>{title}</h1>,
}));
vi.mock("@/components/compound/DhcpStatusCard", () => ({
  DhcpStatusCard: ({
    onToggle,
    isPending,
  }: {
    onToggle: (enabled: boolean) => void;
    isPending?: boolean;
  }) => (
    <div data-testid="dhcp-status-card">
      <button onClick={() => onToggle(true)} disabled={isPending}>
        toggle-dhcp
      </button>
    </div>
  ),
}));
vi.mock("@/components/compound/DhcpConfigCard", () => ({
  DhcpConfigCard: ({
    dnsEnabled,
    updateConfig,
    previewConfig,
  }: {
    dnsEnabled: boolean | undefined;
    updateConfig: { mutateAsync: unknown };
    previewConfig: { mutateAsync: unknown };
  }) => (
    <div
      data-testid="dhcp-config-card"
      data-dns-enabled={String(dnsEnabled)}
      data-has-update={String(typeof updateConfig.mutateAsync === "function")}
      data-has-preview={String(typeof previewConfig.mutateAsync === "function")}
    />
  ),
}));
vi.mock("@/components/compound/DhcpEntryTable", () => ({
  DhcpEntryTable: ({
    leases,
    reservations,
    activeGroup,
    onGroupChange,
    onSearchChange,
    onMakeStatic,
    onRevokeLease,
    onDeleteReservation,
    onAddReservation,
    onDeviceClick,
  }: {
    leases: Array<{ id: string }>;
    reservations: Array<{ id: string }>;
    activeGroup: string;
    onGroupChange: (group: string) => void;
    onSearchChange: (value: string) => void;
    onMakeStatic: (lease: {
      mac_address: string;
      ip_address: string;
      hostname: string;
    }) => void;
    onRevokeLease: (id: string) => void;
    onDeleteReservation: (id: string) => void;
    onAddReservation: () => void;
    onDeviceClick: (id: string) => void;
  }) => (
    <div data-testid="dhcp-entry-table">
      <span data-testid="lease-count">{leases.length}</span>
      <span data-testid="reservation-count">{reservations.length}</span>
      <span data-testid="active-group">{activeGroup}</span>
      <button onClick={() => onGroupChange("reservations")}>group-res</button>
      <button onClick={() => onSearchChange("q")}>search</button>
      <button onClick={() => onAddReservation()}>add-res</button>
      <button
        onClick={() =>
          onMakeStatic({
            mac_address: "AA:BB:CC:DD:EE:FF",
            ip_address: "10.0.0.5",
            hostname: "printer",
          })
        }
      >
        make-static
      </button>
      {leases.map((l) => (
        <button key={l.id} onClick={() => onRevokeLease(l.id)}>
          {`revoke-${l.id}`}
        </button>
      ))}
      {reservations.map((r) => (
        <button key={r.id} onClick={() => onDeleteReservation(r.id)}>
          {`delete-res-${r.id}`}
        </button>
      ))}
      <button onClick={() => onDeviceClick("dev-9")}>device-click</button>
    </div>
  ),
}));
vi.mock("@/components/compound/ConfirmDialog", () => ({
  ConfirmDialog: ({
    open,
    onOpenChange,
    onConfirm,
    title,
    description,
  }: {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    onConfirm: () => void;
    title: string;
    description?: ReactNode;
  }) =>
    open ? (
      <div data-testid={`confirm-${title}`}>
        <span data-testid="confirm-desc">{description}</span>
        <button onClick={() => onConfirm()}>{`confirm-${title}`}</button>
        <button onClick={() => onOpenChange(false)}>{`cancel-${title}`}</button>
      </div>
    ) : null,
}));
vi.mock("@/components/features/CreateReservationInline", () => ({
  CreateReservationInline: ({
    onClose,
    onSuccess,
    defaults,
    createReservation,
  }: {
    onClose: () => void;
    onSuccess: () => void;
    defaults?: unknown;
    createReservation: { mutateAsync: unknown };
  }) => (
    <div
      data-testid="create-reservation"
      data-has-create={String(
        typeof createReservation.mutateAsync === "function",
      )}
    >
      <span data-testid="defaults">{JSON.stringify(defaults ?? null)}</span>
      <button onClick={() => onClose()}>close-res</button>
      <button onClick={() => onSuccess()}>success-res</button>
    </div>
  ),
}));

import Dhcp from "@/pages/Dhcp";
import { renderWithProviders } from "../test-utils";

const toggleMutate = vi.fn();
const revokeMutate = vi.fn();
const deleteMutate = vi.fn();
const createReservationReset = vi.fn();

function populated() {
  useDhcpStatus.mockReturnValue({
    data: { enabled: true },
    isLoading: false,
  });
  useDhcpConfig.mockReturnValue({
    data: { config: { pool_start: "10.0.0.2" } },
  });
  useDhcpLeases.mockReturnValue({
    data: {
      leases: [
        {
          id: "l1",
          ip_address: "10.0.0.10",
          hostname: "laptop",
          mac_address: "AA",
        },
        {
          id: "l2",
          ip_address: "10.0.0.11",
          hostname: null,
          mac_address: "BB",
        },
      ],
    },
  });
  useDhcpReservations.mockReturnValue({
    data: {
      reservations: [
        { id: "r1", ip_address: "10.0.0.20", description: "printer" },
        { id: "r2", ip_address: "10.0.0.21", description: null },
      ],
    },
  });
  useDevices.mockReturnValue({ data: { devices: [] } });
}

beforeEach(() => {
  vi.clearAllMocks();
  useToggleDhcp.mockReturnValue({ mutate: toggleMutate, isPending: false });
  useRevokeLease.mockReturnValue({ mutate: revokeMutate });
  useDeleteReservation.mockReturnValue({ mutate: deleteMutate });
  useCreateReservation.mockReturnValue({
    mutateAsync: vi.fn(),
    mutate: vi.fn(),
    reset: createReservationReset,
    isPending: false,
    isError: false,
    error: null,
  });
  useUpdateDhcpConfig.mockReturnValue({
    mutateAsync: vi.fn(),
    reset: vi.fn(),
    isPending: false,
    isError: false,
    error: null,
  });
  usePreviewDhcpConfig.mockReturnValue({
    mutateAsync: vi.fn(),
    isPending: false,
  });
  useDnsConfig.mockReturnValue({ data: { config: { enabled: true } } });
  useDhcpStatus.mockReturnValue({ data: undefined, isLoading: false });
  useDhcpConfig.mockReturnValue({ data: undefined });
  useDhcpLeases.mockReturnValue({ data: undefined });
  useDhcpReservations.mockReturnValue({ data: undefined });
  useDevices.mockReturnValue({ data: undefined });
});

describe("Dhcp", () => {
  it("shows a loading card while status loads", () => {
    useDhcpStatus.mockReturnValue({ data: undefined, isLoading: true });
    renderWithProviders(<Dhcp />);
    expect(screen.getByText("Loading DHCP status...")).toBeInTheDocument();
    expect(screen.queryByTestId("dhcp-entry-table")).not.toBeInTheDocument();
  });

  it("renders status/config/table when data present", () => {
    populated();
    renderWithProviders(<Dhcp />);
    expect(screen.getByTestId("dhcp-status-card")).toBeInTheDocument();
    expect(screen.getByTestId("dhcp-config-card")).toBeInTheDocument();
    expect(screen.getByTestId("lease-count")).toHaveTextContent("2");
    expect(screen.getByTestId("reservation-count")).toHaveTextContent("2");
  });

  it("wires the hoisted config mutations and DNS state into the config card", () => {
    populated();
    renderWithProviders(<Dhcp />);
    const card = screen.getByTestId("dhcp-config-card");
    expect(card).toHaveAttribute("data-dns-enabled", "true");
    expect(card).toHaveAttribute("data-has-update", "true");
    expect(card).toHaveAttribute("data-has-preview", "true");
  });

  it("keeps the config card's DNS state tri-state while the DNS query is unresolved", () => {
    populated();
    useDnsConfig.mockReturnValue({ data: undefined });
    renderWithProviders(<Dhcp />);
    expect(screen.getByTestId("dhcp-config-card")).toHaveAttribute(
      "data-dns-enabled",
      "undefined",
    );
  });

  it("hands the hoisted create-reservation mutation to the inline form", async () => {
    populated();
    const user = userEvent.setup();
    renderWithProviders(<Dhcp />);
    await user.click(screen.getByText("add-res"));
    expect(screen.getByTestId("create-reservation")).toHaveAttribute(
      "data-has-create",
      "true",
    );
  });

  it("resets the create mutation whenever the form opens, clearing stale errors", async () => {
    populated();
    const user = userEvent.setup();
    renderWithProviders(<Dhcp />);
    await user.click(screen.getByText("add-res"));
    expect(createReservationReset).toHaveBeenCalledTimes(1);
    await user.click(screen.getByText("close-res"));
    // Reopening from a lease must also start from a clean mutation.
    await user.click(screen.getByText("make-static"));
    expect(createReservationReset).toHaveBeenCalledTimes(2);
  });

  it("toggles DHCP, changes group and search", async () => {
    populated();
    const user = userEvent.setup();
    renderWithProviders(<Dhcp />);
    await user.click(screen.getByText("toggle-dhcp"));
    expect(toggleMutate).toHaveBeenCalledWith(true);
    await user.click(screen.getByText("group-res"));
    expect(screen.getByTestId("active-group")).toHaveTextContent(
      "reservations",
    );
    await user.click(screen.getByText("search"));
  });

  it("opens the create form via Add and closes it", async () => {
    populated();
    const user = userEvent.setup();
    renderWithProviders(<Dhcp />);
    await user.click(screen.getByText("add-res"));
    expect(screen.getByTestId("create-reservation")).toBeInTheDocument();
    expect(screen.getByTestId("defaults")).toHaveTextContent("null");
    await user.click(screen.getByText("close-res"));
    expect(screen.queryByTestId("create-reservation")).not.toBeInTheDocument();
  });

  it("opens create form pre-filled from a lease and switches group on success", async () => {
    populated();
    const user = userEvent.setup();
    renderWithProviders(<Dhcp />);
    await user.click(screen.getByText("make-static"));
    expect(screen.getByTestId("defaults")).toHaveTextContent("10.0.0.5");
    await user.click(screen.getByText("success-res"));
    expect(screen.getByTestId("active-group")).toHaveTextContent(
      "reservations",
    );
  });

  it("revokes a lease through the confirm dialog", async () => {
    populated();
    const user = userEvent.setup();
    renderWithProviders(<Dhcp />);
    await user.click(screen.getByText("revoke-l1"));
    expect(screen.getByTestId("confirm-Revoke lease")).toBeInTheDocument();
    expect(screen.getByTestId("confirm-desc")).toHaveTextContent("laptop");
    await user.click(screen.getByText("confirm-Revoke lease"));
    expect(revokeMutate).toHaveBeenCalledWith("l1");
  });

  it("deletes a reservation through the confirm dialog", async () => {
    populated();
    const user = userEvent.setup();
    renderWithProviders(<Dhcp />);
    await user.click(screen.getByText("delete-res-r1"));
    expect(
      screen.getByTestId("confirm-Delete reservation"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("confirm-desc")).toHaveTextContent("printer");
    await user.click(screen.getByText("confirm-Delete reservation"));
    expect(deleteMutate).toHaveBeenCalledWith("r1");
  });

  it("cancels a confirm dialog without mutating", async () => {
    populated();
    const user = userEvent.setup();
    renderWithProviders(<Dhcp />);
    await user.click(screen.getByText("revoke-l2"));
    await user.click(screen.getByText("cancel-Revoke lease"));
    expect(revokeMutate).not.toHaveBeenCalled();
  });

  it("navigates to a device on device click", async () => {
    populated();
    const user = userEvent.setup();
    renderWithProviders(<Dhcp />);
    await user.click(screen.getByText("device-click"));
    expect(navigate).toHaveBeenCalledWith("/devices/dev-9");
  });
});
