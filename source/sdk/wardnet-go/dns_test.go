package wardnet_test

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"testing"

	wardnet "wardnet.network/go"
)

const adBlockingProfileID = "9d4b4d1c-3f4a-4a55-9f0c-6ac0d2a5b111"

const blocklistJSON = `{
  "id":"1f8a5a6b-2c1d-4f5e-8a9b-0c1d2e3f4a5b",
  "profile_id":"` + adBlockingProfileID + `",
  "name":"StevenBlack","url":"https://example.invalid/hosts",
  "enabled":true,"entry_count":132000,"cron_schedule":"0 3 * * *",
  "last_updated":"2026-08-01T03:00:00Z","last_error":null,"last_error_at":null,
  "consecutive_failures":0,
  "created_at":"2026-07-01T00:00:00Z","updated_at":"2026-08-01T03:00:00Z"
}`

func TestDNSConfig(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/dns/config" {
			t.Errorf("path = %q", r.URL.Path)
		}
		writeJSON(w, `{"config":{
			"enabled":true,"resolution_mode":"forwarding",
			"upstream_servers":[{"name":"Cloudflare","address":"1.1.1.1","protocol":"tls",
				"port":853,"tls_server_name":"cloudflare-dns.com"}],
			"forwarder_selection_mode":"fastest","single_upstream":null,
			"cache_size":10000,"cache_ttl_min_secs":60,"cache_ttl_max_secs":86400,
			"dnssec_enabled":true,"rebinding_protection":true,"rate_limit_per_second":100,
			"dns_filtering_enabled":true,"query_log_enabled":true,"query_log_retention_days":7}}`)
	}))

	cfg, err := c.DNS.Config(context.Background())
	if err != nil {
		t.Fatalf("Config: %v", err)
	}
	if !cfg.Enabled || cfg.ResolutionMode != "forwarding" || cfg.CacheSize != 10000 {
		t.Errorf("unexpected config: %+v", cfg)
	}
	if len(cfg.UpstreamServers) != 1 {
		t.Fatalf("upstreams = %d, want 1", len(cfg.UpstreamServers))
	}
	up := cfg.UpstreamServers[0]
	if up.Address != "1.1.1.1" || up.Port != 853 || up.TLSServerName != "cloudflare-dns.com" {
		t.Errorf("unexpected upstream: %+v", up)
	}
	// A null single_upstream must land as the empty string, not a panic.
	if cfg.SingleUpstream != "" {
		t.Errorf("single upstream = %q, want empty", cfg.SingleUpstream)
	}
}

func TestDNSSetEnabledSendsToggle(t *testing.T) {
	var body map[string]any
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/api/dns/config/toggle" {
			t.Errorf("%s %s", r.Method, r.URL.Path)
		}
		raw, _ := io.ReadAll(r.Body)
		_ = json.Unmarshal(raw, &body)
		writeJSON(w, `{"config":{
			"enabled":false,"resolution_mode":"forwarding","upstream_servers":[],
			"forwarder_selection_mode":"failover","cache_size":0,
			"cache_ttl_min_secs":0,"cache_ttl_max_secs":0,"dnssec_enabled":false,
			"rebinding_protection":false,"rate_limit_per_second":0,
			"dns_filtering_enabled":false,"query_log_enabled":false,
			"query_log_retention_days":0}}`)
	}))

	cfg, err := c.DNS.SetEnabled(context.Background(), false)
	if err != nil {
		t.Fatalf("SetEnabled: %v", err)
	}
	if body["enabled"] != false {
		t.Errorf("body = %v, want enabled:false", body)
	}
	if cfg.Enabled {
		t.Error("returned config still enabled")
	}
}

func TestDNSBlocklistsScopedToProfile(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		want := "/api/dns/filter/profiles/" + adBlockingProfileID + "/blocklists"
		if r.URL.Path != want {
			t.Errorf("path = %q, want %q", r.URL.Path, want)
		}
		writeJSON(w, `{"blocklists":[`+blocklistJSON+`]}`)
	}))

	lists, err := c.DNS.Blocklists(context.Background(), adBlockingProfileID)
	if err != nil {
		t.Fatalf("Blocklists: %v", err)
	}
	if len(lists) != 1 {
		t.Fatalf("blocklists = %d, want 1", len(lists))
	}
	b := lists[0]
	if b.Name != "StevenBlack" || b.EntryCount != 132000 || !b.Enabled {
		t.Errorf("unexpected blocklist: %+v", b)
	}
	if b.LastUpdated == nil {
		t.Error("last_updated not decoded")
	}
	if b.LastError != "" {
		t.Errorf("last error = %q, want empty", b.LastError)
	}
}

func TestDNSUpdateBlocklistSendsOnlySetFields(t *testing.T) {
	var body map[string]any
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		_ = json.Unmarshal(raw, &body)
		writeJSON(w, `{"blocklist":`+blocklistJSON+`,"message":"updated"}`)
	}))

	enabled := false
	_, err := c.DNS.UpdateBlocklist(context.Background(), adBlockingProfileID,
		"1f8a5a6b-2c1d-4f5e-8a9b-0c1d2e3f4a5b", wardnet.UpdateBlocklistParams{Enabled: &enabled})
	if err != nil {
		t.Fatalf("UpdateBlocklist: %v", err)
	}
	if len(body) != 1 {
		t.Errorf("body = %v, want only the enabled field", body)
	}
	if body["enabled"] != false {
		t.Errorf("enabled = %v, want false", body["enabled"])
	}
}

func TestDNSAddAllowlistEntryOmitsEmptyReason(t *testing.T) {
	var body map[string]any
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		_ = json.Unmarshal(raw, &body)
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusCreated)
		_, _ = io.WriteString(w, `{"entry":{"id":"2a8a5a6b-2c1d-4f5e-8a9b-0c1d2e3f4a5b",
			"profile_id":"`+adBlockingProfileID+`","domain":"example.com","reason":null,
			"created_at":"2026-08-01T00:00:00Z"},"message":"created"}`)
	}))

	e, err := c.DNS.AddAllowlistEntry(context.Background(), adBlockingProfileID, "example.com", "")
	if err != nil {
		t.Fatalf("AddAllowlistEntry: %v", err)
	}
	if _, ok := body["reason"]; ok {
		t.Errorf("body = %v, want no reason field", body)
	}
	if e.Domain != "example.com" || e.Reason != "" {
		t.Errorf("unexpected entry: %+v", e)
	}
}

func TestDNSFlushCache(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/api/dns/cache/flush" {
			t.Errorf("%s %s", r.Method, r.URL.Path)
		}
		writeJSON(w, `{"entries_cleared":42,"message":"flushed"}`)
	}))

	n, err := c.DNS.FlushCache(context.Background())
	if err != nil {
		t.Fatalf("FlushCache: %v", err)
	}
	if n != 42 {
		t.Errorf("entries cleared = %d, want 42", n)
	}
}

func TestDNSInvalidProfileIDNoRequest(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(_ http.ResponseWriter, _ *http.Request) {
		t.Error("daemon called with an unparseable profile ID")
	}))

	if _, err := c.DNS.Blocklists(context.Background(), "not-a-uuid"); err == nil {
		t.Fatal("Blocklists: want error")
	}
}
