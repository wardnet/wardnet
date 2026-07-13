import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { dnsFilterService, jobsService, toast } = vi.hoisted(() => ({
  dnsFilterService: {
    listProfiles: vi.fn(),
    getProfile: vi.fn(),
    createProfile: vi.fn(),
    updateProfile: vi.fn(),
    deleteProfile: vi.fn(),
    listBlocklists: vi.fn(),
    createBlocklist: vi.fn(),
    updateBlocklist: vi.fn(),
    deleteBlocklist: vi.fn(),
    refreshBlocklist: vi.fn(),
    listAllowlist: vi.fn(),
    createAllowlistEntry: vi.fn(),
    deleteAllowlistEntry: vi.fn(),
    listFilterRules: vi.fn(),
    createFilterRule: vi.fn(),
    updateFilterRule: vi.fn(),
    deleteFilterRule: vi.fn(),
    listDeviceSettings: vi.fn(),
    getDeviceSettings: vi.fn(),
    updateDeviceSettings: vi.fn(),
    getConfig: vi.fn(),
    updateConfig: vi.fn(),
  },
  jobsService: { get: vi.fn() },
  toast: { success: vi.fn(), error: vi.fn(), loading: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ dnsFilterService, jobsService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import * as f from "../../src/hooks/useDnsFilter";

const w = createQueryWrapper;
const PID = "profile-1";

describe("useDnsFilter profiles", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists profiles and gets one by id", async () => {
    dnsFilterService.listProfiles.mockResolvedValue({ profiles: [] });
    dnsFilterService.getProfile.mockResolvedValue({ profile: {} });
    const { result: l } = renderHook(() => f.useDnsFilterProfiles(), {
      wrapper: w(),
    });
    await waitFor(() => expect(l.current.isSuccess).toBe(true));

    const { result: g } = renderHook(() => f.useDnsFilterProfile(PID), {
      wrapper: w(),
    });
    await waitFor(() => expect(g.current.isSuccess).toBe(true));
    expect(dnsFilterService.getProfile).toHaveBeenCalledWith(PID);

    const { result: none } = renderHook(
      () => f.useDnsFilterProfile(undefined),
      { wrapper: w() },
    );
    expect(none.current.fetchStatus).toBe("idle");
  });

  it("creates and updates a profile", async () => {
    dnsFilterService.createProfile.mockResolvedValue({ message: "created" });
    dnsFilterService.updateProfile.mockResolvedValue({ message: "" });
    const { result: c } = renderHook(() => f.useCreateDnsFilterProfile(), {
      wrapper: w(),
    });
    await act(async () => {
      await c.current.mutateAsync({ name: "Kids" });
    });
    expect(toast.success).toHaveBeenCalledWith("created");

    const { result: u } = renderHook(() => f.useUpdateDnsFilterProfile(), {
      wrapper: w(),
    });
    await act(async () => {
      await u.current.mutateAsync({ id: PID, body: { name: "Kids2" } });
    });
    expect(toast.success).toHaveBeenCalledWith("Profile updated");
  });

  it("maps a 409 delete to the builtin-profile error copy", async () => {
    dnsFilterService.deleteProfile.mockRejectedValue({ status: 409 });
    const { result } = renderHook(() => f.useDeleteDnsFilterProfile(), {
      wrapper: w(),
    });
    await act(async () => {
      await result.current.mutateAsync(PID).catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith(
      "Builtin profiles cannot be deleted",
    );
  });

  it("maps a non-409 delete failure to the generic error", async () => {
    dnsFilterService.deleteProfile.mockRejectedValue({ status: 500 });
    const { result } = renderHook(() => f.useDeleteDnsFilterProfile(), {
      wrapper: w(),
    });
    await act(async () => {
      await result.current.mutateAsync(PID).catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Failed to delete profile");
  });
});

describe("useDnsFilter blocklists / allowlist / rules", () => {
  beforeEach(() => vi.clearAllMocks());

  it("runs the blocklist CRUD calls scoped to the profile", async () => {
    dnsFilterService.listBlocklists.mockResolvedValue({ blocklists: [] });
    dnsFilterService.createBlocklist.mockResolvedValue({ message: "" });
    dnsFilterService.updateBlocklist.mockResolvedValue({ message: "" });
    dnsFilterService.deleteBlocklist.mockResolvedValue({ message: "" });

    const { result: list } = renderHook(() => f.useBlocklists(PID), {
      wrapper: w(),
    });
    await waitFor(() => expect(list.current.isSuccess).toBe(true));

    const { result: create } = renderHook(() => f.useCreateBlocklist(PID), {
      wrapper: w(),
    });
    await act(async () => {
      await create.current.mutateAsync({ url: "http://x" } as never);
    });
    expect(dnsFilterService.createBlocklist).toHaveBeenCalledWith(PID, {
      url: "http://x",
    });

    const { result: update } = renderHook(() => f.useUpdateBlocklist(PID), {
      wrapper: w(),
    });
    await act(async () => {
      await update.current.mutateAsync({ id: "b1", body: { enabled: false } });
    });
    expect(dnsFilterService.updateBlocklist).toHaveBeenCalledWith(PID, "b1", {
      enabled: false,
    });

    const { result: del } = renderHook(() => f.useDeleteBlocklist(PID), {
      wrapper: w(),
    });
    await act(async () => {
      await del.current.mutateAsync("b1");
    });
    expect(dnsFilterService.deleteBlocklist).toHaveBeenCalledWith(PID, "b1");
  });

  it("refreshes a blocklist and tracks the job to completion", async () => {
    dnsFilterService.refreshBlocklist.mockResolvedValue({ job_id: "job-1" });
    jobsService.get.mockResolvedValue({
      id: "job-1",
      status: "SUCCEED",
      percentage_done: 100,
    });
    const { result } = renderHook(() => f.useRefreshBlocklist(PID), {
      wrapper: w(),
    });
    act(() => {
      result.current.mutate("b1");
    });
    await waitFor(() =>
      expect(dnsFilterService.refreshBlocklist).toHaveBeenCalledWith(PID, "b1"),
    );
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        "Blocklist refreshed",
        expect.any(Object),
      ),
    );
  });

  // Regression guard: importing a huge blocklist used to wedge the daemon long
  // enough for systemd's watchdog to kill it. The job registry is in-memory, so
  // after the restart `GET /api/jobs/{id}` 404s. The hook only reacted to
  // `data`, which stays undefined on error — so it polled forever and the toast
  // sat at "80%" indefinitely. A job that vanishes must surface as a failure.
  it("fails the refresh when the job disappears (daemon restart) instead of polling forever", async () => {
    dnsFilterService.refreshBlocklist.mockResolvedValue({ job_id: "job-1" });
    jobsService.get
      .mockResolvedValueOnce({
        id: "job-1",
        status: "RUNNING",
        percentage_done: 80,
      })
      .mockRejectedValue(
        Object.assign(new Error("job not found"), { status: 404 }),
      );

    const { result } = renderHook(() => f.useRefreshBlocklist(PID), {
      wrapper: w(),
    });
    act(() => {
      result.current.mutate("b1");
    });

    // The first poll succeeds, so the 404 only lands on the next tick of the
    // 1s refetch interval — but a 404 is final, so it is not retried past that.
    await waitFor(
      () =>
        expect(toast.error).toHaveBeenCalledWith(
          "Blocklist refresh failed",
          expect.any(Object),
        ),
      { timeout: 5_000 },
    );
    // And the row must stop showing "Updating…" forever.
    await waitFor(() => expect(result.current.isPending).toBe(false), {
      timeout: 5_000,
    });
  });

  // The counterpart: a blip while the daemon is busy importing must NOT be
  // reported as a failed refresh. The job is still running server-side, and
  // telling the user it failed pushes them into re-triggering a second
  // multi-million-domain import.
  it("rides out a transient poll failure instead of declaring the refresh failed", async () => {
    dnsFilterService.refreshBlocklist.mockResolvedValue({ job_id: "job-1" });
    jobsService.get
      .mockRejectedValueOnce(
        Object.assign(new Error("network"), { status: 503 }),
      )
      .mockResolvedValue({
        id: "job-1",
        status: "SUCCEED",
        percentage_done: 100,
      });

    const { result } = renderHook(() => f.useRefreshBlocklist(PID), {
      wrapper: w(),
    });
    act(() => {
      result.current.mutate("b1");
    });

    await waitFor(
      () =>
        expect(toast.success).toHaveBeenCalledWith(
          "Blocklist refreshed",
          expect.any(Object),
        ),
      { timeout: 10_000 },
    );
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("runs allowlist create/delete and rules CRUD", async () => {
    dnsFilterService.listAllowlist.mockResolvedValue({ entries: [] });
    dnsFilterService.createAllowlistEntry.mockResolvedValue({ message: "" });
    dnsFilterService.deleteAllowlistEntry.mockResolvedValue({ message: "" });
    dnsFilterService.listFilterRules.mockResolvedValue({ rules: [] });
    dnsFilterService.createFilterRule.mockResolvedValue({ message: "" });
    dnsFilterService.updateFilterRule.mockResolvedValue({ message: "" });
    dnsFilterService.deleteFilterRule.mockResolvedValue({ message: "" });

    const { result: al } = renderHook(() => f.useAllowlist(PID), {
      wrapper: w(),
    });
    await waitFor(() => expect(al.current.isSuccess).toBe(true));

    const { result: alc } = renderHook(() => f.useCreateAllowlistEntry(PID), {
      wrapper: w(),
    });
    await act(async () => {
      await alc.current.mutateAsync({ domain: "ok.com" });
    });
    expect(toast.success).toHaveBeenCalledWith("Domain allowlisted");

    const { result: ald } = renderHook(() => f.useDeleteAllowlistEntry(PID), {
      wrapper: w(),
    });
    await act(async () => {
      await ald.current.mutateAsync("a1");
    });
    expect(dnsFilterService.deleteAllowlistEntry).toHaveBeenCalledWith(
      PID,
      "a1",
    );

    const { result: rules } = renderHook(() => f.useFilterRules(PID), {
      wrapper: w(),
    });
    await waitFor(() => expect(rules.current.isSuccess).toBe(true));

    const { result: rc } = renderHook(() => f.useCreateFilterRule(PID), {
      wrapper: w(),
    });
    await act(async () => {
      await rc.current.mutateAsync({ domain: "x", action: "block" } as never);
    });
    expect(toast.success).toHaveBeenCalledWith("Filter rule added");

    const { result: ru } = renderHook(() => f.useUpdateFilterRule(PID), {
      wrapper: w(),
    });
    await act(async () => {
      await ru.current.mutateAsync({
        id: "r1",
        body: { action: "allow" } as never,
      });
    });
    expect(dnsFilterService.updateFilterRule).toHaveBeenCalledWith(PID, "r1", {
      action: "allow",
    });

    const { result: rd } = renderHook(() => f.useDeleteFilterRule(PID), {
      wrapper: w(),
    });
    await act(async () => {
      await rd.current.mutateAsync("r1");
    });
    expect(dnsFilterService.deleteFilterRule).toHaveBeenCalledWith(PID, "r1");
  });
});

describe("useDnsFilter device settings + config", () => {
  beforeEach(() => vi.clearAllMocks());

  it("collapses the enabled filter into a stable cache key", async () => {
    dnsFilterService.listDeviceSettings.mockResolvedValue({ settings: [] });
    for (const params of [{}, { enabled: true }, { enabled: false }]) {
      const { result } = renderHook(
        () => f.useDeviceFilterSettingsList(params),
        { wrapper: w() },
      );
      await waitFor(() => expect(result.current.isSuccess).toBe(true));
    }
    expect(dnsFilterService.listDeviceSettings).toHaveBeenCalledTimes(3);
  });

  it("reads and updates a single device's settings", async () => {
    dnsFilterService.getDeviceSettings.mockResolvedValue({ settings: {} });
    dnsFilterService.updateDeviceSettings.mockResolvedValue({ message: "" });
    const { result: get } = renderHook(() => f.useDeviceFilterSettings("d1"), {
      wrapper: w(),
    });
    await waitFor(() => expect(get.current.isSuccess).toBe(true));

    const { result: upd } = renderHook(
      () => f.useUpdateDeviceFilterSettings(),
      { wrapper: w() },
    );
    await act(async () => {
      await upd.current.mutateAsync({
        id: "d1",
        body: { enabled: true } as never,
      });
    });
    expect(toast.success).toHaveBeenCalledWith("DNS filter settings updated");
  });

  it("reads config and reports update success + error", async () => {
    dnsFilterService.getConfig.mockResolvedValue({ config: {} });
    const { result: get } = renderHook(() => f.useDnsFilterConfig(), {
      wrapper: w(),
    });
    await waitFor(() => expect(get.current.isSuccess).toBe(true));

    dnsFilterService.updateConfig.mockResolvedValueOnce({});
    const { result: upd } = renderHook(() => f.useUpdateDnsFilterConfig(), {
      wrapper: w(),
    });
    await act(async () => {
      await upd.current.mutateAsync({ default_profile_id: PID } as never);
    });
    expect(toast.success).toHaveBeenCalledWith(
      "DNS filter configuration updated",
      { id: "dns-filter-config-update" },
    );

    dnsFilterService.updateConfig.mockRejectedValueOnce(new Error("x"));
    await act(async () => {
      await upd.current
        .mutateAsync({ default_profile_id: PID } as never)
        .catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith(
      "Failed to update DNS filter configuration",
      { id: "dns-filter-config-update" },
    );
  });

  it("suppresses toasts when the update hook is silent", async () => {
    const { result: upd } = renderHook(
      () => f.useUpdateDnsFilterConfig({ silent: true }),
      { wrapper: w() },
    );

    dnsFilterService.updateConfig.mockResolvedValueOnce({});
    await act(async () => {
      await upd.current.mutateAsync({ default_profile_ids: [PID] } as never);
    });

    dnsFilterService.updateConfig.mockRejectedValueOnce(new Error("x"));
    await act(async () => {
      await upd.current
        .mutateAsync({ default_profile_ids: [PID] } as never)
        .catch(() => {});
    });

    expect(toast.success).not.toHaveBeenCalled();
    expect(toast.error).not.toHaveBeenCalled();
  });
});
