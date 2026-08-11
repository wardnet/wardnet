package main

import (
	"bufio"
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"

	"github.com/coder/websocket"
	wardnet "wardnet.network/go"
)

func TestFormatLogEntry(t *testing.T) {
	entry := &wardnet.LogEntry{
		Timestamp: "2026-08-01T00:00:00.123Z",
		Level:     "WARN",
		Target:    "wardnetd::dns",
		Message:   "upstream timed out",
		Fields:    map[string]string{"upstream": "1.1.1.1", "attempt": "2"},
		Span:      map[string]string{"request_id": "abc"},
	}

	// Span context first, then event fields, each alphabetical, so repeated
	// lines stay aligned instead of shuffling with map iteration order.
	want := "2026-08-01T00:00:00.123Z WARN  wardnetd::dns: upstream timed out " +
		"request_id=abc attempt=2 upstream=1.1.1.1"
	if got := formatLogEntry(entry); got != want {
		t.Errorf("formatLogEntry() =\n%q\nwant\n%q", got, want)
	}
}

func TestFormatLogEntryWithoutFields(t *testing.T) {
	entry := &wardnet.LogEntry{
		Timestamp: "2026-08-01T00:00:00.123Z",
		Level:     "INFO",
		Target:    "wardnetd",
		Message:   "started",
	}
	want := "2026-08-01T00:00:00.123Z INFO  wardnetd: started"
	if got := formatLogEntry(entry); got != want {
		t.Errorf("formatLogEntry() = %q, want %q", got, want)
	}
}

func TestLogsFilterFlagsRequireFollow(t *testing.T) {
	// The filters are applied server-side on the live stream, so accepting
	// them for a file download would silently do nothing.
	_, err := runCLI(t, "http://127.0.0.1:1", "logs", "--level", "warn")
	if err == nil {
		t.Fatal("logs --level without --follow: want error")
	}
}

// logStreamServer accepts the upgrade and pushes one entry, then blocks until
// the client goes away — standing in for a daemon with a quiet log.
func logStreamServer(t *testing.T, entry string) *httptest.Server {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		conn, err := websocket.Accept(w, r, nil)
		if err != nil {
			t.Errorf("Accept: %v", err)
			return
		}
		defer func() { _ = conn.CloseNow() }()
		if err := conn.Write(r.Context(), websocket.MessageText, []byte(entry)); err != nil {
			return
		}
		<-r.Context().Done()
	}))
	t.Cleanup(srv.Close)
	return srv
}

func TestFollowLogsWritesToStderrUntilCancelled(t *testing.T) {
	srv := logStreamServer(t, `{"timestamp":"2026-08-01T00:00:00.123Z","level":"INFO",
		"target":"wardnetd::dns","message":"resolver started"}`)

	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	t.Setenv("WARDNET_DAEMON_URL", srv.URL)
	t.Setenv("WARDNET_TOKEN", "test-token")

	c := &cli{}
	client, err := c.client()
	if err != nil {
		t.Fatalf("client: %v", err)
	}

	r, w, err := os.Pipe()
	if err != nil {
		t.Fatalf("pipe: %v", err)
	}
	realStderr := os.Stderr
	os.Stderr = w

	// Stand in for SIGINT: the signal reaches the command as a cancelled
	// root context, and a requested stop must not be reported as a failure.
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	lines := make(chan string, 1)
	go func() {
		buf := bufio.NewReader(r)
		line, _ := buf.ReadString('\n')
		lines <- line
		cancel()
	}()

	runErr := followLogs(ctx, c, client, wardnet.LogFilter{})

	os.Stderr = realStderr
	_ = w.Close()
	_ = r.Close()

	if runErr != nil {
		t.Errorf("followLogs after cancel = %v, want nil", runErr)
	}
	got := <-lines
	if !strings.Contains(got, "resolver started") {
		t.Errorf("stderr line = %q, want the log message", got)
	}
}
