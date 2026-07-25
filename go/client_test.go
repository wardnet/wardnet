package wardnet_test

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"testing"

	wardnet "wardnet.network/go"
)

const deviceJSON = `{
  "id":"6e05df45-1fa4-4327-8c1e-218c79b253ba",
  "mac":"aa:bb:cc:11:22:02","name":"alice-phone","hostname":"alice-phone",
  "manufacturer":"Samsung","device_type":"Phone",
  "first_seen":"2026-07-17T02:53:46Z","last_seen":"2026-07-24T02:53:16Z",
  "last_ip":"192.168.1.42","admin_locked":false,
  "zone_id":"00000000-0000-0000-0000-000000000001",
  "dns_capture_enabled":false,"dns_capture_cap_count":0,"dns_capture_cap_days":0,
  "connection_mode":"lan","dhcp_status":"lease",
  "current_rule":{"type":"tunnel","tunnel_id":"d4681478-8a90-4ce6-b220-0650a333d73c"}
}`

// newClient starts an httptest server with handler and returns a Client
// pointed at it, using a fixed token so header assertions are possible.
func newClient(t *testing.T, handler http.Handler) *wardnet.Client {
	t.Helper()
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)
	c, err := wardnet.New(srv.URL, wardnet.WithToken("test-token"))
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	return c
}

// writeJSON writes a 200 application/json response. The generated client only
// decodes a body into its typed field when the content type says JSON.
func writeJSON(w http.ResponseWriter, body string) {
	w.Header().Set("Content-Type", "application/json")
	_, _ = io.WriteString(w, body)
}

func TestSystemStatus(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if got := r.Header.Get("Authorization"); got != "Bearer test-token" {
			t.Errorf("Authorization = %q, want Bearer test-token", got)
		}
		if r.URL.Path != "/api/system/status" {
			t.Errorf("path = %q", r.URL.Path)
		}
		writeJSON(w, `{"version":"0.7.0","release_version":"2026.08.00",
			"uptime_seconds":123,"device_count":10,"tunnel_count":3,"tunnel_active_count":2,
			"db_size_bytes":34,"cpu_usage_percent":49.0,"memory_used_bytes":1,"memory_total_bytes":2,
			"disk_free_bytes":3,"disk_total_bytes":4,
			"last_shutdown":{"state":"unknown","at":null,"acknowledged_at":null}}`)
	}))

	st, err := c.System.Status(context.Background())
	if err != nil {
		t.Fatalf("Status: %v", err)
	}
	if st.ReleaseVersion != "2026.08.00" || st.DeviceCount != 10 || st.TunnelActiveCount != 2 {
		t.Errorf("unexpected status: %+v", st)
	}
	if st.LastShutdown.State != "unknown" {
		t.Errorf("shutdown state = %q", st.LastShutdown.State)
	}
}

func TestDevicesList(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, `{"devices":[`+deviceJSON+`]}`)
	}))

	devs, err := c.Devices.List(context.Background())
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(devs) != 1 {
		t.Fatalf("got %d devices", len(devs))
	}
	d := devs[0]
	if d.Name != "alice-phone" || d.LastIP != "192.168.1.42" || d.Type != "Phone" {
		t.Errorf("unexpected device: %+v", d)
	}
	if d.ConnectionMode != "lan" || d.DHCPStatus != "lease" {
		t.Errorf("connection_mode=%q dhcp_status=%q", d.ConnectionMode, d.DHCPStatus)
	}
	if d.Rule == nil || d.Rule.Kind != wardnet.RoutingTunnel {
		t.Fatalf("rule = %+v, want tunnel", d.Rule)
	}
	if d.Rule.TunnelID != "d4681478-8a90-4ce6-b220-0650a333d73c" {
		t.Errorf("tunnel id = %q", d.Rule.TunnelID)
	}
}

func TestDevicesSetRuleSendsOnlyRule(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPut {
			t.Errorf("method = %s, want PUT", r.Method)
		}
		var body map[string]json.RawMessage
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode body: %v", err)
		}
		// The partial update must carry only routing_target — never name,
		// which the daemon would otherwise clobber.
		if _, ok := body["routing_target"]; !ok {
			t.Error("body missing routing_target")
		}
		if _, ok := body["name"]; ok {
			t.Error("body must not include name")
		}
		writeJSON(w, `{"device":`+deviceJSON+`,"current_rule":{"type":"direct"}}`)
	}))

	got, err := c.Devices.SetRule(context.Background(),
		"6e05df45-1fa4-4327-8c1e-218c79b253ba", wardnet.RouteDirect())
	if err != nil {
		t.Fatalf("SetRule: %v", err)
	}
	// The fixture's nested device.current_rule is a tunnel while the
	// authoritative top-level current_rule is direct; the SDK must report the
	// authoritative one.
	if got.Rule == nil || got.Rule.Kind != wardnet.RoutingDirect {
		t.Errorf("rule = %+v, want direct (authoritative top-level current_rule)", got.Rule)
	}
}

func TestSetRuleInvalidTunnelIDNoRequest(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(_ http.ResponseWriter, _ *http.Request) {
		t.Error("server should not be reached for an invalid tunnel id")
	}))
	_, err := c.Devices.SetRule(context.Background(),
		"6e05df45-1fa4-4327-8c1e-218c79b253ba", wardnet.RouteTunnel("not-a-uuid"))
	if err == nil {
		t.Error("expected error for invalid tunnel id in routing target")
	}
}

func TestAPIError(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusNotFound)
		_, _ = io.WriteString(w, `{"error":"not found","detail":"tunnel x not found","request_id":"r1"}`)
	}))

	_, err := c.Tunnels.Get(context.Background(), "00000000-0000-0000-0000-000000000000")
	if err == nil {
		t.Fatal("expected error")
	}
	var apiErr *wardnet.APIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("error is %T, want *wardnet.APIError", err)
	}
	if apiErr.StatusCode != 404 || apiErr.Message != "not found" || apiErr.Detail != "tunnel x not found" {
		t.Errorf("unexpected APIError: %+v", apiErr)
	}
}

func TestInvalidIDNoRequest(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(_ http.ResponseWriter, _ *http.Request) {
		t.Error("server should not be reached for an invalid ID")
	}))
	if _, err := c.Devices.Get(context.Background(), "not-a-uuid"); err == nil {
		t.Error("expected error for invalid device id")
	}
}
