package main

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

const blocklistSubjectID = "00000000-0000-0000-0000-0000000001a1"

// anomaliesServer serves a two-entry listing plus the reevaluate endpoint,
// recording the query string the CLI actually sent so the flag tests can
// assert on it.
func anomaliesServer(t *testing.T, query *string) *httptest.Server {
	t.Helper()
	mux := http.NewServeMux()
	mux.HandleFunc("/api/anomalies", func(w http.ResponseWriter, r *http.Request) {
		if query != nil {
			*query = r.URL.RawQuery
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"anomalies":[
			{"id":"11111111-1111-1111-1111-111111111111",
			 "type":"blocklist_refresh_failing","severity":"error","component":"dns",
			 "subject_id":"`+blocklistSubjectID+`",
			 "message":"Blocklist \"HaGeZi Pro\" has failed to refresh 7 times",
			 "hint":"check the URL is still reachable","details":{"profile_id":"p-1"},
			 "opened_at":"2026-08-01T00:00:00Z","last_seen_at":"2026-08-01T06:00:00Z",
			 "occurrences":7,"resolved_at":null},
			{"id":"22222222-2222-2222-2222-222222222222",
			 "type":"update_failed","severity":"warning","component":"update",
			 "subject_id":null,"message":"Update to 2026.08.01 failed",
			 "hint":"retry the update","details":null,
			 "opened_at":"2026-08-02T00:00:00Z","last_seen_at":"2026-08-02T00:00:00Z",
			 "occurrences":1,"resolved_at":null}]}`)
	})
	mux.HandleFunc("/api/anomalies/reevaluate", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"evaluated":6,"resolved":1}`)
	})
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)
	return srv
}

func TestAnomaliesListRendersATable(t *testing.T) {
	srv := anomaliesServer(t, nil)

	out, err := runCLI(t, srv.URL, "anomalies", "list")
	if err != nil {
		t.Fatalf("anomalies list: %v", err)
	}

	for _, want := range []string{
		"OPENED", "SEVERITY", "TYPE", "SUBJECT", "SEEN", "MESSAGE",
		"blocklist_refresh_failing", "HaGeZi Pro", "update_failed",
	} {
		if !strings.Contains(out, want) {
			t.Errorf("output missing %q:\n%s", want, out)
		}
	}
	// An anomaly with no subject still occupies the column — a blank cell
	// would silently shift every field after it.
	if !strings.Contains(out, "-") {
		t.Errorf("a subject-less anomaly should render a dash:\n%s", out)
	}
}

// The daemon defaults to open anomalies, so the CLI sends no status unless
// asked. Passing the flags through wrongly would quietly list the wrong set.
func TestAnomaliesListPassesFiltersThrough(t *testing.T) {
	var query string
	srv := anomaliesServer(t, &query)

	if _, err := runCLI(t, srv.URL, "anomalies", "list",
		"--status", "all", "--subject", blocklistSubjectID); err != nil {
		t.Fatalf("anomalies list: %v", err)
	}

	if !strings.Contains(query, "status=all") {
		t.Errorf("status filter not sent, query was %q", query)
	}
	if !strings.Contains(query, "subject_id="+blocklistSubjectID) {
		t.Errorf("subject filter not sent, query was %q", query)
	}
}

// Nothing wrong with the box is the common case, and an empty table reads as
// a rendering failure rather than good news.
func TestAnomaliesListSaysSoWhenThereAreNone(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/anomalies", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"anomalies":[]}`)
	})
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)

	out, err := runCLI(t, srv.URL, "anomalies", "list")
	if err != nil {
		t.Fatalf("anomalies list: %v", err)
	}
	if !strings.Contains(out, "no anomalies") {
		t.Errorf("want a plain-language empty state, got:\n%s", out)
	}
}

func TestAnomaliesListJSONOutIsMachineReadable(t *testing.T) {
	srv := anomaliesServer(t, nil)

	out, err := runCLI(t, srv.URL, "--json", "anomalies", "list")
	if err != nil {
		t.Fatalf("anomalies list --json: %v", err)
	}

	var got []struct {
		Type        string `json:"type"`
		Occurrences int    `json:"occurrences"`
	}
	if err := json.Unmarshal([]byte(out), &got); err != nil {
		t.Fatalf("--json output is not valid JSON: %v\n%s", err, out)
	}
	if len(got) != 2 {
		t.Fatalf("got %d anomalies, want 2", len(got))
	}
	if got[0].Type != "blocklist_refresh_failing" || got[0].Occurrences != 7 {
		t.Errorf("first anomaly decoded as %+v", got[0])
	}
}

func TestAnomaliesReevaluateReportsTheSummary(t *testing.T) {
	srv := anomaliesServer(t, nil)

	out, err := runCLI(t, srv.URL, "anomalies", "reevaluate")
	if err != nil {
		t.Fatalf("anomalies reevaluate: %v", err)
	}
	if !strings.Contains(out, "evaluated 6") || !strings.Contains(out, "resolved 1") {
		t.Errorf("want the evaluated/resolved summary, got:\n%s", out)
	}
}

func TestAnomaliesReevaluateJSONOut(t *testing.T) {
	srv := anomaliesServer(t, nil)

	out, err := runCLI(t, srv.URL, "--json", "anomalies", "reevaluate")
	if err != nil {
		t.Fatalf("anomalies reevaluate --json: %v", err)
	}

	var got struct {
		Evaluated int `json:"evaluated"`
		Resolved  int `json:"resolved"`
	}
	if err := json.Unmarshal([]byte(out), &got); err != nil {
		t.Fatalf("--json output is not valid JSON: %v\n%s", err, out)
	}
	if got.Evaluated != 6 || got.Resolved != 1 {
		t.Errorf("decoded %+v, want evaluated 6 / resolved 1", got)
	}
}

// A daemon that rejects the call must surface as a CLI error, not a silent
// empty listing that reads as "nothing is wrong".
func TestAnomaliesListSurfacesDaemonErrors(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/anomalies", func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusForbidden)
		_, _ = io.WriteString(w, `{"error":"admin privileges required"}`)
	})
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)

	if _, err := runCLI(t, srv.URL, "anomalies", "list"); err == nil {
		t.Fatal("expected an error when the daemon refuses the request")
	}
}

// A reevaluate that fails must not print a summary — "evaluated 0, resolved 0"
// is indistinguishable from a healthy box that had nothing to close.
func TestAnomaliesReevaluateSurfacesDaemonErrors(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/anomalies/reevaluate", func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
		_, _ = io.WriteString(w, `{"error":"detector failed"}`)
	})
	srv := httptest.NewServer(mux)
	t.Cleanup(srv.Close)

	out, err := runCLI(t, srv.URL, "anomalies", "reevaluate")
	if err == nil {
		t.Fatal("expected an error when the daemon fails the reevaluate")
	}
	if strings.Contains(out, "evaluated") {
		t.Errorf("a failed reevaluate must not print a summary:\n%s", out)
	}
}
