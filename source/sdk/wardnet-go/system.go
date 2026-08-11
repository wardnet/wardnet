package wardnet

import (
	"context"
	"net/http"
	"time"
)

// SystemService reports daemon health and runtime status.
type SystemService struct{ c *Client }

// SystemStatus is a point-in-time snapshot of the daemon and its host.
type SystemStatus struct {
	// Version is the diagnostic, git-derived build version.
	Version string `json:"version"`
	// ReleaseVersion is the public-facing CalVer release.
	ReleaseVersion string `json:"release_version"`
	// UptimeSeconds is how long the daemon has been running.
	UptimeSeconds int64 `json:"uptime_seconds"`
	// DeviceCount is the number of known devices.
	DeviceCount int64 `json:"device_count"`
	// TunnelCount is the number of configured tunnels.
	TunnelCount int64 `json:"tunnel_count"`
	// TunnelActiveCount is how many tunnels are currently up.
	TunnelActiveCount int64 `json:"tunnel_active_count"`
	// DBSizeBytes is the on-disk size of the daemon database.
	DBSizeBytes int64 `json:"db_size_bytes"`
	// CPUUsagePercent is the host CPU utilisation.
	CPUUsagePercent float32 `json:"cpu_usage_percent"`
	// MemoryUsedBytes / MemoryTotalBytes describe host memory.
	MemoryUsedBytes  int64 `json:"memory_used_bytes"`
	MemoryTotalBytes int64 `json:"memory_total_bytes"`
	// DiskFreeBytes / DiskTotalBytes describe the data volume.
	DiskFreeBytes  int64 `json:"disk_free_bytes"`
	DiskTotalBytes int64 `json:"disk_total_bytes"`
	// LastShutdown classifies how the previous run ended.
	LastShutdown LastShutdown `json:"last_shutdown"`
}

// LastShutdown describes how the daemon's previous run ended.
type LastShutdown struct {
	// State is "unknown", "graceful", or "unclean".
	State string `json:"state"`
	// At is when the previous run last touched the database. Nil on a
	// first-ever boot.
	At *time.Time `json:"at"`
	// AcknowledgedAt is when an admin dismissed the unclean-shutdown banner,
	// if ever.
	AcknowledgedAt *time.Time `json:"acknowledged_at"`
}


// Status fetches the current system status.
func (s *SystemService) Status(ctx context.Context) (*SystemStatus, error) {
	resp, err := s.c.rest.SystemStatusWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	m := resp.JSON200
	return &SystemStatus{
		Version:           m.Version,
		ReleaseVersion:    m.ReleaseVersion,
		UptimeSeconds:     m.UptimeSeconds,
		DeviceCount:       m.DeviceCount,
		TunnelCount:       m.TunnelCount,
		TunnelActiveCount: m.TunnelActiveCount,
		DBSizeBytes:       m.DbSizeBytes,
		CPUUsagePercent:   m.CpuUsagePercent,
		MemoryUsedBytes:   m.MemoryUsedBytes,
		MemoryTotalBytes:  m.MemoryTotalBytes,
		DiskFreeBytes:     m.DiskFreeBytes,
		DiskTotalBytes:    m.DiskTotalBytes,
		LastShutdown: LastShutdown{
			State:          string(m.LastShutdown.State),
			At:             m.LastShutdown.At,
			AcknowledgedAt: m.LastShutdown.AcknowledgedAt,
		},
	}, nil
}


// Restart asks the daemon to exit so its supervisor starts it again. It
// returns as soon as the restart is scheduled; the daemon is unreachable for
// a few seconds afterwards.
func (s *SystemService) Restart(ctx context.Context) error {
	resp, err := s.c.rest.RestartWithResponse(ctx)
	if err != nil {
		return err
	}
	if resp.HTTPResponse == nil || resp.HTTPResponse.StatusCode != http.StatusNoContent {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}
