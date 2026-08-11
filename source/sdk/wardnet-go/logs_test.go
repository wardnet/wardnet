package wardnet_test

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"maps"
	"net/http"
	"testing"
	"time"

	"github.com/coder/websocket"
	wardnet "wardnet.network/go"
)

// logStreamServer accepts the upgrade, records the first client command, and
// sends each message in turn before closing.
func logStreamServer(t *testing.T, gotCommand chan<- string, messages ...string) http.Handler {
	t.Helper()
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/system/logs/stream" {
			t.Errorf("path = %q", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer test-token" {
			t.Errorf("Authorization = %q, want Bearer test-token", got)
		}
		conn, err := websocket.Accept(w, r, nil)
		if err != nil {
			t.Errorf("Accept: %v", err)
			return
		}
		defer func() { _ = conn.CloseNow() }()

		ctx, cancel := context.WithTimeout(r.Context(), 5*time.Second)
		defer cancel()

		if gotCommand != nil {
			_, data, err := conn.Read(ctx)
			if err != nil {
				t.Errorf("Read: %v", err)
				return
			}
			gotCommand <- string(data)
		}
		for _, m := range messages {
			if err := conn.Write(ctx, websocket.MessageText, []byte(m)); err != nil {
				return
			}
		}
		_ = conn.Close(websocket.StatusNormalClosure, "")
	})
}

func TestLogsStreamEntries(t *testing.T) {
	c := newClient(t, logStreamServer(t, nil,
		`{"timestamp":"2026-08-01T00:00:00.123Z","level":"INFO","target":"wardnetd::dns",
		  "message":"resolver started","fields":{"port":"53"}}`,
		`{"type":"lagged","skipped":7}`,
	))

	stream, err := c.Logs.Stream(context.Background(), wardnet.LogFilter{})
	if err != nil {
		t.Fatalf("Stream: %v", err)
	}
	defer func() { _ = stream.Close() }()

	ev, err := stream.Recv(context.Background())
	if err != nil {
		t.Fatalf("Recv: %v", err)
	}
	if ev.Entry == nil {
		t.Fatal("first event carried no entry")
	}
	if ev.Entry.Level != "INFO" || ev.Entry.Message != "resolver started" {
		t.Errorf("unexpected entry: %+v", ev.Entry)
	}
	if ev.Entry.Fields["port"] != "53" {
		t.Errorf("fields = %v", ev.Entry.Fields)
	}

	ev, err = stream.Recv(context.Background())
	if err != nil {
		t.Fatalf("Recv: %v", err)
	}
	if ev.Entry != nil || ev.Skipped != 7 {
		t.Errorf("want a drop notice for 7 entries, got %+v", ev)
	}
}

func TestLogsStreamSendsFilter(t *testing.T) {
	gotCommand := make(chan string, 1)
	c := newClient(t, logStreamServer(t, gotCommand))

	stream, err := c.Logs.Stream(context.Background(), wardnet.LogFilter{
		Level:  "warn",
		Target: "wardnetd::dns",
	})
	if err != nil {
		t.Fatalf("Stream: %v", err)
	}
	defer func() { _ = stream.Close() }()

	select {
	case cmd := <-gotCommand:
		var got map[string]string
		if err := json.Unmarshal([]byte(cmd), &got); err != nil {
			t.Fatalf("command %s: %v", cmd, err)
		}
		want := map[string]string{"type": "set_filter", "level": "warn", "target": "wardnetd::dns"}
		if !maps.Equal(got, want) {
			t.Errorf("command = %v, want %v", got, want)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("no filter command arrived")
	}
}

func TestLogsStreamRejectedUpgradeIsAPIError(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusUnauthorized)
		_, _ = io.WriteString(w, `{"error":"unauthorized","detail":"admin session required"}`)
	}))

	_, err := c.Logs.Stream(context.Background(), wardnet.LogFilter{})
	if err == nil {
		t.Fatal("Stream: want error")
	}
	var apiErr *wardnet.APIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("error = %v, want *APIError", err)
	}
	if apiErr.StatusCode != http.StatusUnauthorized || apiErr.Message != "unauthorized" {
		t.Errorf("unexpected API error: %+v", apiErr)
	}
}

func TestLogsDownload(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/system/logs/download" {
			t.Errorf("path = %q", r.URL.Path)
		}
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		_, _ = io.WriteString(w, "2026-08-01 INFO started\n")
	}))

	body, err := c.Logs.Download(context.Background())
	if err != nil {
		t.Fatalf("Download: %v", err)
	}
	if string(body) != "2026-08-01 INFO started\n" {
		t.Errorf("body = %q", body)
	}
}

func TestLogsDownloadError(t *testing.T) {
	c := newClient(t, http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusForbidden)
		_, _ = io.WriteString(w, `{"error":"forbidden"}`)
	}))

	if _, err := c.Logs.Download(context.Background()); err == nil {
		t.Fatal("Download: want error")
	}
}
