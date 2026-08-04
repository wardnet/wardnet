package wardnet

import (
	"context"
	"time"

	"wardnet.network/go/internal/rest"
)

// TunnelsService manages WireGuard tunnels.
type TunnelsService struct{ c *Client }

// TunnelStatus is a tunnel's live state: "up", "down", "connecting", or
// "reconnecting".
type TunnelStatus string

// Tunnel is a configured WireGuard tunnel and its runtime state.
type Tunnel struct {
	// ID is the tunnel's UUID.
	ID string `json:"id"`
	// Label is the human-readable name.
	Label string `json:"label"`
	// CountryCode is the exit country (ISO-3166 alpha-2).
	CountryCode string `json:"country_code"`
	// Provider is the VPN provider that supplied the tunnel, or "" for a
	// manually imported one.
	Provider string `json:"provider"`
	// InterfaceName is the WireGuard interface the daemon assigned.
	InterfaceName string `json:"interface_name"`
	// Endpoint is the peer endpoint (host:port).
	Endpoint string `json:"endpoint"`
	// Status is the tunnel's live state.
	Status TunnelStatus `json:"status"`
	// LastHandshake is the most recent WireGuard handshake, if any.
	LastHandshake *time.Time `json:"last_handshake"`
	// BytesRx / BytesTx are the interface byte counters.
	BytesRx int64 `json:"bytes_rx"`
	BytesTx int64 `json:"bytes_tx"`
	// CreatedAt is when the tunnel was imported.
	CreatedAt time.Time `json:"created_at"`
	// OverrideDefaultDNS reports whether tunneled devices resolve DNS through
	// wardnet.
	OverrideDefaultDNS bool `json:"override_default_dns"`
}

// TunnelTestResult is the outcome of probing a tunnel's exit.
type TunnelTestResult struct {
	// TunnelID is the probed tunnel.
	TunnelID string `json:"tunnel_id"`
	// ExitIP is the public IP observed at the tunnel exit.
	ExitIP string `json:"exit_ip"`
	// CountryCode is the exit country reported by the probe (ISO-3166 alpha-2).
	CountryCode string `json:"country_code"`
	// LatencyMs is the probe round-trip latency in milliseconds.
	LatencyMs int64 `json:"latency_ms"`
}

// CreateTunnelParams describes a tunnel to import.
type CreateTunnelParams struct {
	// Label is the human-readable name.
	Label string
	// CountryCode is the exit country (ISO-3166 alpha-2).
	CountryCode string
	// Config is the WireGuard .conf file contents to import.
	Config string
	// Provider optionally records the VPN provider the config came from.
	Provider string
}

// List returns every configured tunnel.
func (s *TunnelsService) List(ctx context.Context) ([]Tunnel, error) {
	resp, err := s.c.rest.ListTunnelsWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]Tunnel, 0, len(resp.JSON200.Tunnels))
	for i := range resp.JSON200.Tunnels {
		out = append(out, tunnelFromREST(&resp.JSON200.Tunnels[i]))
	}
	return out, nil
}

// Get returns a single tunnel by ID.
func (s *TunnelsService) Get(ctx context.Context, id string) (*Tunnel, error) {
	uid, err := parseUUID(id, "tunnel")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.GetTunnelWithResponse(ctx, uid)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	t := tunnelFromREST(&resp.JSON200.Tunnel)
	return &t, nil
}

// Add imports a tunnel from a WireGuard config and returns the created tunnel.
func (s *TunnelsService) Add(ctx context.Context, params CreateTunnelParams) (*Tunnel, error) {
	body := rest.CreateTunnelJSONRequestBody{
		Label:       params.Label,
		CountryCode: params.CountryCode,
		Config:      params.Config,
	}
	if params.Provider != "" {
		body.Provider = ptr(params.Provider)
	}
	resp, err := s.c.rest.CreateTunnelWithResponse(ctx, body)
	if err != nil {
		return nil, err
	}
	if resp.JSON201 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	t := tunnelFromREST(&resp.JSON201.Tunnel)
	return &t, nil
}

// Remove deletes a tunnel by ID.
func (s *TunnelsService) Remove(ctx context.Context, id string) error {
	uid, err := parseUUID(id, "tunnel")
	if err != nil {
		return err
	}
	resp, err := s.c.rest.DeleteTunnelWithResponse(ctx, uid)
	if err != nil {
		return err
	}
	if resp.JSON200 == nil {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

// Test probes a tunnel and reports its exit IP, country, and latency.
func (s *TunnelsService) Test(ctx context.Context, id string) (*TunnelTestResult, error) {
	uid, err := parseUUID(id, "tunnel")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.TestTunnelWithResponse(ctx, uid)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	r := resp.JSON200.Result
	return &TunnelTestResult{
		TunnelID:    r.TunnelId.String(),
		ExitIP:      r.ExitIp,
		CountryCode: r.CountryCode,
		LatencyMs:   r.LatencyMs,
	}, nil
}

func tunnelFromREST(t *rest.Tunnel) Tunnel {
	return Tunnel{
		ID:                 t.Id.String(),
		Label:              t.Label,
		CountryCode:        t.CountryCode,
		Provider:           deref(t.Provider),
		InterfaceName:      t.InterfaceName,
		Endpoint:           t.Endpoint,
		Status:             TunnelStatus(t.Status),
		LastHandshake:      t.LastHandshake,
		BytesRx:            t.BytesRx,
		BytesTx:            t.BytesTx,
		CreatedAt:          t.CreatedAt,
		OverrideDefaultDNS: t.OverrideDefaultDns,
	}
}
