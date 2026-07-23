import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  useDevices,
  usePrivateDnsStatus,
  useSetPrivateDnsEnabled,
  useGrantPrivateDnsDevice,
  useRevokePrivateDnsDevice,
  useSendPrivateDnsToDevice,
} from "@wardnet/web";
import { PrivateDnsCard } from "@/components/features/PrivateDnsCard";
import { renderWithProviders, makeDevice } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDevices: vi.fn(),
    usePrivateDnsStatus: vi.fn(),
    useSetPrivateDnsEnabled: vi.fn(),
    useGrantPrivateDnsDevice: vi.fn(),
    useRevokePrivateDnsDevice: vi.fn(),
    useSendPrivateDnsToDevice: vi.fn(),
  };
});

const setEnabledMutate = vi.fn();

type Prereqs = {
  wardnet_provider: boolean;
  certificate: boolean;
  entitled: boolean;
  relay_connected: boolean;
};

function setup({
  enabled = false,
  prerequisites,
  grants = [] as Array<{
    id: string;
    device_id: string;
    hostname: string;
    created_at: string;
  }>,
  devices = [] as ReturnType<typeof makeDevice>[],
}: {
  enabled?: boolean;
  prerequisites: Prereqs;
  grants?: Array<{
    id: string;
    device_id: string;
    hostname: string;
    created_at: string;
  }>;
  devices?: ReturnType<typeof makeDevice>[];
}) {
  vi.mocked(usePrivateDnsStatus).mockReturnValue({
    data: { enabled, domain: "abc.my.wardnet.services", prerequisites, grants },
    isLoading: false,
  } as unknown as ReturnType<typeof usePrivateDnsStatus>);
  vi.mocked(useDevices).mockReturnValue({
    data: { devices },
  } as unknown as ReturnType<typeof useDevices>);
  vi.mocked(useSetPrivateDnsEnabled).mockReturnValue({
    mutate: setEnabledMutate,
    isPending: false,
  } as unknown as ReturnType<typeof useSetPrivateDnsEnabled>);
  const idleMutation = {
    mutate: vi.fn(),
    mutateAsync: vi.fn(),
    isPending: false,
  };
  vi.mocked(useGrantPrivateDnsDevice).mockReturnValue(
    idleMutation as unknown as ReturnType<typeof useGrantPrivateDnsDevice>,
  );
  vi.mocked(useRevokePrivateDnsDevice).mockReturnValue(
    idleMutation as unknown as ReturnType<typeof useRevokePrivateDnsDevice>,
  );
  vi.mocked(useSendPrivateDnsToDevice).mockReturnValue(
    idleMutation as unknown as ReturnType<typeof useSendPrivateDnsToDevice>,
  );
}

const ALL_MET: Prereqs = {
  wardnet_provider: true,
  certificate: true,
  entitled: true,
  relay_connected: true,
};

describe("PrivateDnsCard", () => {
  beforeEach(() => vi.clearAllMocks());

  it("renders the prerequisite checklist and disables the toggle when a hard gate is unmet", () => {
    setup({ prerequisites: { ...ALL_MET, entitled: false } });
    renderWithProviders(<PrivateDnsCard />);

    expect(screen.getByTestId("private-dns-prerequisites")).toBeInTheDocument();
    // The unmet-Premium hint is shown, and the toggle can't be enabled.
    expect(screen.getByText(/Premium feature/)).toBeInTheDocument();
    expect(
      screen.getByRole("switch", { name: "Enable Private DNS" }),
    ).toBeDisabled();
  });

  it("allows enabling once every hard gate is met", async () => {
    setup({ prerequisites: ALL_MET });
    renderWithProviders(<PrivateDnsCard />);

    const toggle = screen.getByRole("switch", { name: "Enable Private DNS" });
    expect(toggle).toBeEnabled();
    await userEvent.click(toggle);
    expect(setEnabledMutate).toHaveBeenCalledWith(true);
  });

  it("lists granted devices with their hostname once enabled", () => {
    const device = makeDevice({ id: "d1", name: "Alice phone" });
    setup({
      enabled: true,
      prerequisites: ALL_MET,
      devices: [device],
      grants: [
        {
          id: "g1",
          device_id: "d1",
          hostname: "tok.abc.my.wardnet.services",
          created_at: "2026-07-21T00:00:00Z",
        },
      ],
    });
    renderWithProviders(<PrivateDnsCard />);

    expect(screen.getByText("Alice phone")).toBeInTheDocument();
    expect(screen.getByText("tok.abc.my.wardnet.services")).toBeInTheDocument();
    // The one granted device is no longer grantable, so no grant button.
    expect(screen.queryByText("Grant access")).not.toBeInTheDocument();
  });
});
