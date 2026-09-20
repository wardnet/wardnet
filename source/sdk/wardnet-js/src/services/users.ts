import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
import type {
  AuthMethods,
  ConfigureOauthProviderRequest,
  CreateUserRequest,
  Enrolment,
  EnrolmentInvite,
  ListEnrolmentsResponse,
  ListUserCredentialsResponse,
  ListUsersResponse,
  OauthProvider,
  OauthProviderConfig,
  StartOauthOptions,
  UpdateUserProfileRequest,
  User,
  UserCredential,
  UserRole,
} from "../types/user.js";

/**
 * The household user directory, enrolment, and federated sign-in (ADR-0031).
 *
 * Everything here is admin-only except {@link changeOwnPassword},
 * {@link availableMethods}, {@link startOauth} and {@link redeemEnrolment}.
 * The daemon enforces that; this class does not pre-check, because a
 * client-side guess about authorization is one that goes stale silently.
 */
export class UserService {
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** Every household user, for the admin directory (admin only). */
  async list(): Promise<User[]> {
    const res: ListUsersResponse = await this.api.get("/users");
    return res.users;
  }

  /** One household user. Readable by an admin, or by that user about themselves. */
  async getById(id: string): Promise<User> {
    return this.api.get("/users/{id}", { path: { id } });
  }

  /**
   * Create a household user with **no credential** (admin only).
   *
   * The account cannot be used until it is enrolled — call
   * {@link issueEnrolment} next and hand the token over out of band.
   */
  async create(body: CreateUserRequest): Promise<User> {
    return this.api.post("/users", {
      body: {
        display_name: body.display_name,
        email: body.email ?? null,
        role: body.role,
      },
    });
  }

  /**
   * Update a user's display name and email.
   *
   * A user may edit their own profile; changing anybody else's is an admin
   * action. Both fields are replacements — omitting `email` clears it.
   */
  async updateProfile(id: string, body: UpdateUserProfileRequest): Promise<User> {
    return this.api.patch("/users/{id}", {
      path: { id },
      body: {
        display_name: body.display_name,
        email: body.email ?? null,
      },
    });
  }

  /**
   * Enable or disable a user (admin only).
   *
   * Disabling revokes immediately: every live session is deleted. Refuses to
   * disable the last enabled admin.
   */
  async setEnabled(id: string, enabled: boolean): Promise<User> {
    return this.api.put("/users/{id}/enabled", { path: { id }, body: { enabled } });
  }

  /** Change a user's role (admin only). Refuses to demote the last enabled admin. */
  async setRole(id: string, role: UserRole): Promise<User> {
    return this.api.put("/users/{id}/role", { path: { id }, body: { role } });
  }

  /**
   * Delete a user (admin only).
   *
   * Credentials, enrolments and sessions cascade; owned devices are simply
   * unassigned — deleting a person must not delete the household's hardware.
   * Refuses to delete the last enabled admin.
   */
  async delete(id: string): Promise<void> {
    await this.api.del("/users/{id}", { path: { id } });
  }

  /** List a user's credentials, without secrets (admin only). */
  async listCredentials(id: string): Promise<UserCredential[]> {
    const res: ListUserCredentialsResponse = await this.api.get("/users/{id}/credentials", {
      path: { id },
    });
    return res.credentials;
  }

  /**
   * Unlink every credential of one federated provider from a user (admin only).
   *
   * Never touches the local password: an account whose only credential
   * depended on a reachable provider would be unreachable during a WAN outage.
   */
  async unlinkOauth(id: string, provider: OauthProvider): Promise<void> {
    await this.api.del("/users/{id}/credentials/{provider}", { path: { id, provider } });
  }

  /**
   * Set your own password, proving the current one first.
   *
   * **Every** session for the account is invalidated, this one included — so
   * the next call will be unauthorized and the person must sign in again.
   * Not an admin route in either direction: an admin cannot set someone
   * else's password (they would then know it).
   */
  async changeOwnPassword(currentPassword: string, newPassword: string): Promise<void> {
    await this.api.post("/users/me/password", {
      body: { current_password: currentPassword, new_password: newPassword },
    });
  }

  /**
   * Issue a one-time enrolment invitation (admin only).
   *
   * The returned `token` is shown **exactly once** — only its hash is stored,
   * so an admin who loses it must issue another.
   */
  async issueEnrolment(id: string): Promise<EnrolmentInvite> {
    return this.api.post("/users/{id}/enrolments", { path: { id } });
  }

  /** Outstanding and spent invitations for a user (admin only). Tokens are never included. */
  async listEnrolments(id: string): Promise<Enrolment[]> {
    const res: ListEnrolmentsResponse = await this.api.get("/users/{id}/enrolments", {
      path: { id },
    });
    return res.enrolments;
  }

  /** Revoke an unredeemed invitation (admin only). */
  async revokeEnrolment(id: string, enrolmentId: string): Promise<void> {
    await this.api.del("/users/{id}/enrolments/{enrolment_id}", {
      path: { id, enrolment_id: enrolmentId },
    });
  }

  /**
   * Which sign-in methods this box can offer right now (unauthenticated).
   *
   * Read this before rendering a sign-in surface: a federated button for an
   * unconfigured provider is a button that fails at the provider.
   */
  async availableMethods(): Promise<AuthMethods> {
    return this.api.get("/auth/methods");
  }

  /**
   * Begin an OAuth ceremony and return the provider URL to navigate to.
   *
   * Returns the URL rather than navigating, so the caller controls the
   * navigation and can surface an error. A ceremony started while signed in is
   * a **link**; one started anonymously is a **sign-in** — the daemon records
   * which on the ceremony and never re-guesses it at the callback.
   */
  async startOauth(provider: OauthProvider, options: StartOauthOptions = {}): Promise<string> {
    const res = await this.api.get("/auth/oauth/{provider}/start", {
      path: { provider },
      query: {
        return_to: options.returnTo ?? "admin",
        remember_me: options.rememberMe ?? false,
      },
    });
    return res.url;
  }

  /**
   * Redeem a one-time enrolment invitation, setting the member's first
   * password (unauthenticated).
   *
   * The person redeeming has no credential yet and therefore cannot have a
   * session — the token *is* the authorization.
   */
  async redeemEnrolment(token: string, password: string): Promise<User> {
    return this.api.post("/auth/enrolments/redeem", { body: { token, password } });
  }

  /**
   * Admin: every provider's full configuration, including client IDs and the
   * **stored** on/off flags.
   *
   * Use this — not {@link availableMethods} — to drive a settings screen.
   * `availableMethods` is unauthenticated, so it deliberately omits the client
   * ID and reports effective availability rather than the stored flag.
   */
  async listOauthProviders(): Promise<OauthProviderConfig[]> {
    const res = await this.api.get("/auth/providers");
    return res.providers;
  }

  /**
   * Configure a federated provider's client id and secret (admin only).
   *
   * Omitting `client_secret` leaves an existing one in place, so a form that
   * cannot read the secret back can still submit an `enabled` toggle.
   */
  async configureOauthProvider(
    provider: OauthProvider,
    body: ConfigureOauthProviderRequest,
  ): Promise<OauthProviderConfig> {
    return this.api.put("/auth/providers/{provider}", {
      path: { provider },
      body: {
        client_id: body.client_id,
        client_secret: body.client_secret ?? null,
        enabled: body.enabled,
      },
    });
  }

  /**
   * Forget a provider's configuration entirely, including its secret (admin only).
   *
   * Existing links are left alone: this removes the box's ability to run the
   * ceremony, not the record of who linked what.
   */
  async clearOauthProvider(provider: OauthProvider): Promise<void> {
    await this.api.del("/auth/providers/{provider}", { path: { provider } });
  }
}
