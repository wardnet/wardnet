package wardnet

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"

	"github.com/coder/websocket"
)

// logStreamPath is the daemon's WebSocket log endpoint.
const logStreamPath = "/api/system/logs/stream"

// LogsService downloads and streams daemon logs.
type LogsService struct{ c *Client }

// LogEntry is one structured log line.
type LogEntry struct {
	// Timestamp is when the event was recorded, RFC 3339 with milliseconds.
	Timestamp string `json:"timestamp"`
	// Level is "TRACE", "DEBUG", "INFO", "WARN", or "ERROR".
	Level string `json:"level"`
	// Target is the emitting module path (e.g. "wardnetd_services::dns").
	Target string `json:"target"`
	// Message is the human-readable event text.
	Message string `json:"message"`
	// Fields are the event's structured fields, excluding the message.
	Fields map[string]string `json:"fields,omitempty"`
	// Span holds the fields of the enclosing tracing span, when there is one.
	Span map[string]string `json:"span,omitempty"`
}

// LogFilter narrows what the daemon sends on a log stream. Filtering happens
// server-side, so a narrow filter also saves bandwidth.
type LogFilter struct {
	// Level is the minimum severity to receive: "trace", "debug", "info",
	// "warn", or "error". Empty leaves the daemon's default ("info").
	Level string
	// Target keeps only entries whose target starts with this prefix. Empty
	// keeps every target.
	Target string
}

// LogEvent is one message from a log stream: either an entry or a notice
// that the daemon dropped entries because the client fell behind.
type LogEvent struct {
	// Entry is the log line, or nil for a drop notice.
	Entry *LogEntry
	// Skipped is how many entries were dropped, and is non-zero only on a
	// drop notice.
	Skipped int64
}

// LogStream is a live connection to the daemon's log stream. Call
// [LogStream.Recv] in a loop and [LogStream.Close] when finished.
type LogStream struct {
	conn *websocket.Conn
}

// Download fetches the daemon's log file as plain text.
func (s *LogsService) Download(ctx context.Context) ([]byte, error) {
	resp, err := s.c.rest.DownloadLogsWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.HTTPResponse == nil || resp.HTTPResponse.StatusCode != http.StatusOK {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return resp.Body, nil
}

// Stream opens the log stream and applies filter to it. The stream stays open
// until [LogStream.Close] is called or ctx is cancelled; ctx therefore governs
// the whole connection, not just the handshake.
func (s *LogsService) Stream(ctx context.Context, filter LogFilter) (*LogStream, error) {
	header := http.Header{}
	if s.c.token != "" {
		header.Set("Authorization", "Bearer "+s.c.token)
	}
	if s.c.userAgent != "" {
		header.Set("User-Agent", s.c.userAgent)
	}

	// The dial borrows the client's transport but never its timeout: a
	// deadline that is right for a request would cut the stream off mid-run.
	dialClient := &http.Client{}
	if s.c.httpClient != nil {
		dialClient.Transport = s.c.httpClient.Transport
		dialClient.Jar = s.c.httpClient.Jar
	}

	conn, resp, err := websocket.Dial(ctx, wsURL(s.c.baseURL), &websocket.DialOptions{
		HTTPClient: dialClient,
		HTTPHeader: header,
	})
	if err != nil {
		// A rejected upgrade is an ordinary HTTP failure (401 without a
		// token, 403 without admin), so report it as one. Only the first
		// kilobyte of a failed handshake's body is readable.
		if resp != nil {
			body, _ := io.ReadAll(io.LimitReader(resp.Body, 1024))
			return nil, apiError(resp, body)
		}
		return nil, fmt.Errorf("wardnet: cannot open log stream: %w", err)
	}

	stream := &LogStream{conn: conn}
	if filter.Level != "" || filter.Target != "" {
		if err := stream.setFilter(ctx, filter); err != nil {
			_ = stream.Close()
			return nil, err
		}
	}
	return stream, nil
}

// Recv blocks until the next stream message arrives. It returns an error once
// the daemon hangs up or ctx is cancelled — check ctx.Err() to tell a
// caller-requested stop from a lost connection.
func (s *LogStream) Recv(ctx context.Context) (LogEvent, error) {
	for {
		_, data, err := s.conn.Read(ctx)
		if err != nil {
			return LogEvent{}, err
		}
		// The daemon multiplexes drop notices onto the same socket, tagged
		// with a "type" the entries themselves never carry.
		var probe struct {
			Type    string `json:"type"`
			Skipped int64  `json:"skipped"`
		}
		if err := json.Unmarshal(data, &probe); err == nil && probe.Type == "lagged" {
			return LogEvent{Skipped: probe.Skipped}, nil
		}
		var entry LogEntry
		if err := json.Unmarshal(data, &entry); err != nil {
			// A message we cannot parse is not worth ending the stream over.
			continue
		}
		return LogEvent{Entry: &entry}, nil
	}
}

// Close ends the stream.
func (s *LogStream) Close() error {
	return s.conn.Close(websocket.StatusNormalClosure, "")
}

// setFilter tells the daemon which entries this client wants.
func (s *LogStream) setFilter(ctx context.Context, filter LogFilter) error {
	cmd := map[string]string{"type": "set_filter"}
	if filter.Level != "" {
		cmd["level"] = filter.Level
	}
	if filter.Target != "" {
		cmd["target"] = filter.Target
	}
	payload, err := json.Marshal(cmd)
	if err != nil {
		return err
	}
	return s.conn.Write(ctx, websocket.MessageText, payload)
}

// wsURL rewrites an http(s) base URL into the ws(s) log stream endpoint.
func wsURL(baseURL string) string {
	switch {
	case strings.HasPrefix(baseURL, "https://"):
		return "wss://" + strings.TrimPrefix(baseURL, "https://") + logStreamPath
	case strings.HasPrefix(baseURL, "http://"):
		return "ws://" + strings.TrimPrefix(baseURL, "http://") + logStreamPath
	default:
		return baseURL + logStreamPath
	}
}
