import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { networkService } = vi.hoisted(() => ({
  networkService: {
    getStatus: vi.fn(),
    discoverGatewayMac: vi.fn(),
    dhcpSelfProbe: vi.fn(),
  },
}));
vi.mock("../../src/lib/sdk", () => ({ networkService }));

import {
  useNetworkStatus,
  useDiscoverGatewayMac,
  useDhcpSelfProbe,
} from "../../src/hooks/useNetwork";

describe("useNetwork hooks", () => {
  beforeEach(() => vi.clearAllMocks());

  it("reads the LAN status", async () => {
    networkService.getStatus.mockResolvedValue({ iface: "eth0" });
    const { result } = renderHook(() => useNetworkStatus(), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ iface: "eth0" });
  });

  it("discovers the gateway MAC with an empty body by default", async () => {
    networkService.discoverGatewayMac.mockResolvedValue({ mac: "aa" });
    const { result } = renderHook(() => useDiscoverGatewayMac(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync(undefined as never);
    });
    expect(networkService.discoverGatewayMac).toHaveBeenCalledWith({});
  });

  it("runs the DHCP self-probe", async () => {
    networkService.dhcpSelfProbe.mockResolvedValue({ responder: null });
    const { result } = renderHook(() => useDhcpSelfProbe(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync();
    });
    expect(networkService.dhcpSelfProbe).toHaveBeenCalledOnce();
  });
});
