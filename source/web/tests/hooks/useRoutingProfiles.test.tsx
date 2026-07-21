import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

// Mock the SDK singleton the hooks call. `vi.hoisted` runs before the hoisted
// `vi.mock` factory so the factory can close over the spies safely.
const { routingProfilesService } = vi.hoisted(() => ({
  routingProfilesService: {
    listProfiles: vi.fn(),
    getProfile: vi.fn(),
    createProfile: vi.fn(),
    updateProfile: vi.fn(),
    deleteProfile: vi.fn(),
    listRules: vi.fn(),
    createRule: vi.fn(),
    updateRule: vi.fn(),
    deleteRule: vi.fn(),
    getDeviceProfiles: vi.fn(),
    setDeviceProfiles: vi.fn(),
    listProfileDevices: vi.fn(),
  },
}));
vi.mock("../../src/lib/sdk", () => ({ routingProfilesService }));

const { toast } = vi.hoisted(() => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  useRoutingProfiles,
  useRoutingProfile,
  useCreateRoutingProfile,
  useUpdateRoutingProfile,
  useDeleteRoutingProfile,
  useDomainRoutingRules,
  useCreateDomainRoutingRule,
  useUpdateDomainRoutingRule,
  useDeleteDomainRoutingRule,
  useDeviceRoutingProfiles,
  useSetDeviceRoutingProfiles,
  useProfileDevices,
} from "../../src/hooks/useRoutingProfiles";

describe("useRoutingProfiles hooks", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists profiles via listProfiles", async () => {
    const profiles = [{ id: "p1", name: "Streaming" }];
    routingProfilesService.listProfiles.mockResolvedValue({ profiles });

    const { result } = renderHook(() => useRoutingProfiles(), {
      wrapper: createQueryWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(routingProfilesService.listProfiles).toHaveBeenCalledOnce();
    expect(result.current.data).toEqual({ profiles });
  });

  it("fetches a single profile only when an id is given", async () => {
    routingProfilesService.getProfile.mockResolvedValue({
      profile: { id: "p1" },
    });

    const { result } = renderHook(() => useRoutingProfile("p1"), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(routingProfilesService.getProfile).toHaveBeenCalledWith("p1");
  });

  it("does not fetch a profile when the id is undefined", () => {
    renderHook(() => useRoutingProfile(undefined), {
      wrapper: createQueryWrapper(),
    });
    expect(routingProfilesService.getProfile).not.toHaveBeenCalled();
  });

  it("creates a profile and toasts success", async () => {
    routingProfilesService.createProfile.mockResolvedValue({
      profile: { id: "p1" },
      message: "created",
    });

    const { result } = renderHook(() => useCreateRoutingProfile(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ name: "Streaming" });
    });
    expect(routingProfilesService.createProfile).toHaveBeenCalledWith({
      name: "Streaming",
    });
    expect(toast.success).toHaveBeenCalled();
  });

  it("renames a profile via updateProfile", async () => {
    routingProfilesService.updateProfile.mockResolvedValue({
      profile: { id: "p1" },
      message: "updated",
    });

    const { result } = renderHook(() => useUpdateRoutingProfile(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ id: "p1", body: { name: "New" } });
    });
    expect(routingProfilesService.updateProfile).toHaveBeenCalledWith("p1", {
      name: "New",
    });
  });

  it("deletes a profile via deleteProfile", async () => {
    routingProfilesService.deleteProfile.mockResolvedValue({ message: "gone" });

    const { result } = renderHook(() => useDeleteRoutingProfile(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("p1");
    });
    expect(routingProfilesService.deleteProfile).toHaveBeenCalledWith("p1");
  });

  it("lists a profile's rules via listRules", async () => {
    routingProfilesService.listRules.mockResolvedValue({ rules: [] });

    const { result } = renderHook(() => useDomainRoutingRules("p1"), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(routingProfilesService.listRules).toHaveBeenCalledWith("p1");
  });

  it("creates a rule with (profileId, body)", async () => {
    routingProfilesService.createRule.mockResolvedValue({
      rule: { id: "r1" },
      message: "ok",
    });
    const body = {
      pattern: "*.netflix.com",
      target: { type: "direct" as const },
      enabled: true,
    };

    const { result } = renderHook(() => useCreateDomainRoutingRule(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ profileId: "p1", body });
    });
    expect(routingProfilesService.createRule).toHaveBeenCalledWith("p1", body);
  });

  it("updates a rule with (ruleId, body)", async () => {
    routingProfilesService.updateRule.mockResolvedValue({
      rule: { id: "r1" },
      message: "ok",
    });

    const { result } = renderHook(() => useUpdateDomainRoutingRule(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({
        ruleId: "r1",
        body: { enabled: false },
      });
    });
    expect(routingProfilesService.updateRule).toHaveBeenCalledWith("r1", {
      enabled: false,
    });
  });

  it("deletes a rule via deleteRule", async () => {
    routingProfilesService.deleteRule.mockResolvedValue({ message: "gone" });

    const { result } = renderHook(() => useDeleteDomainRoutingRule(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync("r1");
    });
    expect(routingProfilesService.deleteRule).toHaveBeenCalledWith("r1");
  });

  it("reads a device's assigned profiles via getDeviceProfiles", async () => {
    routingProfilesService.getDeviceProfiles.mockResolvedValue({
      profile_ids: ["p1"],
    });

    const { result } = renderHook(() => useDeviceRoutingProfiles("d1"), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(routingProfilesService.getDeviceProfiles).toHaveBeenCalledWith("d1");
  });

  it("sets device profiles, wrapping the ordered ids into { profile_ids }", async () => {
    routingProfilesService.setDeviceProfiles.mockResolvedValue({
      message: "ok",
    });

    const { result } = renderHook(() => useSetDeviceRoutingProfiles(), {
      wrapper: createQueryWrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({
        deviceId: "d1",
        profileIds: ["p2", "p1"],
      });
    });
    expect(routingProfilesService.setDeviceProfiles).toHaveBeenCalledWith(
      "d1",
      {
        profile_ids: ["p2", "p1"],
      },
    );
  });

  it("reverse-lists a profile's devices via listProfileDevices", async () => {
    routingProfilesService.listProfileDevices.mockResolvedValue({
      device_ids: ["d1"],
    });

    const { result } = renderHook(() => useProfileDevices("p1"), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(routingProfilesService.listProfileDevices).toHaveBeenCalledWith(
      "p1",
    );
  });
});
