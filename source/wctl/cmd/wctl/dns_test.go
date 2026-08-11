package main

import (
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

const (
	adBlockingID = "9d4b4d1c-3f4a-4a55-9f0c-6ac0d2a5b111"
	parentalID   = "7c2b4d1c-3f4a-4a55-9f0c-6ac0d2a5b222"
)

// filterProfilesServer serves the profile list plus a blocklist collection per
// profile, recording which profile the CLI actually asked for.
func filterProfilesServer(t *testing.T, requested *string) *httptest.Server {
	t.Helper()
	mux := http.NewServeMux()
	mux.HandleFunc("/api/dns/filter/profiles", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"profiles":[
			{"id":"`+adBlockingID+`","name":"Ad Blocking","description":"Seeded ad lists",
			 "builtin":true,"created_at":"2026-07-01T00:00:00Z","updated_at":"2026-07-01T00:00:00Z"},
			{"id":"`+parentalID+`","name":"Parental Controls","description":null,
			 "builtin":true,"created_at":"2026-07-01T00:00:00Z","updated_at":"2026-07-01T00:00:00Z"}]}`)
	})
	mux.HandleFunc("/api/dns/filter/profiles/{profileID}/blocklists",
		func(w http.ResponseWriter, r *http.Request) {
			*requested = r.PathValue("profileID")
			w.Header().Set("Content-Type", "application/json")
			_, _ = io.WriteString(w, `{"blocklists":[{
				"id":"1f8a5a6b-2c1d-4f5e-8a9b-0c1d2e3f4a5b","profile_id":"`+*requested+`",
				"name":"StevenBlack","url":"https://example.invalid/hosts","enabled":true,
				"entry_count":132000,"cron_schedule":"0 3 * * *",
				"last_updated":"2026-08-01T03:00:00Z","last_error":null,"last_error_at":null,
				"consecutive_failures":0,"created_at":"2026-07-01T00:00:00Z",
				"updated_at":"2026-08-01T03:00:00Z"}]}`)
		})
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	return srv
}

func TestDNSBlocklistsListDefaultsToAdBlocking(t *testing.T) {
	var requested string
	srv := filterProfilesServer(t, &requested)

	out, err := runCLI(t, srv.URL, "dns", "blocklists", "list")
	if err != nil {
		t.Fatalf("dns blocklists list: %v", err)
	}
	if requested != adBlockingID {
		t.Errorf("queried profile %q, want the Ad Blocking profile %q", requested, adBlockingID)
	}
	if !strings.Contains(out, "StevenBlack") || !strings.Contains(out, "132000") {
		t.Errorf("output missing blocklist row:\n%s", out)
	}
}

func TestDNSProfileFlagAcceptsName(t *testing.T) {
	var requested string
	srv := filterProfilesServer(t, &requested)

	if _, err := runCLI(t, srv.URL, "dns", "blocklists", "list", "--profile", "parental controls"); err != nil {
		t.Fatalf("dns blocklists list: %v", err)
	}
	if requested != parentalID {
		t.Errorf("queried profile %q, want %q", requested, parentalID)
	}
}

func TestDNSProfileFlagAcceptsID(t *testing.T) {
	var requested string
	srv := filterProfilesServer(t, &requested)

	if _, err := runCLI(t, srv.URL, "dns", "blocklists", "list", "--profile", parentalID); err != nil {
		t.Fatalf("dns blocklists list: %v", err)
	}
	if requested != parentalID {
		t.Errorf("queried profile %q, want %q", requested, parentalID)
	}
}

func TestDNSUnknownProfileNamesTheAlternatives(t *testing.T) {
	var requested string
	srv := filterProfilesServer(t, &requested)

	_, err := runCLI(t, srv.URL, "dns", "blocklists", "list", "--profile", "Nonsense")
	if err == nil {
		t.Fatal("unknown profile: want error")
	}
	if !strings.Contains(err.Error(), "Ad Blocking") {
		t.Errorf("error %q does not list the available profiles", err)
	}
	if requested != "" {
		t.Errorf("blocklists fetched for %q despite the unknown profile", requested)
	}
}

func TestDNSBlocklistUpdateRequiresAField(t *testing.T) {
	var requested string
	srv := filterProfilesServer(t, &requested)

	// Without this guard, cobra's flag defaults would send an update that
	// silently renamed the blocklist to the empty string.
	_, err := runCLI(t, srv.URL, "dns", "blocklists", "update",
		"1f8a5a6b-2c1d-4f5e-8a9b-0c1d2e3f4a5b")
	if err == nil {
		t.Fatal("update with no flags: want error")
	}
	if !strings.Contains(err.Error(), "nothing to update") {
		t.Errorf("unexpected error: %v", err)
	}
}
