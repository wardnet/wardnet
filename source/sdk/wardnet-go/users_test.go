package wardnet_test

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"testing"

	wardnet "wardnet.network/go"
)

const (
	testUserID     = "6e05df45-1fa4-4327-8c1e-218c79b253ba"
	testDeviceID   = "3f8a5a6b-2c1d-4f5e-8a9b-0c1d2e3f4a5b"
	testEnrolID    = "9a1b2c3d-4e5f-4a6b-8c9d-0e1f2a3b4c5d"
	testUserObject = `{"id":"6e05df45-1fa4-4327-8c1e-218c79b253ba","display_name":"Ana",
		"email":"ana@example.invalid","role":"admin","enabled":true,
		"created_at":"2026-08-01T00:00:00Z","updated_at":"2026-08-02T00:00:00Z"}`
)

func TestUsersList(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/users" {
			t.Errorf("path = %q", r.URL.Path)
		}
		writeJSON(w, `{"users":[`+testUserObject+`,
			{"id":"11111111-2222-4333-8444-555555555555","display_name":"Cleo",
			 "email":null,"role":"member","enabled":false,
			 "created_at":"2026-08-01T00:00:00Z","updated_at":"2026-08-01T00:00:00Z"}]}`)
	}))

	users, err := c.Users.List(context.Background())
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(users) != 2 {
		t.Fatalf("users = %d, want 2", len(users))
	}
	if users[0].DisplayName != "Ana" || users[0].Role != wardnet.RoleAdmin {
		t.Errorf("unexpected first user: %+v", users[0])
	}
	// A null email is "" on the way out, not a dangling pointer.
	if users[1].Email != "" {
		t.Errorf("email = %q, want empty for a null", users[1].Email)
	}
	if users[1].Enabled {
		t.Error("Cleo should be disabled")
	}
}

func TestUsersGet(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/users/"+testUserID {
			t.Errorf("path = %q", r.URL.Path)
		}
		writeJSON(w, testUserObject)
	}))

	u, err := c.Users.Get(context.Background(), testUserID)
	if err != nil {
		t.Fatalf("Get: %v", err)
	}
	if u.ID != testUserID || u.Email != "ana@example.invalid" {
		t.Errorf("unexpected user: %+v", u)
	}
}

func TestUsersGetRejectsAMalformedID(t *testing.T) {
	// Caught before any request goes out — a bad id is the caller's mistake,
	// not something the daemon should be asked about.
	c := newClient(t, http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		t.Error("no request should have been made")
	}))

	if _, err := c.Users.Get(context.Background(), "not-a-uuid"); err == nil {
		t.Fatal("expected an error for a malformed id")
	}
}

func TestUsersCreate(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode: %v", err)
		}
		if body["display_name"] != "Ana" || body["role"] != "admin" {
			t.Errorf("unexpected body: %+v", body)
		}
		if body["email"] != "ana@example.invalid" {
			t.Errorf("email = %v", body["email"])
		}
		writeJSON(w, testUserObject)
	}))

	u, err := c.Users.Create(context.Background(), "Ana", "ana@example.invalid", wardnet.RoleAdmin)
	if err != nil {
		t.Fatalf("Create: %v", err)
	}
	if u.DisplayName != "Ana" {
		t.Errorf("display name = %q", u.DisplayName)
	}
}

func TestUsersCreateOmitsAnEmptyEmail(t *testing.T) {
	// The column is uniquely indexed, so an empty string would collide on the
	// second user — "no email" has to travel as absent, not as "".
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		if strings.Contains(string(raw), `"email"`) {
			t.Errorf("body should omit email entirely, got %s", raw)
		}
		writeJSON(w, testUserObject)
	}))

	if _, err := c.Users.Create(context.Background(), "Ana", "", wardnet.RoleMember); err != nil {
		t.Fatalf("Create: %v", err)
	}
}

func TestUsersUpdateProfile(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPatch {
			t.Errorf("method = %q, want PATCH", r.Method)
		}
		writeJSON(w, testUserObject)
	}))

	if _, err := c.Users.UpdateProfile(context.Background(), testUserID, "Ana", "ana@example.invalid"); err != nil {
		t.Fatalf("UpdateProfile: %v", err)
	}
}

func TestUsersSetEnabledAndRole(t *testing.T) {
	var seen []string
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		seen = append(seen, r.Method+" "+r.URL.Path)
		writeJSON(w, testUserObject)
	}))

	if _, err := c.Users.SetEnabled(context.Background(), testUserID, false); err != nil {
		t.Fatalf("SetEnabled: %v", err)
	}
	if _, err := c.Users.SetRole(context.Background(), testUserID, wardnet.RoleMember); err != nil {
		t.Fatalf("SetRole: %v", err)
	}

	want := []string{
		"PUT /api/users/" + testUserID + "/enabled",
		"PUT /api/users/" + testUserID + "/role",
	}
	for i, w := range want {
		if seen[i] != w {
			t.Errorf("call %d = %q, want %q", i, seen[i], w)
		}
	}
}

func TestUsersDelete(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodDelete {
			t.Errorf("method = %q", r.Method)
		}
		w.WriteHeader(http.StatusNoContent)
	}))

	if err := c.Users.Delete(context.Background(), testUserID); err != nil {
		t.Fatalf("Delete: %v", err)
	}
}

func TestUsersDeleteSurfacesARefusal(t *testing.T) {
	// The daemon refuses to delete the last enabled admin. That has to reach
	// the caller as an error, not be swallowed by a 204-shaped happy path.
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusConflict)
		_, _ = io.WriteString(w, `{"error":"conflict","detail":"cannot remove the last admin"}`)
	}))

	if err := c.Users.Delete(context.Background(), testUserID); err == nil {
		t.Fatal("expected an error for a refused delete")
	}
}

func TestUsersCredentials(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/users/"+testUserID+"/credentials" {
			t.Errorf("path = %q", r.URL.Path)
		}
		writeJSON(w, `{"credentials":[
			{"id":"cred-1","kind":"password","subject":"ana","label":null,
			 "created_at":"2026-08-01T00:00:00Z","last_used_at":null},
			{"id":"cred-2","kind":"github","subject":"12345","label":"ana-on-github",
			 "created_at":"2026-08-01T00:00:00Z","last_used_at":"2026-08-03T00:00:00Z"}]}`)
	}))

	creds, err := c.Users.Credentials(context.Background(), testUserID)
	if err != nil {
		t.Fatalf("Credentials: %v", err)
	}
	if len(creds) != 2 {
		t.Fatalf("credentials = %d, want 2", len(creds))
	}
	if creds[0].Kind != wardnet.CredentialPassword || creds[1].Kind != wardnet.CredentialGitHub {
		t.Errorf("kinds = %q, %q", creds[0].Kind, creds[1].Kind)
	}
	if creds[0].LastUsedAt != "" {
		t.Errorf("last used = %q, want empty for a never-used credential", creds[0].LastUsedAt)
	}
	if creds[1].Label != "ana-on-github" {
		t.Errorf("label = %q", creds[1].Label)
	}
}

func TestUsersUnlinkOAuth(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/users/"+testUserID+"/credentials/google" {
			t.Errorf("path = %q", r.URL.Path)
		}
		w.WriteHeader(http.StatusNoContent)
	}))

	if err := c.Users.UnlinkOAuth(context.Background(), testUserID, wardnet.ProviderGoogle); err != nil {
		t.Fatalf("UnlinkOAuth: %v", err)
	}
}

func TestUsersChangeOwnPassword(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/users/me/password" {
			t.Errorf("path = %q", r.URL.Path)
		}
		var body map[string]string
		_ = json.NewDecoder(r.Body).Decode(&body)
		if body["current_password"] != "old" || body["new_password"] != "new" {
			t.Errorf("unexpected body: %+v", body)
		}
		w.WriteHeader(http.StatusNoContent)
	}))

	if err := c.Users.ChangeOwnPassword(context.Background(), "old", "new"); err != nil {
		t.Fatalf("ChangeOwnPassword: %v", err)
	}
}

func TestUsersIssueEnrolment(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Errorf("method = %q", r.Method)
		}
		writeJSON(w, `{"token":"one-time-token","expires_at":"2026-08-04T00:00:00Z",
			"user_id":"`+testUserID+`"}`)
	}))

	invite, err := c.Users.IssueEnrolment(context.Background(), testUserID)
	if err != nil {
		t.Fatalf("IssueEnrolment: %v", err)
	}
	// Present exactly once, in this response — there is no second chance to
	// fetch it, so the SDK must surface it rather than drop it.
	if invite.Token != "one-time-token" {
		t.Errorf("token = %q", invite.Token)
	}
	if invite.UserID != testUserID {
		t.Errorf("user id = %q", invite.UserID)
	}
}

func TestUsersEnrolments(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, `{"enrolments":[
			{"id":"`+testEnrolID+`","user_id":"`+testUserID+`",
			 "created_at":"2026-08-01T00:00:00Z","expires_at":"2026-08-04T00:00:00Z",
			 "used_at":null},
			{"id":"`+testEnrolID+`","user_id":"`+testUserID+`",
			 "created_at":"2026-07-01T00:00:00Z","expires_at":"2026-07-04T00:00:00Z",
			 "used_at":"2026-07-02T00:00:00Z"}]}`)
	}))

	rows, err := c.Users.Enrolments(context.Background(), testUserID)
	if err != nil {
		t.Fatalf("Enrolments: %v", err)
	}
	// `used_at` is the whole difference between an open invitation and a spent
	// one; a spent row is kept rather than deleted so the UI can say when.
	if rows[0].UsedAt != "" {
		t.Errorf("first should be open, got used_at = %q", rows[0].UsedAt)
	}
	if rows[1].UsedAt != "2026-07-02T00:00:00Z" {
		t.Errorf("second should be spent, got %q", rows[1].UsedAt)
	}
}

func TestUsersRevokeEnrolment(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		want := "/api/users/" + testUserID + "/enrolments/" + testEnrolID
		if r.URL.Path != want {
			t.Errorf("path = %q, want %q", r.URL.Path, want)
		}
		w.WriteHeader(http.StatusNoContent)
	}))

	if err := c.Users.RevokeEnrolment(context.Background(), testUserID, testEnrolID); err != nil {
		t.Fatalf("RevokeEnrolment: %v", err)
	}
}

func TestUsersRedeemEnrolment(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/auth/enrolments/redeem" {
			t.Errorf("path = %q", r.URL.Path)
		}
		writeJSON(w, testUserObject)
	}))

	u, err := c.Users.RedeemEnrolment(context.Background(), "tok", "a-password")
	if err != nil {
		t.Fatalf("RedeemEnrolment: %v", err)
	}
	if u.DisplayName != "Ana" {
		t.Errorf("display name = %q", u.DisplayName)
	}
}

func TestUsersAvailableMethods(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/auth/methods" {
			t.Errorf("path = %q", r.URL.Path)
		}
		// The public contract: no client id, no redirect URI.
		writeJSON(w, `{"password":true,"providers":[
			{"provider":"google","enabled":true,"configured":true},
			{"provider":"github","enabled":false,"configured":false}]}`)
	}))

	methods, err := c.Users.AvailableMethods(context.Background())
	if err != nil {
		t.Fatalf("AvailableMethods: %v", err)
	}
	if !methods.Password {
		t.Error("password must always be available — it is the floor")
	}
	if len(methods.Providers) != 2 || !methods.Providers[0].Enabled {
		t.Errorf("unexpected providers: %+v", methods.Providers)
	}
}

func TestUsersListOAuthProviders(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/auth/providers" {
			t.Errorf("path = %q", r.URL.Path)
		}
		writeJSON(w, `{"providers":[
			{"provider":"google","client_id":"the-id","enabled":true,"configured":true,
			 "redirect_uri":"https://home.example/api/auth/oauth/google/callback"},
			{"provider":"github","client_id":null,"enabled":false,"configured":false,
			 "redirect_uri":null}]}`)
	}))

	providers, err := c.Users.ListOAuthProviders(context.Background())
	if err != nil {
		t.Fatalf("ListOAuthProviders: %v", err)
	}
	// The admin projection carries what the public one withholds.
	if providers[0].ClientID != "the-id" {
		t.Errorf("client id = %q", providers[0].ClientID)
	}
	if providers[0].RedirectURI == "" {
		t.Error("redirect URI should be present for a configured provider")
	}
	if providers[1].ClientID != "" || providers[1].RedirectURI != "" {
		t.Errorf("nulls should become empty strings: %+v", providers[1])
	}
}

func TestUsersStartOAuth(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/auth/oauth/google/start" {
			t.Errorf("path = %q", r.URL.Path)
		}
		// Both pieces of caller intent must reach the ceremony here: the
		// callback carries no body, and `remember_me` cannot be raised later.
		q := r.URL.Query()
		if q.Get("return_to") != "admin_app" || q.Get("remember_me") != "true" {
			t.Errorf("query = %q", r.URL.RawQuery)
		}
		writeJSON(w, `{"url":"https://accounts.google.com/o/oauth2/v2/auth?x=1"}`)
	}))

	url, err := c.Users.StartOAuth(context.Background(), wardnet.ProviderGoogle, wardnet.ReturnToAdminApp, true)
	if err != nil {
		t.Fatalf("StartOAuth: %v", err)
	}
	if !strings.HasPrefix(url, "https://accounts.google.com/") {
		t.Errorf("url = %q", url)
	}
}

func TestUsersStartOAuthDefaultsReturnTo(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if got := r.URL.Query().Get("return_to"); got != "admin" {
			t.Errorf("return_to = %q, want the admin default", got)
		}
		writeJSON(w, `{"url":"https://accounts.google.com/o/oauth2/v2/auth"}`)
	}))

	if _, err := c.Users.StartOAuth(context.Background(), wardnet.ProviderGoogle, "", false); err != nil {
		t.Fatalf("StartOAuth: %v", err)
	}
}

func TestUsersConfigureOAuthProvider(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		_ = json.NewDecoder(r.Body).Decode(&body)
		if body["client_id"] != "the-id" || body["enabled"] != true {
			t.Errorf("unexpected body: %+v", body)
		}
		if body["client_secret"] != "s3cr3t" {
			t.Errorf("secret = %v", body["client_secret"])
		}
		writeJSON(w, `{"provider":"google","client_id":"the-id","enabled":true,
			"configured":true,"redirect_uri":null}`)
	}))

	status, err := c.Users.ConfigureOAuthProvider(
		context.Background(), wardnet.ProviderGoogle, "the-id", "s3cr3t", true)
	if err != nil {
		t.Fatalf("ConfigureOAuthProvider: %v", err)
	}
	if !status.Configured {
		t.Error("provider should report configured")
	}
}

func TestUsersConfigureOmitsAnEmptySecret(t *testing.T) {
	// Blank means "keep the stored secret", which the caller cannot read back —
	// sending "" would erase it.
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		if strings.Contains(string(raw), `"client_secret"`) {
			t.Errorf("body should omit client_secret, got %s", raw)
		}
		writeJSON(w, `{"provider":"google","client_id":"the-id","enabled":false,
			"configured":true,"redirect_uri":null}`)
	}))

	if _, err := c.Users.ConfigureOAuthProvider(
		context.Background(), wardnet.ProviderGoogle, "the-id", "", false); err != nil {
		t.Fatalf("ConfigureOAuthProvider: %v", err)
	}
}

func TestUsersClearOAuthProvider(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodDelete || r.URL.Path != "/api/auth/providers/github" {
			t.Errorf("%s %s", r.Method, r.URL.Path)
		}
		w.WriteHeader(http.StatusNoContent)
	}))

	if err := c.Users.ClearOAuthProvider(context.Background(), wardnet.ProviderGitHub); err != nil {
		t.Fatalf("ClearOAuthProvider: %v", err)
	}
}

func TestEnrolmentTTL(t *testing.T) {
	if hours := wardnet.EnrolmentTTL().Hours(); hours != 72 {
		t.Errorf("TTL = %v hours, want 72 to match the daemon", hours)
	}
}

func TestDevicesSetOwner(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPut || r.URL.Path != "/api/devices/"+testDeviceID+"/owner" {
			t.Errorf("%s %s", r.Method, r.URL.Path)
		}
		var body map[string]any
		_ = json.NewDecoder(r.Body).Decode(&body)
		if body["owner_user_id"] != testUserID {
			t.Errorf("owner = %v", body["owner_user_id"])
		}
		writeJSON(w, deviceDetailJSON)
	}))

	d, err := c.Devices.SetOwner(context.Background(), testDeviceID, testUserID)
	if err != nil {
		t.Fatalf("SetOwner: %v", err)
	}
	if d.MAC != "aa:bb:cc:11:22:02" {
		t.Errorf("mac = %q", d.MAC)
	}
}

func TestDevicesSetOwnerClears(t *testing.T) {
	// An empty owner must carry *no* owner. The generated body tags the field
	// `omitempty`, so this goes out as `{}` rather than an explicit null —
	// equivalent on the daemon side, which is pinned by
	// `set_device_owner_request_treats_an_absent_field_as_clear` in
	// wardnet-common. What matters here is that no id is sent: a stale one
	// would reassign rather than unassign.
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		if strings.Contains(string(raw), testUserID) {
			t.Errorf("body must carry no owner id, got %s", raw)
		}
		writeJSON(w, deviceDetailJSON)
	}))

	if _, err := c.Devices.SetOwner(context.Background(), testDeviceID, ""); err != nil {
		t.Fatalf("SetOwner: %v", err)
	}
}

func TestDevicesSetOwnerRejectsAMalformedOwner(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(http.ResponseWriter, *http.Request) {
		t.Error("no request should have been made")
	}))

	if _, err := c.Devices.SetOwner(context.Background(), testDeviceID, "not-a-uuid"); err == nil {
		t.Fatal("expected an error for a malformed owner id")
	}
}

const deviceDetailJSON = `{"device":{"id":"3f8a5a6b-2c1d-4f5e-8a9b-0c1d2e3f4a5b",
	"mac":"aa:bb:cc:11:22:02","name":"alice-phone","hostname":"alice-phone",
	"manufacturer":"Samsung","manufacturer_source":"ieee","is_randomized":false,
	"type":"phone","first_seen":"2026-08-01T00:00:00Z","last_seen":"2026-08-02T00:00:00Z",
	"last_ip":"192.168.1.42","admin_locked":false,
	"zone_id":"00000000-0000-0000-0000-000000000201","connection_mode":"lan",
	"managed":true,"owner_user_id":"6e05df45-1fa4-4327-8c1e-218c79b253ba",
	"dhcp_status":"lease","current_rule":null},"current_rule":null,"signals":[]}`
