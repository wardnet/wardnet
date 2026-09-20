// Household identity types (ADR-0031).
//
// Wire-shaped (snake_case), like the rest of `types/api.ts`: the internal
// client normalizes the generated schema and then checks these against it, so
// a renamed or retyped daemon field stops `services/users.ts` compiling rather
// than silently returning `undefined` at runtime.

/** A user's role. An `admin` is exactly equal to the legacy local admin. */
export type UserRole = "admin" | "member";

/** What sort of credential a `UserCredential` is. */
export type CredentialKind = "password" | "google" | "github" | "passkey";

/** A federated identity provider Wardnet can run a ceremony against. */
export type OauthProvider = "google" | "github";

/**
 * A household user as the admin directory shows them.
 *
 * Carries no credential material of any kind — not even a derived "has a
 * password" flag. Credentials are a separate resource.
 */
export interface User {
  id: string;
  display_name: string;
  /** `null` for a user created by the setup wizard, which asks for no email. */
  email: string | null;
  role: UserRole;
  /**
   * A disabled user keeps their row and their device ownership but cannot sign
   * in; every live session was deleted when they were disabled.
   */
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

/** Response body for GET /api/users. */
export interface ListUsersResponse {
  users: User[];
}

/**
 * Request body for POST /api/users.
 *
 * Creates an account with **no credential**. Issue an enrolment invitation
 * next; the admin never learns the member's password.
 */
export interface CreateUserRequest {
  display_name: string;
  email?: string | null;
  role: UserRole;
}

/** Request body for PATCH /api/users/{id}. Omitting `email` clears it. */
export interface UpdateUserProfileRequest {
  display_name: string;
  email?: string | null;
}

/** One of a user's credentials, without any secret. */
export interface UserCredential {
  id: string;
  kind: CredentialKind;
  /** The login identifier. For OAuth, the provider's own subject. */
  subject: string;
  label: string | null;
  created_at: string;
  last_used_at: string | null;
}

/** Response body for GET /api/users/{id}/credentials. */
export interface ListUserCredentialsResponse {
  credentials: UserCredential[];
}

/**
 * Response body for POST /api/users/{id}/enrolments.
 *
 * `token` appears **exactly once**, here. Only its hash is stored, so an admin
 * who loses it must issue another.
 */
export interface EnrolmentInvite {
  token: string;
  expires_at: string;
  user_id: string;
}

/** An outstanding or spent enrolment invitation. */
export interface Enrolment {
  id: string;
  user_id: string;
  created_at: string;
  expires_at: string;
  /** `null` while outstanding. An invitation is single-use. */
  used_at: string | null;
}

/** Response body for GET /api/users/{id}/enrolments. */
export interface ListEnrolmentsResponse {
  enrolments: Enrolment[];
}

/**
 * One federated provider's availability, from the unauthenticated
 * GET /api/auth/methods.
 *
 * Carries neither the client ID nor the redirect URI — both are admin-only and
 * live on {@link OauthProviderConfig}. A sign-in surface needs neither: it
 * renders a button from `enabled` alone.
 */
export interface AuthProviderStatus {
  provider: string;
  /**
   * The admin turned it on **and** it is fully configured — the *effective*
   * availability. Render sign-in buttons from this.
   */
  enabled: boolean;
  /** A client secret is present. The secret itself is never returned. */
  configured: boolean;
}

/**
 * Response body for GET /api/auth/methods.
 *
 * The contract a sign-in surface reads to decide which buttons to render.
 * `password` is always true — the local password is the floor.
 */
export interface AuthMethods {
  password: boolean;
  providers: AuthProviderStatus[];
}

/**
 * One provider's full configuration, from the **admin** `GET /api/auth/providers`.
 *
 * Separate from {@link AuthProviderStatus}, which is served unauthenticated
 * and therefore carries no client ID — that names the household's own OAuth
 * application and is admin-only.
 */
export interface OauthProviderConfig {
  provider: string;
  /** The configured client ID, or `null` if none is set. */
  client_id: string | null;
  /**
   * The admin's **stored** on/off flag — not the effective availability.
   * This is the value a settings form must round-trip; submitting the
   * effective one would switch a provider off whenever its secret or hostname
   * happened to be missing at read time.
   */
  enabled: boolean;
  /** A client secret is present. The secret itself is never returned. */
  configured: boolean;
  /** The redirect URI to register with the provider by hand. */
  redirect_uri: string | null;
}

/** Which admin surface an OAuth ceremony began on, and returns to. */
export type OauthReturnTo = "admin" | "admin_app";

/** Options for starting an OAuth ceremony. */
export interface StartOauthOptions {
  /** Defaults to `admin`. */
  returnTo?: OauthReturnTo;
  /**
   * Whether to issue a long-lived, refreshable session. Defaults to `false`.
   *
   * Decided here and nowhere else: it gates session refresh, so there is
   * deliberately no endpoint that raises it after the fact.
   */
  rememberMe?: boolean;
}

/** Request body for PUT /api/auth/providers/{provider}. */
export interface ConfigureOauthProviderRequest {
  client_id: string;
  /**
   * Omit to leave any existing secret in place — which is what lets a form
   * that cannot read the secret back still submit an `enabled` toggle.
   */
  client_secret?: string | null;
  enabled: boolean;
}
