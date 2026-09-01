import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import type {
  ConfigureOauthProviderRequest,
  CreateUserRequest,
  OauthProvider,
  OauthReturnTo,
  UpdateUserProfileRequest,
  UserRole,
} from "@wardnet/js";

import { deviceService, userService } from "../lib/sdk";

/**
 * Household identity (ADR-0031).
 *
 * Two query key families, deliberately separate: `["users"]` is the directory
 * and everything hanging off a user, while `["auth-methods"]` is what the
 * sign-in surface may render. Configuring a provider invalidates both, because
 * it changes both the provider list an admin manages and the buttons an
 * unauthenticated visitor sees.
 */

/** Admin: every household user. */
export function useUsers() {
  return useQuery({
    queryKey: ["users"],
    queryFn: () => userService.list(),
  });
}

/** One household user. Readable by an admin, or by that user about themselves. */
export function useUser(id: string | undefined) {
  return useQuery({
    queryKey: ["users", id],
    queryFn: () => userService.getById(id!),
    enabled: Boolean(id),
  });
}

/** Admin: create a household user with no credential. */
export function useCreateUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateUserRequest) => userService.create(body),
    onSuccess: (user) => {
      toast.success(
        `${user.display_name} added — send them an invitation next`,
      );
      qc.invalidateQueries({ queryKey: ["users"] });
    },
    onError: () => toast.error("Failed to add user"),
  });
}

/** Update a user's display name and email. */
export function useUpdateUserProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      body,
    }: {
      id: string;
      body: UpdateUserProfileRequest;
    }) => userService.updateProfile(id, body),
    onSuccess: () => {
      toast.success("Profile updated");
      qc.invalidateQueries({ queryKey: ["users"] });
    },
    onError: () => toast.error("Failed to update profile"),
  });
}

/**
 * Admin: enable or disable a user.
 *
 * Disabling deletes every live session for that account, so it takes effect on
 * their next request rather than whenever their session happens to expire.
 */
export function useSetUserEnabled() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      userService.setEnabled(id, enabled),
    onSuccess: (user) => {
      toast.success(
        user.enabled
          ? `${user.display_name} can sign in again`
          : `${user.display_name} is disabled and signed out everywhere`,
      );
      qc.invalidateQueries({ queryKey: ["users"] });
    },
    // The daemon refuses to disable the last enabled admin. Surfacing its
    // message rather than a generic one is the difference between "why did
    // that not work" and "ah, I am the only admin".
    onError: (err: Error) =>
      toast.error(err.message || "Failed to update user"),
  });
}

/** Admin: change a user's role. Refuses to demote the last enabled admin. */
export function useSetUserRole() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, role }: { id: string; role: UserRole }) =>
      userService.setRole(id, role),
    onSuccess: (user) => {
      toast.success(`${user.display_name} is now ${user.role}`);
      qc.invalidateQueries({ queryKey: ["users"] });
    },
    onError: (err: Error) =>
      toast.error(err.message || "Failed to change role"),
  });
}

/** Admin: delete a user. Owned devices are unassigned, never deleted. */
export function useDeleteUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => userService.delete(id),
    onSuccess: () => {
      toast.success("User deleted");
      qc.invalidateQueries({ queryKey: ["users"] });
      // Device rows carry `owner_user_id`, which the daemon just nulled.
      qc.invalidateQueries({ queryKey: ["devices"] });
    },
    onError: (err: Error) =>
      toast.error(err.message || "Failed to delete user"),
  });
}

/** Admin: a user's credentials, without secrets. */
export function useUserCredentials(id: string | undefined) {
  return useQuery({
    queryKey: ["users", id, "credentials"],
    queryFn: () => userService.listCredentials(id!),
    enabled: Boolean(id),
  });
}

/** Admin: unlink a federated provider. Never touches the local password. */
export function useUnlinkOauth() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, provider }: { id: string; provider: OauthProvider }) =>
      userService.unlinkOauth(id, provider),
    onSuccess: (_data, { id }) => {
      toast.success("Account unlinked");
      qc.invalidateQueries({ queryKey: ["users", id, "credentials"] });
    },
    onError: () => toast.error("Failed to unlink account"),
  });
}

/** Change your own password, proving the current one first. */
export function useChangeOwnPassword() {
  return useMutation({
    mutationFn: ({
      currentPassword,
      newPassword,
    }: {
      currentPassword: string;
      newPassword: string;
    }) => userService.changeOwnPassword(currentPassword, newPassword),
    // Every session goes, including this one — the daemon revokes all of
    // them, because a password change is usually a response to "somebody may
    // know my password". Say so, rather than letting the next request 401 into
    // an unexplained bounce to the login screen.
    onSuccess: () =>
      toast.success("Password changed — sign in again with your new password"),
    onError: (err: Error) =>
      toast.error(err.message || "Failed to change password"),
  });
}

/** Admin: a user's outstanding and spent invitations. */
export function useEnrolments(id: string | undefined) {
  return useQuery({
    queryKey: ["users", id, "enrolments"],
    queryFn: () => userService.listEnrolments(id!),
    enabled: Boolean(id),
  });
}

/**
 * Admin: issue a one-time invitation.
 *
 * No success toast: the token in the response is shown **exactly once** and
 * the caller must render it. A toast would imply the job was done while the
 * one artefact that matters was still on screen waiting to be copied.
 */
export function useIssueEnrolment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => userService.issueEnrolment(id),
    onSuccess: (_invite, id) => {
      qc.invalidateQueries({ queryKey: ["users", id, "enrolments"] });
    },
    onError: (err: Error) =>
      toast.error(err.message || "Failed to create invitation"),
  });
}

/** Admin: revoke an unredeemed invitation. */
export function useRevokeEnrolment() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, enrolmentId }: { id: string; enrolmentId: string }) =>
      userService.revokeEnrolment(id, enrolmentId),
    onSuccess: (_data, { id }) => {
      toast.success("Invitation revoked");
      qc.invalidateQueries({ queryKey: ["users", id, "enrolments"] });
    },
    onError: () => toast.error("Failed to revoke invitation"),
  });
}

/**
 * Redeem a one-time enrolment invitation, setting a first password.
 *
 * Unauthenticated: the person redeeming has no credential yet, so there is no
 * session to carry. Deliberately no cache invalidation — nothing in this
 * client's cache belongs to them, because they were not signed in.
 */
export function useRedeemEnrolment() {
  return useMutation({
    mutationFn: ({ token, password }: { token: string; password: string }) =>
      userService.redeemEnrolment(token, password),
    onError: (err: Error) =>
      toast.error(err.message || "That invitation is not valid"),
  });
}

/**
 * Begin a federated sign-in and hand back the provider URL to navigate to.
 *
 * Returns the URL rather than navigating so the caller decides — a hook that
 * assigned `window.location` itself would be untestable and would fire on any
 * accidental re-render.
 */
export function useStartOauth() {
  return useMutation({
    mutationFn: ({
      provider,
      returnTo,
      rememberMe,
    }: {
      provider: OauthProvider;
      returnTo?: OauthReturnTo;
      rememberMe?: boolean;
    }) => userService.startOauth(provider, { returnTo, rememberMe }),
    onError: (err: Error) =>
      toast.error(err.message || "Could not start sign-in"),
  });
}

/**
 * Which sign-in methods this box can offer.
 *
 * Unauthenticated, so a sign-in screen can call it before anybody has a
 * session — that is the whole point of the endpoint.
 */
export function useAuthMethods() {
  return useQuery({
    queryKey: ["auth-methods"],
    queryFn: () => userService.availableMethods(),
  });
}

/**
 * Admin: every provider's full configuration.
 *
 * Distinct from {@link useAuthMethods}: that one is the unauthenticated
 * sign-in contract and carries neither the client ID nor the stored flag, so
 * it cannot drive a settings form.
 */
export function useOauthProviders() {
  return useQuery({
    queryKey: ["oauth-providers"],
    queryFn: () => userService.listOauthProviders(),
  });
}

/** Admin: configure a federated provider's client id and secret. */
export function useConfigureOauthProvider() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      provider,
      body,
    }: {
      provider: OauthProvider;
      body: ConfigureOauthProviderRequest;
    }) => userService.configureOauthProvider(provider, body),
    onSuccess: () => {
      toast.success("Provider saved");
      qc.invalidateQueries({ queryKey: ["auth-methods"] });
      qc.invalidateQueries({ queryKey: ["oauth-providers"] });
    },
    onError: (err: Error) =>
      toast.error(err.message || "Failed to save provider"),
  });
}

/** Admin: forget a provider's configuration, including its secret. */
export function useClearOauthProvider() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (provider: OauthProvider) =>
      userService.clearOauthProvider(provider),
    onSuccess: () => {
      toast.success("Provider configuration removed");
      qc.invalidateQueries({ queryKey: ["auth-methods"] });
      qc.invalidateQueries({ queryKey: ["oauth-providers"] });
    },
    onError: () => toast.error("Failed to remove provider"),
  });
}

/** Admin: assign or clear a device's owner. Attribution only, never access. */
export function useSetDeviceOwner() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      deviceId,
      ownerUserId,
    }: {
      deviceId: string;
      ownerUserId: string | null;
    }) => deviceService.setOwner(deviceId, ownerUserId),
    onSuccess: () => {
      toast.success("Device owner updated");
      qc.invalidateQueries({ queryKey: ["devices"] });
    },
    onError: () => toast.error("Failed to update device owner"),
  });
}
