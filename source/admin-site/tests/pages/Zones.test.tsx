import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  usePendingDevices,
  useDevices,
  useZoneExceptions,
  useCreateNetworkZone,
  useUpdateNetworkZone,
  useDeleteNetworkZone,
  useCreateZoneException,
  useDeleteZoneException,
  useQuarantineNewDevices,
  useSetQuarantineNewDevices,
  useAssignDeviceZone,
} = vi.hoisted(() => ({
  usePendingDevices: vi.fn(),
  useDevices: vi.fn(),
  useZoneExceptions: vi.fn(),
  useCreateNetworkZone: vi.fn(),
  useUpdateNetworkZone: vi.fn(),
  useDeleteNetworkZone: vi.fn(),
  useCreateZoneException: vi.fn(),
  useDeleteZoneException: vi.fn(),
  useQuarantineNewDevices: vi.fn(),
  useSetQuarantineNewDevices: vi.fn(),
  useAssignDeviceZone: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    usePendingDevices,
    useDevices,
    useZoneExceptions,
    useCreateNetworkZone,
    useUpdateNetworkZone,
    useDeleteNetworkZone,
    useCreateZoneException,
    useDeleteZoneException,
    useQuarantineNewDevices,
    useSetQuarantineNewDevices,
    useAssignDeviceZone,
  };
});

// The cards are mocked to thin harnesses that surface the page's wiring: they
// expose the hoisted callbacks and derived flags so the tests assert what the
// page wired, not the cards' own rendering.
vi.mock("@/components/features/ZonesCard", () => ({
  ZonesCard: ({
    zones,
    isSaving,
    onCreateZone,
    onUpdateZone,
    onSetHome,
    onSetDefaultForNew,
    onDeleteZone,
  }: {
    zones: Array<{ id: string }>;
    isSaving: boolean;
    onCreateZone: (body: unknown, cb?: unknown) => void;
    onUpdateZone: (change: unknown, cb?: unknown) => void;
    onSetHome: (id: string) => void;
    onSetDefaultForNew: (id: string) => void;
    onDeleteZone: (id: string) => void;
  }) => (
    <div data-testid="zones-card">
      <span data-testid="zones-count">{zones.length}</span>
      <span data-testid="zones-saving">{String(isSaving)}</span>
      <button onClick={() => onCreateZone({ name: "Cameras" })}>
        create-zone
      </button>
      <button onClick={() => onUpdateZone({ id: "z1", body: {} })}>
        update-zone
      </button>
      <button onClick={() => onSetHome("z1")}>set-home</button>
      <button onClick={() => onSetDefaultForNew("z1")}>set-dfn</button>
      <button onClick={() => onDeleteZone("z1")}>delete-zone</button>
    </div>
  ),
}));

vi.mock("@/components/features/ZoneExceptionsCard", () => ({
  ZoneExceptionsCard: ({
    exceptions,
    zones,
    devices,
    onCreateException,
    onDeleteException,
  }: {
    exceptions: Array<{ id: string }>;
    zones: Array<{ id: string }>;
    devices: Array<{ id: string }>;
    onCreateException: (body: unknown, cb?: unknown) => void;
    onDeleteException: (id: string) => void;
  }) => (
    <div data-testid="exceptions-card">
      <span data-testid="exceptions-count">{exceptions.length}</span>
      <span data-testid="exceptions-zones">{zones.length}</span>
      <span data-testid="exceptions-devices">{devices.length}</span>
      <button onClick={() => onCreateException({ bidirectional: true })}>
        create-exception
      </button>
      <button onClick={() => onDeleteException("e1")}>delete-exception</button>
    </div>
  ),
}));

vi.mock("@/components/features/QuarantineSettingsCard", () => ({
  QuarantineSettingsCard: ({
    notifyEnabled,
    onSetNotify,
    onSetDefaultForNew,
    pending,
    onApprove,
    approvingDeviceIds,
  }: {
    notifyEnabled: boolean;
    onSetNotify: (enabled: boolean) => void;
    onSetDefaultForNew: (id: string) => void;
    pending: Array<{ id: string }>;
    onApprove: (deviceId: string, zoneId: string) => void;
    approvingDeviceIds: string[];
  }) => (
    <div data-testid="quarantine-card">
      <span data-testid="notify-enabled">{String(notifyEnabled)}</span>
      <span data-testid="pending-count">{pending.length}</span>
      <span data-testid="approving-ids">
        {approvingDeviceIds.join(",") || "-"}
      </span>
      <button onClick={() => onSetNotify(true)}>set-notify</button>
      <button onClick={() => onSetDefaultForNew("z2")}>
        quarantine-set-dfn
      </button>
      <button onClick={() => onApprove("d1", "z1")}>approve</button>
      <button onClick={() => onApprove("d2", "z1")}>approve-2</button>
    </div>
  ),
}));

import Zones from "@/pages/Zones";
import { renderWithProviders } from "../test-utils";

const createZoneMutate = vi.fn();
const updateZoneMutate = vi.fn();
const deleteZoneMutate = vi.fn();
const createExceptionMutate = vi.fn();
const deleteExceptionMutate = vi.fn();
const setQuarantineMutate = vi.fn();
const assignMutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  usePendingDevices.mockReturnValue({
    pending: [{ id: "d-new" }],
    defaultForNew: { id: "z2" },
    homeZone: { id: "z1" },
    zones: [{ id: "z1" }, { id: "z2" }],
  });
  useDevices.mockReturnValue({ data: { devices: [{ id: "d1" }] } });
  useZoneExceptions.mockReturnValue({ data: { exceptions: [{ id: "e1" }] } });
  useCreateNetworkZone.mockReturnValue({
    mutate: createZoneMutate,
    isPending: false,
  });
  useUpdateNetworkZone.mockReturnValue({
    mutate: updateZoneMutate,
    isPending: false,
  });
  useDeleteNetworkZone.mockReturnValue({ mutate: deleteZoneMutate });
  useCreateZoneException.mockReturnValue({
    mutate: createExceptionMutate,
    isPending: false,
  });
  useDeleteZoneException.mockReturnValue({ mutate: deleteExceptionMutate });
  useQuarantineNewDevices.mockReturnValue({ data: { enabled: true } });
  useSetQuarantineNewDevices.mockReturnValue({
    mutate: setQuarantineMutate,
    isPending: false,
  });
  useAssignDeviceZone.mockReturnValue({
    mutate: assignMutate,
    isPending: false,
    variables: undefined,
  });
});

describe("Zones", () => {
  it("passes the shared zone/device/exception data down to the cards", () => {
    renderWithProviders(<Zones />);
    expect(screen.getByTestId("zones-count")).toHaveTextContent("2");
    expect(screen.getByTestId("exceptions-count")).toHaveTextContent("1");
    expect(screen.getByTestId("exceptions-zones")).toHaveTextContent("2");
    expect(screen.getByTestId("exceptions-devices")).toHaveTextContent("1");
    expect(screen.getByTestId("notify-enabled")).toHaveTextContent("true");
    expect(screen.getByTestId("pending-count")).toHaveTextContent("1");
  });

  it("wires the zone lifecycle mutations to the ZonesCard callbacks", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Zones />);
    await user.click(screen.getByText("create-zone"));
    expect(createZoneMutate).toHaveBeenCalledWith({ name: "Cameras" });
    await user.click(screen.getByText("update-zone"));
    expect(updateZoneMutate).toHaveBeenCalledWith({ id: "z1", body: {} });
    await user.click(screen.getByText("delete-zone"));
    expect(deleteZoneMutate).toHaveBeenCalledWith("z1");
  });

  it("promotes zones via dedicated update mutations with promotion bodies", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Zones />);
    await user.click(screen.getByText("set-home"));
    expect(updateZoneMutate).toHaveBeenCalledWith({
      id: "z1",
      body: { is_default: true },
    });
    await user.click(screen.getByText("set-dfn"));
    expect(updateZoneMutate).toHaveBeenCalledWith({
      id: "z1",
      body: { is_default_for_new: true },
    });
  });

  it("derives isSaving for ZonesCard from the create/update mutations", () => {
    useCreateNetworkZone.mockReturnValue({
      mutate: createZoneMutate,
      isPending: true,
    });
    renderWithProviders(<Zones />);
    expect(screen.getByTestId("zones-saving")).toHaveTextContent("true");
  });

  it("wires the exception mutations to the ZoneExceptionsCard callbacks", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Zones />);
    await user.click(screen.getByText("create-exception"));
    expect(createExceptionMutate).toHaveBeenCalledWith({
      bidirectional: true,
    });
    await user.click(screen.getByText("delete-exception"));
    expect(deleteExceptionMutate).toHaveBeenCalledWith("e1");
  });

  it("wires the quarantine toggle, promotion, and approval mutations", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Zones />);
    await user.click(screen.getByText("set-notify"));
    expect(setQuarantineMutate).toHaveBeenCalledWith(true);
    await user.click(screen.getByText("quarantine-set-dfn"));
    expect(updateZoneMutate).toHaveBeenCalledWith({
      id: "z2",
      body: { is_default_for_new: true },
    });
    await user.click(screen.getByText("approve"));
    expect(assignMutate).toHaveBeenCalledWith(
      { deviceId: "d1", zoneId: "z1" },
      expect.objectContaining({ onSettled: expect.any(Function) }),
    );
  });

  it("falls back to empty data while the queries are unresolved", () => {
    usePendingDevices.mockReturnValue({
      pending: [],
      defaultForNew: undefined,
      homeZone: undefined,
      zones: [],
    });
    useDevices.mockReturnValue({ data: undefined });
    useZoneExceptions.mockReturnValue({ data: undefined });
    useQuarantineNewDevices.mockReturnValue({ data: undefined });
    renderWithProviders(<Zones />);
    expect(screen.getByTestId("exceptions-count")).toHaveTextContent("0");
    expect(screen.getByTestId("exceptions-devices")).toHaveTextContent("0");
    expect(screen.getByTestId("notify-enabled")).toHaveTextContent("false");
    expect(screen.getByTestId("approving-ids")).toHaveTextContent("-");
  });

  it("keeps every in-flight approval marked until its own request settles", async () => {
    const user = userEvent.setup();
    // Capture each call's onSettled so the test can settle them out of order.
    const settlers: Array<() => void> = [];
    assignMutate.mockImplementation(
      (_vars: unknown, opts: { onSettled: () => void }) => {
        settlers.push(opts.onSettled);
      },
    );
    renderWithProviders(<Zones />);
    await user.click(screen.getByText("approve"));
    await user.click(screen.getByText("approve-2"));
    // Both approvals are in flight — the first must NOT be un-marked by the
    // second click (the latest-variables race this state exists to prevent).
    expect(screen.getByTestId("approving-ids")).toHaveTextContent("d1,d2");
    act(() => settlers[0]());
    expect(screen.getByTestId("approving-ids")).toHaveTextContent("d2");
    act(() => settlers[1]());
    expect(screen.getByTestId("approving-ids")).toHaveTextContent("-");
  });
});
