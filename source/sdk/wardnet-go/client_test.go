package wardnet_test

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
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
  "connection_mode":"lan","dhcp_status":"lease","managed":true,
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
	if !d.Managed {
		t.Error("managed = false, want true")
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

func TestAnomaliesList(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/anomalies" {
			t.Errorf("path = %q", r.URL.Path)
		}
		// The zero-value options must not pin any filter: the daemon's own
		// defaults (open, limit 50) are what a bare `list` should get.
		if q := r.URL.RawQuery; q != "" {
			t.Errorf("query = %q, want empty", q)
		}
		writeJSON(w, `{"anomalies":[{"id":"00000000-0000-0000-0000-000000000001",
			"type":"blocklist_refresh_failing","severity":"error","component":"dns",
			"subject_id":"bl-1","message":"failed 7 times","hint":"check the URL",
			"details":{"profile_id":"p-1"},"opened_at":"2026-08-01T00:00:00Z",
			"last_seen_at":"2026-08-01T06:00:00Z","occurrences":7,"resolved_at":null}]}`)
	}))

	anomalies, err := c.Anomalies.List(context.Background(), wardnet.ListAnomaliesOptions{})
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(anomalies) != 1 {
		t.Fatalf("anomalies = %d, want 1", len(anomalies))
	}
	a := anomalies[0]
	if a.Type != "blocklist_refresh_failing" || a.Severity != "error" {
		t.Errorf("unexpected anomaly: %+v", a)
	}
	if a.SubjectID != "bl-1" || a.Occurrences != 7 {
		t.Errorf("subject/occurrences = %q/%d", a.SubjectID, a.Occurrences)
	}
	if !a.Open() {
		t.Error("anomaly with no resolved_at should read as open")
	}
	if a.Details["profile_id"] != "p-1" {
		t.Errorf("details = %v", a.Details)
	}
}

func TestAnomaliesListFilters(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		q := r.URL.Query()
		if q.Get("status") != "all" || q.Get("subject_id") != "tun-1" || q.Get("limit") != "5" {
			t.Errorf("query = %q", r.URL.RawQuery)
		}
		writeJSON(w, `{"anomalies":[]}`)
	}))

	if _, err := c.Anomalies.List(context.Background(), wardnet.ListAnomaliesOptions{
		Status:    wardnet.AnomalyStatusAll,
		SubjectID: "tun-1",
		Limit:     5,
	}); err != nil {
		t.Fatalf("List: %v", err)
	}
}

func TestAnomaliesReevaluate(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/api/anomalies/reevaluate" {
			t.Errorf("%s %s", r.Method, r.URL.Path)
		}
		writeJSON(w, `{"evaluated":4,"resolved":1}`)
	}))

	result, err := c.Anomalies.Reevaluate(context.Background())
	if err != nil {
		t.Fatalf("Reevaluate: %v", err)
	}
	if result.Evaluated != 4 || result.Resolved != 1 {
		t.Errorf("unexpected result: %+v", result)
	}
}

func TestSystemRestart(t *testing.T) {
	var called bool
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		called = true
		if r.Method != http.MethodPost || r.URL.Path != "/api/system/restart" {
			t.Errorf("%s %s", r.Method, r.URL.Path)
		}
		w.WriteHeader(http.StatusNoContent)
	}))

	if err := c.System.Restart(context.Background()); err != nil {
		t.Fatalf("Restart: %v", err)
	}
	if !called {
		t.Error("daemon never called")
	}
}

func TestSystemRestartError(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusForbidden)
		_, _ = io.WriteString(w, `{"error":"forbidden"}`)
	}))

	err := c.System.Restart(context.Background())
	var apiErr *wardnet.APIError
	if !errors.As(err, &apiErr) || apiErr.StatusCode != http.StatusForbidden {
		t.Fatalf("error = %v, want a 403 APIError", err)
	}
}

// Release is destructive — it revokes the device's Private-DNS grant and
// Remote peer credential — so this pins the method, verb and path rather than
// leaving a mis-wired route to be discovered against a live daemon.
func TestDevicesRelease(t *testing.T) {
	const id = "6e05df45-1fa4-4327-8c1e-218c79b253ba"
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Errorf("method = %s, want POST", r.Method)
		}
		if want := "/api/devices/" + id + "/release"; r.URL.Path != want {
			t.Errorf("path = %q, want %q", r.URL.Path, want)
		}
		// A released device comes back unmanaged and unnamed.
		writeJSON(w, `{"device":`+strings.Replace(
			strings.Replace(deviceJSON, `"managed":true`, `"managed":false`, 1),
			`"name":"alice-phone"`, `"name":null`, 1)+`,"current_rule":null,"signals":[]}`)
	}))

	d, err := c.Devices.Release(context.Background(), id)
	if err != nil {
		t.Fatalf("Release: %v", err)
	}
	if d.Managed {
		t.Error("managed = true, want false after release")
	}
	if d.Name != "" {
		t.Errorf("name = %q, want empty after release", d.Name)
	}
	if d.Rule != nil {
		t.Errorf("rule = %+v, want nil after release", d.Rule)
	}
}

func TestDevicesReleaseRejectsBadID(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(_ http.ResponseWriter, _ *http.Request) {
		t.Error("no request should be sent for a malformed id")
	}))

	if _, err := c.Devices.Release(context.Background(), "not-a-uuid"); err == nil {
		t.Fatal("Release: expected an error for a malformed device id")
	}
}

// The anomaly endpoints are admin-gated, so a non-admin caller gets a 403.
// That has to surface as an APIError rather than an empty listing, which
// would read as "nothing is wrong with the box".
func TestAnomaliesListSurfacesAPIErrors(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusForbidden)
		_, _ = io.WriteString(w, `{"error":"admin privileges required"}`)
	}))

	_, err := c.Anomalies.List(context.Background(), wardnet.ListAnomaliesOptions{})
	var apiErr *wardnet.APIError
	if !errors.As(err, &apiErr) || apiErr.StatusCode != http.StatusForbidden {
		t.Fatalf("error = %v, want a 403 APIError", err)
	}
}

func TestAnomaliesReevaluateSurfacesAPIErrors(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusInternalServerError)
		_, _ = io.WriteString(w, `{"error":"detector panicked"}`)
	}))

	_, err := c.Anomalies.Reevaluate(context.Background())
	var apiErr *wardnet.APIError
	if !errors.As(err, &apiErr) || apiErr.StatusCode != http.StatusInternalServerError {
		t.Fatalf("error = %v, want a 500 APIError", err)
	}
}

// A daemon that is not reachable at all is a different failure from one that
// answers with an error, and both callers (wctl, the PWA) distinguish them.
func TestAnomaliesSurfaceTransportErrors(t *testing.T) {
	srv := httptest.NewServer(http.NotFoundHandler())
	url := srv.URL
	srv.Close() // nothing is listening now

	c, err := wardnet.New(url, wardnet.WithToken("test-token"))
	if err != nil {
		t.Fatalf("New: %v", err)
	}

	if _, err := c.Anomalies.List(context.Background(), wardnet.ListAnomaliesOptions{}); err == nil {
		t.Error("List against a dead daemon should error")
	}
	if _, err := c.Anomalies.Reevaluate(context.Background()); err == nil {
		t.Error("Reevaluate against a dead daemon should error")
	}
}
