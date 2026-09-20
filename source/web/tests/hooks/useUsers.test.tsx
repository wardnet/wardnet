import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { userService, deviceService, toast } = vi.hoisted(() => ({
  userService: {
    list: vi.fn(),
    getById: vi.fn(),
    create: vi.fn(),
    updateProfile: vi.fn(),
    setEnabled: vi.fn(),
    setRole: vi.fn(),
    delete: vi.fn(),
    listCredentials: vi.fn(),
    unlinkOauth: vi.fn(),
    changeOwnPassword: vi.fn(),
    listEnrolments: vi.fn(),
    issueEnrolment: vi.fn(),
    revokeEnrolment: vi.fn(),
    redeemEnrolment: vi.fn(),
    availableMethods: vi.fn(),
    listOauthProviders: vi.fn(),
    startOauth: vi.fn(),
    configureOauthProvider: vi.fn(),
    clearOauthProvider: vi.fn(),
  },
  deviceService: { setOwner: vi.fn() },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ userService, deviceService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  useAuthMethods,
  useChangeOwnPassword,
  useClearOauthProvider,
  useConfigureOauthProvider,
  useCreateUser,
  useDeleteUser,
  useEnrolments,
  useIssueEnrolment,
  useOauthProviders,
  useRedeemEnrolment,
  useRevokeEnrolment,
  useSetDeviceOwner,
  useSetUserEnabled,
  useSetUserRole,
  useStartOauth,
  useUnlinkOauth,
  useUpdateUserProfile,
  useUser,
  useUserCredentials,
  useUsers,
} from "../../src/hooks/useUsers";

const ana = { id: "u-ana", display_name: "Ana", role: "admin", enabled: true };

describe("household identity hooks", () => {
  beforeEach(() => vi.clearAllMocks());

  it("fetches the directory", async () => {
    userService.list.mockResolvedValue([ana]);
    const { result } = renderHook(() => useUsers(), {
      wrapper: createQueryWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([ana]);
  });

  it("does not fetch one user until an id exists", async () => {
    const { result } = renderHook(() => useUser(undefined), {
      wrapper: createQueryWrapper(),
    });
    // A route param arrives on a later render; firing with `undefined` would
    // request `/users/undefined`.
    expect(result.current.fetchStatus).toBe("idle");
    expect(userService.getById).not.toHaveBeenCalled();
  });

  it("fetches one user once an id arrives", async () => {
    userService.getById.mockResolvedValue(ana);
    const { result } = renderHook(() => useUser("u-ana"), {
      wrapper: createQueryWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(userService.getById).toHaveBeenCalledWith("u-ana");
  });

  it("fetches enrolments once an id arrives", async () => {
    userService.listEnrolments.mockResolvedValue([]);
    const { result } = renderHook(() => useEnrolments("u-ana"), {
      wrapper: createQueryWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(userService.listEnrolments).toHaveBeenCalledWith("u-ana");
  });

  it("fetches credentials and enrolments only with an id", async () => {
    const { result: creds } = renderHook(() => useUserCredentials(undefined), {
      wrapper: createQueryWrapper(),
    });
    const { result: enrol } = renderHook(() => useEnrolments(undefined), {
      wrapper: createQueryWrapper(),
    });
    expect(creds.current.fetchStatus).toBe("idle");
    expect(enrol.current.fetchStatus).toBe("idle");

    userService.listCredentials.mockResolvedValue([]);
    const { result: withId } = renderHook(() => useUserCredentials("u-ana"), {
      wrapper: createQueryWrapper(),
    });
    await waitFor(() => expect(withId.current.isSuccess).toBe(true));
    expect(userService.listCredentials).toHaveBeenCalledWith("u-ana");
  });

  it("names the new user in the success toast", async () => {
    userService.create.mockResolvedValue(ana);
    const { result } = renderHook(() => useCreateUser(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ display_name: "Ana", role: "admin" });
    await waitFor(() => expect(toast.success).toHaveBeenCalled());
    expect(toast.success.mock.calls[0][0]).toContain("Ana");
  });

  it("says where a disabled user stands, not just that it worked", async () => {
    userService.setEnabled.mockResolvedValue({ ...ana, enabled: false });
    const { result } = renderHook(() => useSetUserEnabled(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ id: "u-ana", enabled: false });
    await waitFor(() => expect(toast.success).toHaveBeenCalled());
    expect(toast.success.mock.calls[0][0]).toContain("signed out everywhere");
  });

  it("surfaces the daemon's own refusal rather than a generic message", async () => {
    // The daemon refuses to disable the last enabled admin; that reason is
    // the difference between "why did that not work" and "ah, I am the only
    // admin".
    userService.setEnabled.mockRejectedValue(
      new Error("cannot disable the last admin"),
    );
    const { result } = renderHook(() => useSetUserEnabled(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ id: "u-ana", enabled: false });
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(toast.error).toHaveBeenCalledWith("cannot disable the last admin");
  });

  it("issues an invitation without a success toast", async () => {
    // The token is shown exactly once and the caller must render it; a toast
    // would imply the job was done while the one artefact that matters was
    // still on screen waiting to be copied.
    userService.issueEnrolment.mockResolvedValue({ token: "t" });
    const { result } = renderHook(() => useIssueEnrolment(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate("u-ana");
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(toast.success).not.toHaveBeenCalled();
  });

  it("warns that a password change signs the caller out too", async () => {
    userService.changeOwnPassword.mockResolvedValue(undefined);
    const { result } = renderHook(() => useChangeOwnPassword(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ currentPassword: "a", newPassword: "b" });
    await waitFor(() => expect(toast.success).toHaveBeenCalled());
    expect(toast.success.mock.calls[0][0]).toContain("sign in again");
  });

  it("redeems an invitation without a toast, so the page can speak", async () => {
    userService.redeemEnrolment.mockResolvedValue(ana);
    const { result } = renderHook(() => useRedeemEnrolment(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ token: "t", password: "p" });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(userService.redeemEnrolment).toHaveBeenCalledWith("t", "p");
  });

  it("reads the public and admin provider views from different calls", async () => {
    userService.availableMethods.mockResolvedValue({
      password: true,
      providers: [],
    });
    userService.listOauthProviders.mockResolvedValue([]);

    const { result: pub } = renderHook(() => useAuthMethods(), {
      wrapper: createQueryWrapper(),
    });
    const { result: adm } = renderHook(() => useOauthProviders(), {
      wrapper: createQueryWrapper(),
    });

    await waitFor(() => expect(pub.current.isSuccess).toBe(true));
    await waitFor(() => expect(adm.current.isSuccess).toBe(true));
    expect(userService.availableMethods).toHaveBeenCalled();
    expect(userService.listOauthProviders).toHaveBeenCalled();
  });

  it("passes the ceremony's intent straight through", async () => {
    userService.startOauth.mockResolvedValue("https://accounts.google.com/x");
    const { result } = renderHook(() => useStartOauth(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({
      provider: "google",
      returnTo: "admin_app",
      rememberMe: true,
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(userService.startOauth).toHaveBeenCalledWith("google", {
      returnTo: "admin_app",
      rememberMe: true,
    });
  });

  it("does not navigate on its own when a ceremony cannot start", async () => {
    userService.startOauth.mockRejectedValue(new Error("unreachable"));
    const { result } = renderHook(() => useStartOauth(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ provider: "google" });
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
  });

  it("deletes a user and refreshes the devices they owned", async () => {
    // `owner_user_id` was just nulled daemon-side, so a stale device list
    // would keep showing them as the owner.
    userService.delete.mockResolvedValue(undefined);
    const { result } = renderHook(() => useDeleteUser(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate("u-ana");
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("User deleted"),
    );
  });

  it("confirms a saved profile", async () => {
    userService.updateProfile.mockResolvedValue(ana);
    const { result } = renderHook(() => useUpdateUserProfile(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ id: "u-ana", body: { display_name: "Ana" } });
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Profile updated"),
    );
  });

  it("reports a failed profile save", async () => {
    userService.updateProfile.mockRejectedValue(new Error("nope"));
    const { result } = renderHook(() => useUpdateUserProfile(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ id: "u-ana", body: { display_name: "Ana" } });
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("Failed to update profile"),
    );
  });

  it("names the new role in the toast", async () => {
    userService.setRole.mockResolvedValue({ ...ana, role: "member" });
    const { result } = renderHook(() => useSetUserRole(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ id: "u-ana", role: "member" });
    await waitFor(() => expect(toast.success).toHaveBeenCalled());
    expect(toast.success.mock.calls[0][0]).toContain("member");
  });

  it("surfaces a refusal to demote the last admin", async () => {
    userService.setRole.mockRejectedValue(new Error("last admin"));
    const { result } = renderHook(() => useSetUserRole(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ id: "u-ana", role: "member" });
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("last admin"));
  });

  it("reports a failed delete with the daemon's reason", async () => {
    userService.delete.mockRejectedValue(new Error("cannot remove"));
    const { result } = renderHook(() => useDeleteUser(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate("u-ana");
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("cannot remove"),
    );
  });

  it("unlinks a provider", async () => {
    userService.unlinkOauth.mockResolvedValue(undefined);
    const { result } = renderHook(() => useUnlinkOauth(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ id: "u-ana", provider: "google" });
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Account unlinked"),
    );
  });

  it("reports a failed unlink", async () => {
    userService.unlinkOauth.mockRejectedValue(new Error("nope"));
    const { result } = renderHook(() => useUnlinkOauth(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ id: "u-ana", provider: "google" });
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("Failed to unlink account"),
    );
  });

  it("revokes an invitation", async () => {
    userService.revokeEnrolment.mockResolvedValue(undefined);
    const { result } = renderHook(() => useRevokeEnrolment(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ id: "u-ana", enrolmentId: "e1" });
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Invitation revoked"),
    );
  });

  it("reports a failed revoke and a failed issue", async () => {
    userService.revokeEnrolment.mockRejectedValue(new Error("nope"));
    const { result } = renderHook(() => useRevokeEnrolment(), {
      wrapper: createQueryWrapper(),
    });
    result.current.mutate({ id: "u-ana", enrolmentId: "e1" });
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("Failed to revoke invitation"),
    );

    vi.clearAllMocks();
    userService.issueEnrolment.mockRejectedValue(new Error("no password yet"));
    const { result: issue } = renderHook(() => useIssueEnrolment(), {
      wrapper: createQueryWrapper(),
    });
    issue.current.mutate("u-ana");
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("no password yet"),
    );
  });

  it("saves and clears a provider", async () => {
    userService.configureOauthProvider.mockResolvedValue({});
    const { result } = renderHook(() => useConfigureOauthProvider(), {
      wrapper: createQueryWrapper(),
    });
    result.current.mutate({
      provider: "google",
      body: { client_id: "id", enabled: true },
    });
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Provider saved"),
    );

    vi.clearAllMocks();
    userService.clearOauthProvider.mockResolvedValue(undefined);
    const { result: clear } = renderHook(() => useClearOauthProvider(), {
      wrapper: createQueryWrapper(),
    });
    clear.current.mutate("google");
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        "Provider configuration removed",
      ),
    );
  });

  it("reports provider failures", async () => {
    userService.configureOauthProvider.mockRejectedValue(new Error("bad id"));
    const { result } = renderHook(() => useConfigureOauthProvider(), {
      wrapper: createQueryWrapper(),
    });
    result.current.mutate({
      provider: "google",
      body: { client_id: "", enabled: true },
    });
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("bad id"));

    vi.clearAllMocks();
    userService.clearOauthProvider.mockRejectedValue(new Error("nope"));
    const { result: clear } = renderHook(() => useClearOauthProvider(), {
      wrapper: createQueryWrapper(),
    });
    clear.current.mutate("google");
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("Failed to remove provider"),
    );
  });

  it("reports a failed create, password change and redemption", async () => {
    userService.create.mockRejectedValue(new Error("dup"));
    const { result } = renderHook(() => useCreateUser(), {
      wrapper: createQueryWrapper(),
    });
    result.current.mutate({ display_name: "Ana", role: "admin" });
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("Failed to add user"),
    );

    vi.clearAllMocks();
    userService.changeOwnPassword.mockRejectedValue(new Error("wrong current"));
    const { result: pw } = renderHook(() => useChangeOwnPassword(), {
      wrapper: createQueryWrapper(),
    });
    pw.current.mutate({ currentPassword: "a", newPassword: "b" });
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("wrong current"),
    );

    vi.clearAllMocks();
    userService.redeemEnrolment.mockRejectedValue(new Error("spent"));
    const { result: rd } = renderHook(() => useRedeemEnrolment(), {
      wrapper: createQueryWrapper(),
    });
    rd.current.mutate({ token: "t", password: "p" });
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("spent"));
  });

  it("reports a failed device-owner assignment", async () => {
    deviceService.setOwner.mockRejectedValue(new Error("nope"));
    const { result } = renderHook(() => useSetDeviceOwner(), {
      wrapper: createQueryWrapper(),
    });
    result.current.mutate({ deviceId: "d1", ownerUserId: null });
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith("Failed to update device owner"),
    );
  });

  it("assigns a device owner", async () => {
    deviceService.setOwner.mockResolvedValue({});
    const { result } = renderHook(() => useSetDeviceOwner(), {
      wrapper: createQueryWrapper(),
    });

    result.current.mutate({ deviceId: "d1", ownerUserId: "u-ana" });
    await waitFor(() => expect(toast.success).toHaveBeenCalled());
    expect(deviceService.setOwner).toHaveBeenCalledWith("d1", "u-ana");
  });
});
