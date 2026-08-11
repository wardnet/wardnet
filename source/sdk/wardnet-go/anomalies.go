package wardnet

import (
	"context"
	"time"

	"wardnet.network/go/internal/rest"
)

// AnomaliesService reads the daemon's anomalies — typed admin-facing
// conditions with an open/resolved lifecycle.
type AnomaliesService struct{ c *Client }

// AnomalyStatusFilter selects which anomalies a listing returns.
type AnomalyStatusFilter string

const (
	// AnomalyStatusOpen returns only conditions that still hold. The default.
	AnomalyStatusOpen AnomalyStatusFilter = "open"
	// AnomalyStatusResolved returns only conditions that have since cleared.
	AnomalyStatusResolved AnomalyStatusFilter = "resolved"
	// AnomalyStatusAll returns both.
	AnomalyStatusAll AnomalyStatusFilter = "all"
)

// Anomaly is a typed condition worth surfacing to the box's administrator.
//
// It is opened once when first observed, refreshed while it persists, and
// resolved when it stops holding — so a problem lasting days is one Anomaly,
// not one per observation.
type Anomaly struct {
	ID string
	// Type is the stable catalogue identifier, e.g. "tunnel_start_failed".
	Type string
	// Severity is one of "error", "warning", "info".
	Severity string
	// Component is the subsystem that raised it, e.g. "dns".
	Component string
	// SubjectID is the entity this is about — a tunnel id, a blocklist id —
	// or empty for box-wide conditions.
	SubjectID string
	// Message describes what happened, in plain language.
	Message string
	// Hint is what the admin can do about it.
	Hint string
	// Details is a detector-authored payload. Best-effort: no key is
	// guaranteed beyond those the owning detector documents.
	Details map[string]any
	// OpenedAt is when the anomaly was first raised.
	OpenedAt time.Time
	// LastSeenAt is when the condition was last observed.
	LastSeenAt time.Time
	// Occurrences counts observations since it opened.
	Occurrences int32
	// ResolvedAt is nil while the anomaly is still open.
	ResolvedAt *time.Time
}

// Open reports whether the underlying condition is still believed to hold.
func (a Anomaly) Open() bool { return a.ResolvedAt == nil }

// ListAnomaliesOptions filters a listing. The zero value lists open
// anomalies with the daemon's default limit.
type ListAnomaliesOptions struct {
	// Status defaults to AnomalyStatusOpen when empty.
	Status AnomalyStatusFilter
	// SubjectID restricts the listing to anomalies about one entity.
	SubjectID string
	// Limit is clamped by the daemon to 1..=200; 0 uses its default of 50.
	Limit int32
}

// ReevaluateResult reports what a reevaluation pass checked and closed.
type ReevaluateResult struct {
	// Evaluated is how many open anomalies were examined.
	Evaluated int32
	// Resolved is how many of those were closed.
	Resolved int32
}

// List returns anomalies matching opts, newest first.
func (s *AnomaliesService) List(ctx context.Context, opts ListAnomaliesOptions) ([]Anomaly, error) {
	params := &rest.ListAnomaliesParams{}
	if opts.Status != "" {
		status := rest.AnomalyQueryStatus(opts.Status)
		params.Status = &status
	}
	if opts.SubjectID != "" {
		params.SubjectId = &opts.SubjectID
	}
	if opts.Limit > 0 {
		params.Limit = &opts.Limit
	}

	resp, err := s.c.rest.ListAnomaliesWithResponse(ctx, params)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}

	out := make([]Anomaly, 0, len(resp.JSON200.Anomalies))
	for _, a := range resp.JSON200.Anomalies {
		out = append(out, toAnomaly(a))
	}
	return out, nil
}

// Reevaluate re-checks every open anomaly now rather than waiting for the
// daemon's next scheduled pass, closing the ones whose condition no longer
// holds. Closing one that was alerted also sends its recovery notification.
func (s *AnomaliesService) Reevaluate(ctx context.Context) (*ReevaluateResult, error) {
	resp, err := s.c.rest.ReevaluateAnomaliesWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return &ReevaluateResult{
		Evaluated: resp.JSON200.Evaluated,
		Resolved:  resp.JSON200.Resolved,
	}, nil
}

func toAnomaly(a rest.ApiAnomaly) Anomaly {
	out := Anomaly{
		ID:          a.Id.String(),
		Type:        string(a.Type),
		Severity:    string(a.Severity),
		Component:   a.Component,
		Message:     a.Message,
		Hint:        a.Hint,
		OpenedAt:    a.OpenedAt,
		LastSeenAt:  a.LastSeenAt,
		Occurrences: a.Occurrences,
		ResolvedAt:  a.ResolvedAt,
	}
	if a.SubjectId != nil {
		out.SubjectID = *a.SubjectId
	}
	if a.Details != nil {
		out.Details = *a.Details
	}
	return out
}
