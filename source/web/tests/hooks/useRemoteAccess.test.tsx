import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { remoteAccessService } = vi.hoisted(() => ({
  remoteAccessService: {
    requestEnrollmentCode: vi.fn(),
    enroll: vi.fn(),
    checkSlug: vi.fn(),
    register: vi.fn(),
    configureCloudflare: vi.fn(),
    ddnsStatus: vi.fn(),
    resolutionCheck: vi.fn(),
    teardown: vi.fn(),
    tlsStatus: vi.fn(),
  },
}));
vi.mock("../../src/lib/sdk", () => ({ remoteAccessService }));

import {
  useRequestEnrollmentCode,
  useEnrollDdns,
  useCheckDdnsSlug,
  useRegisterDdns,
  useConfigureCloudflare,
  useDdnsStatus,
  useResolutionCheck,
  useDeleteDdns,
  useTlsStatus,
} from "../../src/hooks/useRemoteAccess";

const wrapper = createQueryWrapper;

describe("useRemoteAccess mutations", () => {
  beforeEach(() => vi.clearAllMocks());

  it.each([
    [
      "enrollment code",
      useRequestEnrollmentCode,
      "requestEnrollmentCode",
      { email: "a@b.c" },
    ],
    ["enroll", useEnrollDdns, "enroll", { code: "x" }],
    ["check slug", useCheckDdnsSlug, "checkSlug", "myslug"],
    ["register", useRegisterDdns, "register", { slug: "s" }],
    [
      "configure cloudflare",
      useConfigureCloudflare,
      "configureCloudflare",
      { token: "t" },
    ],
  ] as const)("%s calls its service method", async (_n, hook, method, arg) => {
    // eslint-disable-next-line security/detect-object-injection -- method is a literal from an it.each as-const tuple in a test; no external key
    (remoteAccessService[method] as ReturnType<typeof vi.fn>).mockResolvedValue(
      {},
    );
    const { result } = renderHook(() => hook(), { wrapper: wrapper() });
    await act(async () => {
      await result.current.mutateAsync(arg as never);
    });
    // eslint-disable-next-line security/detect-object-injection -- method is a literal from an it.each as-const tuple in a test; no external key
    expect(remoteAccessService[method]).toHaveBeenCalledWith(arg);
  });

  it("tears down remote access", async () => {
    remoteAccessService.teardown.mockResolvedValue({});
    const { result } = renderHook(() => useDeleteDdns(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync();
    });
    expect(remoteAccessService.teardown).toHaveBeenCalledOnce();
  });
});

describe("useRemoteAccess queries", () => {
  beforeEach(() => vi.clearAllMocks());

  it("reads DDNS status when enabled", async () => {
    remoteAccessService.ddnsStatus.mockResolvedValue({ configured: false });
    const { result } = renderHook(() => useDdnsStatus(), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
  });

  it("does not fetch DDNS status when disabled", () => {
    const { result } = renderHook(() => useDdnsStatus(false), {
      wrapper: wrapper(),
    });
    expect(result.current.fetchStatus).toBe("idle");
  });

  it("runs the resolution check", async () => {
    remoteAccessService.resolutionCheck.mockResolvedValue({ resolves: true });
    const { result } = renderHook(() => useResolutionCheck(), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
  });

  it("reads TLS status", async () => {
    remoteAccessService.tlsStatus.mockResolvedValue({ phase: "issued" });
    const { result } = renderHook(() => useTlsStatus(), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ phase: "issued" });
  });
});
