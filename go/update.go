package wardnet

import (
	"context"
	"encoding/json"
	"time"

	"wardnet.network/go/internal/rest"
)

// UpdateService drives the auto-update subsystem.
type UpdateService struct{ c *Client }

// InstallPhaseKind identifies an in-flight install's phase.
type InstallPhaseKind string

const (
	InstallIdle           InstallPhaseKind = "idle"
	InstallChecking       InstallPhaseKind = "checking"
	InstallDownloading    InstallPhaseKind = "downloading"
	InstallVerifying      InstallPhaseKind = "verifying"
	InstallStaging        InstallPhaseKind = "staging"
	InstallSwapping       InstallPhaseKind = "swapping"
	InstallRestartPending InstallPhaseKind = "restart_pending"
	InstallApplied        InstallPhaseKind = "applied"
	InstallFailed         InstallPhaseKind = "failed"
)

// InstallPhase describes where an in-flight install is. Bytes/Total are set
// while [InstallDownloading]; Reason is set on [InstallFailed].
type InstallPhase struct {
	Kind   InstallPhaseKind
	Bytes  int64
	Total  *int64
	Reason string
}

// MarshalJSON encodes the phase in the daemon's internally-tagged form.
func (p InstallPhase) MarshalJSON() ([]byte, error) {
	switch p.Kind {
	case InstallDownloading:
		return json.Marshal(struct {
			Phase string `json:"phase"`
			Bytes int64  `json:"bytes"`
			Total *int64 `json:"total"`
		}{"downloading", p.Bytes, p.Total})
	case InstallFailed:
		return json.Marshal(struct {
			Phase  string `json:"phase"`
			Reason string `json:"reason"`
		}{"failed", p.Reason})
	default:
		return json.Marshal(struct {
			Phase string `json:"phase"`
		}{string(p.Kind)})
	}
}

// UnmarshalJSON decodes the daemon's internally-tagged install-phase form.
func (p *InstallPhase) UnmarshalJSON(b []byte) error {
	var probe struct {
		Phase  string `json:"phase"`
		Bytes  int64  `json:"bytes"`
		Total  *int64 `json:"total"`
		Reason string `json:"reason"`
	}
	if err := json.Unmarshal(b, &probe); err != nil {
		return err
	}
	*p = InstallPhase{
		Kind:   InstallPhaseKind(probe.Phase),
		Bytes:  probe.Bytes,
		Total:  probe.Total,
		Reason: probe.Reason,
	}
	return nil
}

// InstallHandle identifies a kicked-off install.
type InstallHandle struct {
	// InstallID is the install operation's UUID.
	InstallID string `json:"install_id"`
	// TargetVersion is the version being installed.
	TargetVersion string `json:"target_version"`
}

// UpdateStatus is a snapshot of the auto-update subsystem.
type UpdateStatus struct {
	// CurrentVersion is the running daemon version.
	CurrentVersion string `json:"current_version"`
	// LatestVersion is the newest known version for the channel, or "".
	LatestVersion string `json:"latest_version"`
	// UpdateAvailable reports whether a newer version is available.
	UpdateAvailable bool `json:"update_available"`
	// AutoUpdateEnabled reports whether auto-update is on.
	AutoUpdateEnabled bool `json:"auto_update_enabled"`
	// Channel is the active release channel: "stable", "beta", or "edge".
	Channel string `json:"channel"`
	// LastCheckAt / LastInstallAt are timestamps, nil if never.
	LastCheckAt   *time.Time `json:"last_check_at"`
	LastInstallAt *time.Time `json:"last_install_at"`
	// Phase is the current install phase.
	Phase InstallPhase `json:"phase"`
	// PendingVersion is the in-flight install's target, or "".
	PendingVersion string `json:"pending_version"`
	// AppliedVersion is the version most recently applied across a restart,
	// or "".
	AppliedVersion string `json:"applied_version"`
	// AppliedAt is when AppliedVersion was first observed running, nil if none.
	AppliedAt *time.Time `json:"applied_at"`
	// RollbackAvailable reports whether a prior binary is present to roll back.
	RollbackAvailable bool `json:"rollback_available"`
	// EdgeAvailable reports whether this host may follow the edge channel.
	EdgeAvailable bool `json:"edge_available"`
}

// Status returns the current auto-update status.
func (s *UpdateService) Status(ctx context.Context) (*UpdateStatus, error) {
	resp, err := s.c.rest.UpdateStatusWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return updateStatusFromREST(&resp.JSON200.Status)
}

// Check forces a manifest refresh and returns the resulting status.
func (s *UpdateService) Check(ctx context.Context) (*UpdateStatus, error) {
	resp, err := s.c.rest.CheckWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return updateStatusFromREST(&resp.JSON200.Status)
}

// Install kicks off an install. Pass an empty version to install the channel's
// latest.
func (s *UpdateService) Install(ctx context.Context, version string) (*InstallHandle, error) {
	var body rest.InstallJSONRequestBody
	if version != "" {
		body.Version = ptr(version)
	}
	resp, err := s.c.rest.InstallWithResponse(ctx, body)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return &InstallHandle{
		InstallID:     resp.JSON200.Handle.InstallId.String(),
		TargetVersion: resp.JSON200.Handle.TargetVersion,
	}, nil
}

// Rollback reverts to the previously installed binary.
func (s *UpdateService) Rollback(ctx context.Context) error {
	resp, err := s.c.rest.RollbackWithResponse(ctx)
	if err != nil {
		return err
	}
	if resp.JSON200 == nil {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

func updateStatusFromREST(m *rest.UpdateStatus) (*UpdateStatus, error) {
	phase, err := installPhaseFromREST(m.InstallPhase)
	if err != nil {
		return nil, err
	}
	return &UpdateStatus{
		CurrentVersion:    m.CurrentVersion,
		LatestVersion:     deref(m.LatestVersion),
		UpdateAvailable:   m.UpdateAvailable,
		AutoUpdateEnabled: m.AutoUpdateEnabled,
		Channel:           string(m.Channel),
		LastCheckAt:       m.LastCheckAt,
		LastInstallAt:     m.LastInstallAt,
		Phase:             phase,
		PendingVersion:    deref(m.PendingVersion),
		AppliedVersion:    deref(m.AppliedVersion),
		AppliedAt:         m.AppliedAt,
		RollbackAvailable: m.RollbackAvailable,
		EdgeAvailable:     m.EdgeAvailable,
	}, nil
}

// installPhaseFromREST bridges the generated union to the public phase.
func installPhaseFromREST(iph rest.InstallPhase) (InstallPhase, error) {
	return bridgeJSON[InstallPhase](iph)
}
