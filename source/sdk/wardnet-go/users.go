package wardnet

import (
	"context"
	"time"

	"wardnet.network/go/internal/rest"
)

// UsersService manages the household user directory, enrolment invitations,
// and federated sign-in configuration (ADR-0031).
//
// Most methods are admin-only. ChangeOwnPassword, AvailableMethods, StartOAuth
// and RedeemEnrolment are not; the daemon enforces the distinction, and this
// package deliberately does not pre-check it, because a client-side guess
// about authorization is one that goes stale silently.
type UsersService struct{ c *Client }

// UserRole is a household user's role: "admin" or "member". An admin is
// exactly equal to the legacy local admin — there is no second tier.
type UserRole string

const (
	// RoleAdmin can administer the box.
	RoleAdmin UserRole = "admin"
	// RoleMember can use it, and manage only their own profile.
	RoleMember UserRole = "member"
)

// CredentialKind is what sort of credential a [UserCredential] is.
type CredentialKind string

const (
	// CredentialPassword is an Argon2id local password. The floor: it needs no
	// WAN, no certificate and no provider, and is never removable.
	CredentialPassword CredentialKind = "password"
	CredentialGoogle   CredentialKind = "google"
	CredentialGitHub   CredentialKind = "github"
	// CredentialPasskey is reserved. The column accepts it; no code path
	// produces one yet (issue #1194).
	CredentialPasskey CredentialKind = "passkey"
)

// OAuthProvider is a federated identity provider Wardnet can run a ceremony
// against.
type OAuthProvider string

const (
	ProviderGoogle OAuthProvider = "google"
	ProviderGitHub OAuthProvider = "github"
)

// User is a household user as the admin directory shows them.
//
// Carries no credential material of any kind — not even a derived "has a
// password" flag. Credentials are a separate call, [UsersService.Credentials].
type User struct {
	// ID is the user's UUID.
	ID string `json:"id"`
	// DisplayName is the name shown in the UI.
	DisplayName string `json:"display_name"`
	// Email is the user's address, or "" if they have none. A user created by
	// the setup wizard has none, because it does not ask.
	Email string `json:"email"`
	// Role is "admin" or "member".
	Role UserRole `json:"role"`
	// Enabled reports whether the user may sign in. A disabled user keeps
	// their row and their device ownership; every live session was deleted
	// when they were disabled.
	Enabled bool `json:"enabled"`
	// CreatedAt / UpdatedAt are RFC 3339 timestamps.
	CreatedAt string `json:"created_at"`
	UpdatedAt string `json:"updated_at"`
}

// UserCredential is one of a user's credentials, without any secret.
type UserCredential struct {
	// ID is the credential's UUID.
	ID string `json:"id"`
	// Kind is what sort of credential this is.
	Kind CredentialKind `json:"kind"`
	// Subject is the login identifier. Safe to display: for OAuth it is the
	// provider's own subject.
	Subject string `json:"subject"`
	// Label is a human label, or "" if none.
	Label string `json:"label"`
	// CreatedAt is an RFC 3339 timestamp.
	CreatedAt string `json:"created_at"`
	// LastUsedAt is the last successful authentication, or "" if never used.
	LastUsedAt string `json:"last_used_at"`
}

// EnrolmentInvite is a freshly issued one-time invitation.
//
// Token is present exactly once, in this value. Only its hash is persisted, so
// an admin who loses it must issue another — the intended property, not a
// limitation.
type EnrolmentInvite struct {
	// Token is the credential to hand over, out of band.
	Token string `json:"token"`
	// ExpiresAt is an RFC 3339 expiry.
	ExpiresAt string `json:"expires_at"`
	// UserID is the user being enrolled.
	UserID string `json:"user_id"`
}

// Enrolment is an outstanding or spent invitation.
type Enrolment struct {
	// ID is the enrolment's UUID.
	ID string `json:"id"`
	// UserID is the user being enrolled.
	UserID string `json:"user_id"`
	// CreatedAt / ExpiresAt are RFC 3339 timestamps.
	CreatedAt string `json:"created_at"`
	ExpiresAt string `json:"expires_at"`
	// UsedAt is "" while outstanding. An invitation is single-use, so a
	// non-empty value means it can never be redeemed again.
	UsedAt string `json:"used_at"`
}

// AuthProviderStatus is one federated provider's availability, as reported by
// the unauthenticated AvailableMethods. It deliberately carries neither the
// client ID nor the redirect URI — between them they name the household's own
// OAuth application and its public hostname, and both are admin-only; see
// [OAuthProviderConfig].
type AuthProviderStatus struct {
	// Provider is "google" or "github".
	Provider OAuthProvider `json:"provider"`
	// Enabled reports that the admin turned it on and it is fully configured —
	// the effective availability. Offer sign-in from this.
	Enabled bool `json:"enabled"`
	// Configured reports that a client secret is present. The secret itself is
	// never returned by any endpoint.
	Configured bool `json:"configured"`
}

// OAuthProviderConfig is one provider's full configuration, from the admin
// ListOAuthProviders.
type OAuthProviderConfig struct {
	// Provider is "google" or "github".
	Provider OAuthProvider `json:"provider"`
	// ClientID is the configured client ID, or "" if none is set.
	ClientID string `json:"client_id"`
	// Enabled is the admin's stored on/off flag, not the effective
	// availability — this is the value a settings form must round-trip.
	Enabled bool `json:"enabled"`
	// Configured reports that a client secret is stored. The secret itself is
	// never returned.
	Configured bool `json:"configured"`
	// RedirectURI is the URI to register with the provider, or "".
	RedirectURI string `json:"redirect_uri"`
}

// AuthMethods reports which sign-in methods the box can offer right now.
type AuthMethods struct {
	// Password is always true. The local password is the floor.
	Password bool `json:"password"`
	// Providers lists each federated provider's availability.
	Providers []AuthProviderStatus `json:"providers"`
}

// OAuthReturnTo names the admin surface a ceremony began on, and returns to.
type OAuthReturnTo string

const (
	// ReturnToAdmin is the desktop admin site. The default.
	ReturnToAdmin OAuthReturnTo = "admin"
	// ReturnToAdminApp is the admin mobile PWA.
	ReturnToAdminApp OAuthReturnTo = "admin_app"
)

func userFromREST(u *rest.UserResponse) User {
	return User{
		ID:          u.Id.String(),
		DisplayName: u.DisplayName,
		Email:       deref(u.Email),
		Role:        UserRole(u.Role),
		Enabled:     u.Enabled,
		CreatedAt:   u.CreatedAt,
		UpdatedAt:   u.UpdatedAt,
	}
}

// List returns every household user (admin only).
func (s *UsersService) List(ctx context.Context) ([]User, error) {
	resp, err := s.c.rest.ListUsersWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]User, 0, len(resp.JSON200.Users))
	for i := range resp.JSON200.Users {
		out = append(out, userFromREST(&resp.JSON200.Users[i]))
	}
	return out, nil
}

// Get returns one household user. Readable by an admin, or by that user about
// themselves.
func (s *UsersService) Get(ctx context.Context, id string) (*User, error) {
	uid, err := parseUUID(id, "user")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.GetUserWithResponse(ctx, uid)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	u := userFromREST(resp.JSON200)
	return &u, nil
}

// Create adds a household user with no credential (admin only).
//
// The account cannot be used until it is enrolled: the admin never learns the
// member's password, so account creation and credential creation are
// deliberately separate steps. Call [UsersService.IssueEnrolment] next.
func (s *UsersService) Create(ctx context.Context, displayName, email string, role UserRole) (*User, error) {
	body := rest.CreateUserJSONRequestBody{
		DisplayName: displayName,
		Role:        rest.UserRole(role),
	}
	if email != "" {
		body.Email = &email
	}
	resp, err := s.c.rest.CreateUserWithResponse(ctx, body)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	u := userFromREST(resp.JSON200)
	return &u, nil
}

// UpdateProfile replaces a user's display name and email.
//
// A user may edit their own profile; changing anybody else's is an admin
// action. Both fields are replacements — passing an empty email clears it.
func (s *UsersService) UpdateProfile(ctx context.Context, id, displayName, email string) (*User, error) {
	uid, err := parseUUID(id, "user")
	if err != nil {
		return nil, err
	}
	body := rest.UpdateProfileJSONRequestBody{DisplayName: displayName}
	if email != "" {
		body.Email = &email
	}
	resp, err := s.c.rest.UpdateProfileWithResponse(ctx, uid, body)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	u := userFromREST(resp.JSON200)
	return &u, nil
}

// SetEnabled enables or disables a user (admin only).
//
// Disabling revokes immediately: every live session is deleted. Refuses to
// disable the last enabled admin.
func (s *UsersService) SetEnabled(ctx context.Context, id string, enabled bool) (*User, error) {
	uid, err := parseUUID(id, "user")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.SetUserEnabledWithResponse(ctx, uid, rest.SetUserEnabledJSONRequestBody{Enabled: enabled})
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	u := userFromREST(resp.JSON200)
	return &u, nil
}

// SetRole changes a user's role (admin only). Refuses to demote the last
// enabled admin.
func (s *UsersService) SetRole(ctx context.Context, id string, role UserRole) (*User, error) {
	uid, err := parseUUID(id, "user")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.SetRoleWithResponse(ctx, uid, rest.SetRoleJSONRequestBody{Role: rest.UserRole(role)})
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	u := userFromREST(resp.JSON200)
	return &u, nil
}

// Delete removes a household user (admin only).
//
// Credentials, enrolments and sessions cascade; owned devices are unassigned
// rather than deleted — removing a person must not remove the household's
// hardware. Refuses to delete the last enabled admin.
func (s *UsersService) Delete(ctx context.Context, id string) error {
	uid, err := parseUUID(id, "user")
	if err != nil {
		return err
	}
	resp, err := s.c.rest.DeleteUserWithResponse(ctx, uid)
	if err != nil {
		return err
	}
	if resp.StatusCode() != 204 {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

// Credentials lists a user's credentials, without secrets (admin only).
func (s *UsersService) Credentials(ctx context.Context, id string) ([]UserCredential, error) {
	uid, err := parseUUID(id, "user")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.ListCredentialsWithResponse(ctx, uid)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]UserCredential, 0, len(resp.JSON200.Credentials))
	for i := range resp.JSON200.Credentials {
		c := &resp.JSON200.Credentials[i]
		out = append(out, UserCredential{
			ID:         c.Id,
			Kind:       CredentialKind(c.Kind),
			Subject:    c.Subject,
			Label:      deref(c.Label),
			CreatedAt:  c.CreatedAt,
			LastUsedAt: deref(c.LastUsedAt),
		})
	}
	return out, nil
}

// UnlinkOAuth removes every credential of one federated provider from a user
// (admin only).
//
// Never touches the local password: an account whose only credential depended
// on a reachable provider would be unreachable during a WAN outage.
func (s *UsersService) UnlinkOAuth(ctx context.Context, id string, provider OAuthProvider) error {
	uid, err := parseUUID(id, "user")
	if err != nil {
		return err
	}
	resp, err := s.c.rest.UnlinkOauthWithResponse(ctx, uid, string(provider))
	if err != nil {
		return err
	}
	if resp.StatusCode() != 204 {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

// ChangeOwnPassword sets the calling user's password, proving the current one
// first.
//
// Every session for the account is invalidated, including the one that made
// this call — the caller's token stops working immediately. Not an admin call
// in either direction: an admin cannot set someone else's password, because
// they would then know it.
func (s *UsersService) ChangeOwnPassword(ctx context.Context, currentPassword, newPassword string) error {
	resp, err := s.c.rest.ChangeOwnPasswordWithResponse(ctx, rest.ChangeOwnPasswordJSONRequestBody{
		CurrentPassword: currentPassword,
		NewPassword:     newPassword,
	})
	if err != nil {
		return err
	}
	if resp.StatusCode() != 204 {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

// IssueEnrolment creates a one-time invitation for a user with no password yet
// (admin only).
//
// The returned Token is available exactly once. Only its hash is stored, so an
// admin who loses it must issue another.
func (s *UsersService) IssueEnrolment(ctx context.Context, id string) (*EnrolmentInvite, error) {
	uid, err := parseUUID(id, "user")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.IssueEnrolmentWithResponse(ctx, uid)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return &EnrolmentInvite{
		Token:     resp.JSON200.Token,
		ExpiresAt: resp.JSON200.ExpiresAt,
		UserID:    resp.JSON200.UserId.String(),
	}, nil
}

// Enrolments lists a user's outstanding and spent invitations (admin only).
// Tokens are never included.
func (s *UsersService) Enrolments(ctx context.Context, id string) ([]Enrolment, error) {
	uid, err := parseUUID(id, "user")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.ListEnrolmentsWithResponse(ctx, uid)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]Enrolment, 0, len(resp.JSON200.Enrolments))
	for i := range resp.JSON200.Enrolments {
		e := &resp.JSON200.Enrolments[i]
		out = append(out, Enrolment{
			ID:        e.Id.String(),
			UserID:    e.UserId.String(),
			CreatedAt: e.CreatedAt,
			ExpiresAt: e.ExpiresAt,
			UsedAt:    deref(e.UsedAt),
		})
	}
	return out, nil
}

// RevokeEnrolment cancels an unredeemed invitation (admin only).
func (s *UsersService) RevokeEnrolment(ctx context.Context, userID, enrolmentID string) error {
	uid, err := parseUUID(userID, "user")
	if err != nil {
		return err
	}
	eid, err := parseUUID(enrolmentID, "enrolment")
	if err != nil {
		return err
	}
	resp, err := s.c.rest.RevokeEnrolmentWithResponse(ctx, uid, eid)
	if err != nil {
		return err
	}
	if resp.StatusCode() != 204 {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

// RedeemEnrolment spends a one-time invitation, setting the member's first
// password. Unauthenticated: the person redeeming has no credential yet and
// therefore cannot have a session — the token is the authorization.
func (s *UsersService) RedeemEnrolment(ctx context.Context, token, password string) (*User, error) {
	resp, err := s.c.rest.RedeemEnrolmentWithResponse(ctx, rest.RedeemEnrolmentJSONRequestBody{
		Token:    token,
		Password: password,
	})
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	u := userFromREST(resp.JSON200)
	return &u, nil
}

// AvailableMethods reports which sign-in methods the box can offer right now.
// Unauthenticated — it backs the sign-in surface, whose job is to be reachable
// before anybody has a session.
func (s *UsersService) AvailableMethods(ctx context.Context) (*AuthMethods, error) {
	resp, err := s.c.rest.AvailableMethodsWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := &AuthMethods{Password: resp.JSON200.Password}
	for i := range resp.JSON200.Providers {
		p := &resp.JSON200.Providers[i]
		out.Providers = append(out.Providers, AuthProviderStatus{
			Provider:   OAuthProvider(p.Provider),
			Enabled:    p.Enabled,
			Configured: p.Configured,
		})
	}
	return out, nil
}

// StartOAuth begins an OAuth ceremony and returns the provider URL to send a
// browser to.
//
// A ceremony started by a signed-in caller is a link; one started anonymously
// is a sign-in. Which it is, is recorded on the ceremony here and never
// re-guessed at the callback — the callback is a bare provider redirect that
// carries nothing saying which ceremony it belongs to.
//
// rememberMe must be decided here: it gates session refresh, so there is
// deliberately no call that raises it after the fact.
func (s *UsersService) StartOAuth(ctx context.Context, provider OAuthProvider, returnTo OAuthReturnTo, rememberMe bool) (string, error) {
	if returnTo == "" {
		returnTo = ReturnToAdmin
	}
	rt := string(returnTo)
	params := &rest.StartOauthParams{ReturnTo: &rt, RememberMe: &rememberMe}
	resp, err := s.c.rest.StartOauthWithResponse(ctx, string(provider), params)
	if err != nil {
		return "", err
	}
	if resp.JSON200 == nil {
		return "", apiError(resp.HTTPResponse, resp.Body)
	}
	return resp.JSON200.Url, nil
}

// ConfigureOAuthProvider sets a provider's client id and secret, and whether
// it is offered (admin only).
//
// Passing an empty clientSecret leaves any existing secret in place, so an
// admin can toggle enabled or fix a typo'd client id without re-pasting it.
// ListOAuthProviders returns every provider's full configuration, including
// client IDs and the stored on/off flags (admin only).
//
// Use this rather than AvailableMethods to drive a settings screen:
// AvailableMethods is unauthenticated, so it omits the client ID and reports
// effective availability instead of the stored flag.
func (s *UsersService) ListOAuthProviders(ctx context.Context) ([]OAuthProviderConfig, error) {
	resp, err := s.c.rest.ListOauthProvidersWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]OAuthProviderConfig, 0, len(resp.JSON200.Providers))
	for i := range resp.JSON200.Providers {
		p := &resp.JSON200.Providers[i]
		out = append(out, OAuthProviderConfig{
			Provider:    OAuthProvider(p.Provider),
			ClientID:    deref(p.ClientId),
			Enabled:     p.Enabled,
			Configured:  p.Configured,
			RedirectURI: deref(p.RedirectUri),
		})
	}
	return out, nil
}

func (s *UsersService) ConfigureOAuthProvider(ctx context.Context, provider OAuthProvider, clientID, clientSecret string, enabled bool) (*OAuthProviderConfig, error) {
	body := rest.ConfigureOauthProviderJSONRequestBody{
		ClientId: clientID,
		Enabled:  enabled,
	}
	if clientSecret != "" {
		body.ClientSecret = &clientSecret
	}
	resp, err := s.c.rest.ConfigureOauthProviderWithResponse(ctx, string(provider), body)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return &OAuthProviderConfig{
		Provider:    OAuthProvider(resp.JSON200.Provider),
		ClientID:    deref(resp.JSON200.ClientId),
		Enabled:     resp.JSON200.Enabled,
		Configured:  resp.JSON200.Configured,
		RedirectURI: deref(resp.JSON200.RedirectUri),
	}, nil
}

// ClearOAuthProvider forgets a provider's configuration entirely, including
// its secret (admin only).
//
// Existing links are left alone: this removes the box's ability to run the
// ceremony, not the record of who linked what.
func (s *UsersService) ClearOAuthProvider(ctx context.Context, provider OAuthProvider) error {
	resp, err := s.c.rest.ClearOauthProviderWithResponse(ctx, string(provider))
	if err != nil {
		return err
	}
	if resp.StatusCode() != 204 {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

// enrolmentTTL is how long an admin-issued invitation stays redeemable. Long
// enough to hand a phone to a family member over a weekend, short enough that
// a forgotten invitation is not a standing way in. Mirrors the daemon's own
// constant; exposed so callers can render "expires in ..." without parsing.
const enrolmentTTL = 72 * time.Hour

// EnrolmentTTL returns how long a freshly issued invitation stays redeemable.
func EnrolmentTTL() time.Duration { return enrolmentTTL }
