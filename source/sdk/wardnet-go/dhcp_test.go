package wardnet_test

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"testing"

	wardnet "wardnet.network/go"
)

func TestDHCPConfig(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/dhcp/config" {
			t.Errorf("path = %q", r.URL.Path)
		}
		writeJSON(w, `{"config":{"enabled":true,"gateway_ip":"192.168.1.1","router_ip":null,
			"pool_start":"192.168.1.100","pool_end":"192.168.1.200","subnet_mask":"255.255.255.0",
			"upstream_dns":["1.1.1.1"],"lease_duration_secs":86400}}`)
	}))

	cfg, err := c.DHCP.Config(context.Background())
	if err != nil {
		t.Fatalf("Config: %v", err)
	}
	if cfg.PoolStart != "192.168.1.100" || cfg.LeaseDurationSecs != 86400 {
		t.Errorf("unexpected config: %+v", cfg)
	}
	if cfg.RouterIP != "" {
		t.Errorf("router IP = %q, want empty for a null", cfg.RouterIP)
	}
}

func TestDHCPLeases(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/dhcp/leases" {
			t.Errorf("path = %q", r.URL.Path)
		}
		writeJSON(w, `{"leases":[{"id":"3f8a5a6b-2c1d-4f5e-8a9b-0c1d2e3f4a5b",
			"mac_address":"aa:bb:cc:11:22:02","ip_address":"192.168.1.42",
			"hostname":"alice-phone","device_id":"6e05df45-1fa4-4327-8c1e-218c79b253ba",
			"status":"active","lease_start":"2026-08-01T00:00:00Z",
			"lease_end":"2026-08-02T00:00:00Z","created_at":"2026-08-01T00:00:00Z",
			"updated_at":"2026-08-01T00:00:00Z"}]}`)
	}))

	leases, err := c.DHCP.Leases(context.Background())
	if err != nil {
		t.Fatalf("Leases: %v", err)
	}
	if len(leases) != 1 {
		t.Fatalf("leases = %d, want 1", len(leases))
	}
	l := leases[0]
	if l.IP != "192.168.1.42" || l.Status != "active" || l.Hostname != "alice-phone" {
		t.Errorf("unexpected lease: %+v", l)
	}
	if l.DeviceID != "6e05df45-1fa4-4327-8c1e-218c79b253ba" {
		t.Errorf("device ID = %q", l.DeviceID)
	}
}

func TestDHCPLeaseWithoutDeviceOrHostname(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, `{"leases":[{"id":"3f8a5a6b-2c1d-4f5e-8a9b-0c1d2e3f4a5b",
			"mac_address":"aa:bb:cc:11:22:03","ip_address":"192.168.1.43",
			"hostname":null,"device_id":null,"status":"expired",
			"lease_start":"2026-08-01T00:00:00Z","lease_end":"2026-08-02T00:00:00Z",
			"created_at":"2026-08-01T00:00:00Z","updated_at":"2026-08-01T00:00:00Z"}]}`)
	}))

	leases, err := c.DHCP.Leases(context.Background())
	if err != nil {
		t.Fatalf("Leases: %v", err)
	}
	if leases[0].DeviceID != "" || leases[0].Hostname != "" {
		t.Errorf("nulls not flattened: %+v", leases[0])
	}
}

func TestDHCPAddReservationOmitsEmptyOptionals(t *testing.T) {
	var body map[string]any
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/api/dhcp/reservations" {
			t.Errorf("%s %s", r.Method, r.URL.Path)
		}
		raw, _ := io.ReadAll(r.Body)
		_ = json.Unmarshal(raw, &body)
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusCreated)
		_, _ = io.WriteString(w, `{"reservation":{"id":"4f8a5a6b-2c1d-4f5e-8a9b-0c1d2e3f4a5b",
			"mac_address":"aa:bb:cc:11:22:04","ip_address":"192.168.1.50","hostname":null,
			"description":null,"created_at":"2026-08-01T00:00:00Z",
			"updated_at":"2026-08-01T00:00:00Z"},"message":"created"}`)
	}))

	r, err := c.DHCP.AddReservation(context.Background(), wardnet.CreateReservationParams{
		MAC: "aa:bb:cc:11:22:04",
		IP:  "192.168.1.50",
	})
	if err != nil {
		t.Fatalf("AddReservation: %v", err)
	}
	if _, ok := body["hostname"]; ok {
		t.Errorf("body = %v, want no hostname field", body)
	}
	if r.IP != "192.168.1.50" {
		t.Errorf("unexpected reservation: %+v", r)
	}
}

func TestDHCPRevokeLeaseInvalidIDNoRequest(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(_ http.ResponseWriter, _ *http.Request) {
		t.Error("daemon called with an unparseable lease ID")
	}))

	if err := c.DHCP.RevokeLease(context.Background(), "nope"); err == nil {
		t.Fatal("RevokeLease: want error")
	}
}
