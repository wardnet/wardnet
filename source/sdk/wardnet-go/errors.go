package wardnet

import (
	"encoding/json"
	"fmt"
	"net/http"

	"wardnet.network/go/internal/rest"
)

// APIError is returned when the daemon answers with a non-2xx status. It
// carries the daemon's structured error body when present.
type APIError struct {
	// StatusCode is the HTTP status returned by the daemon.
	StatusCode int
	// Message is the daemon's short error string (e.g. "not found").
	Message string
	// Detail is the daemon's human-readable explanation, when provided.
	Detail string
	// RequestID correlates the failure with the daemon's structured logs,
	// when provided.
	RequestID string
}

func (e *APIError) Error() string {
	msg := fmt.Sprintf("wardnet: daemon returned %d: %s", e.StatusCode, e.Message)
	if e.Detail != "" {
		msg += " (" + e.Detail + ")"
	}
	return msg
}

// apiError builds an [APIError] from a non-2xx response, decoding the daemon's
// error envelope from body when it is valid JSON.
func apiError(resp *http.Response, body []byte) error {
	e := &APIError{StatusCode: statusOf(resp)}
	var env rest.ApiError
	if err := json.Unmarshal(body, &env); err == nil && env.Error != "" {
		e.Message = env.Error
		e.Detail = deref(env.Detail)
		e.RequestID = deref(env.RequestId)
	} else {
		e.Message = http.StatusText(e.StatusCode)
	}
	return e
}

func statusOf(resp *http.Response) int {
	if resp == nil {
		return 0
	}
	return resp.StatusCode
}
