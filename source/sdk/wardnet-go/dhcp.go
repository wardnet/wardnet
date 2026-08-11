package wardnet

import (
	"context"
	"time"

	"wardnet.network/go/internal/rest"
)

// DHCPService configures the DHCP server, its leases, and its reservations.
type DHCPService struct{ c *Client }

// DHCPConfig is the DHCP pool configuration. The gateway address is
// auto-detected by the daemon rather than configured.
type DHCPConfig struct {
	// Enabled reports whether the server should be running.
	Enabled bool `json:"enabled"`
	// GatewayIP is wardnet's own LAN address, advertised as gateway and DNS.
	GatewayIP string `json:"gateway_ip"`
	// RouterIP is the upstream router, offered as the secondary gateway.
	RouterIP string `json:"router_ip"`
	// PoolStart / PoolEnd bound the dynamic address range.
	PoolStart string `json:"pool_start"`
	PoolEnd   string `json:"pool_end"`
	// SubnetMask is the mask handed to clients.
	SubnetMask string `json:"subnet_mask"`
	// UpstreamDNS are the resolvers offered when wardnet's own is not used.
	UpstreamDNS []string `json:"upstream_dns"`
	// LeaseDurationSecs is how long an offered lease is valid.
	LeaseDurationSecs int32 `json:"lease_duration_secs"`
}

// DHCPServerStatus is the DHCP server's live state. It is named apart from
// [DHCPStatus], which describes how one device's address is managed.
type DHCPServerStatus struct {
	// Enabled is the configured intent; Running is the observed reality.
	Enabled bool `json:"enabled"`
	Running bool `json:"running"`
	// ActiveLeaseCount is how many leases are currently held.
	ActiveLeaseCount int64 `json:"active_lease_count"`
	// PoolUsed / PoolTotal describe address exhaustion.
	PoolUsed  int64 `json:"pool_used"`
	PoolTotal int64 `json:"pool_total"`
}

// Lease is an active or historical DHCP lease.
type Lease struct {
	// ID is the lease's UUID.
	ID string `json:"id"`
	// MAC is the client's hardware address.
	MAC string `json:"mac_address"`
	// IP is the leased address.
	IP string `json:"ip_address"`
	// Hostname is the name the client claimed, when it sent one.
	Hostname string `json:"hostname"`
	// DeviceID is the matching known device, when one is tracked.
	DeviceID string `json:"device_id"`
	// Status is "active", "expired", or "released".
	Status string `json:"status"`
	// LeaseStart / LeaseEnd bound the lease's validity.
	LeaseStart time.Time `json:"lease_start"`
	LeaseEnd   time.Time `json:"lease_end"`
}

// Reservation is a static MAC-to-IP binding.
type Reservation struct {
	// ID is the reservation's UUID.
	ID string `json:"id"`
	// MAC is the hardware address the address is pinned to.
	MAC string `json:"mac_address"`
	// IP is the reserved address.
	IP string `json:"ip_address"`
	// Hostname is an optional name to hand back to the client.
	Hostname string `json:"hostname"`
	// Description is an optional operator note.
	Description string `json:"description"`
	// CreatedAt is when the reservation was made.
	CreatedAt time.Time `json:"created_at"`
}

// CreateReservationParams describes a reservation to add.
type CreateReservationParams struct {
	// MAC is the hardware address to pin.
	MAC string
	// IP is the address to hand it.
	IP string
	// Hostname is an optional name for the client.
	Hostname string
	// Description is an optional operator note.
	Description string
}

// Config fetches the DHCP configuration.
func (s *DHCPService) Config(ctx context.Context) (*DHCPConfig, error) {
	resp, err := s.c.rest.DhcpGetConfigWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	cfg := dhcpConfigFromREST(&resp.JSON200.Config)
	return &cfg, nil
}

// SetEnabled starts or stops the DHCP server and returns the resulting config.
func (s *DHCPService) SetEnabled(ctx context.Context, enabled bool) (*DHCPConfig, error) {
	resp, err := s.c.rest.DhcpToggleWithResponse(ctx, rest.DhcpToggleJSONRequestBody{Enabled: enabled})
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	cfg := dhcpConfigFromREST(&resp.JSON200.Config)
	return &cfg, nil
}

// Status reports the DHCP server's live state.
func (s *DHCPService) Status(ctx context.Context) (*DHCPServerStatus, error) {
	resp, err := s.c.rest.DhcpStatusWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	m := resp.JSON200
	return &DHCPServerStatus{
		Enabled:          m.Enabled,
		Running:          m.Running,
		ActiveLeaseCount: m.ActiveLeaseCount,
		PoolUsed:         m.PoolUsed,
		PoolTotal:        m.PoolTotal,
	}, nil
}

// Leases returns every known lease.
func (s *DHCPService) Leases(ctx context.Context) ([]Lease, error) {
	resp, err := s.c.rest.ListLeasesWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]Lease, 0, len(resp.JSON200.Leases))
	for i := range resp.JSON200.Leases {
		l := &resp.JSON200.Leases[i]
		lease := Lease{
			ID:         l.Id.String(),
			MAC:        l.MacAddress,
			IP:         l.IpAddress,
			Hostname:   deref(l.Hostname),
			Status:     string(l.Status),
			LeaseStart: l.LeaseStart,
			LeaseEnd:   l.LeaseEnd,
		}
		if l.DeviceId != nil {
			lease.DeviceID = l.DeviceId.String()
		}
		out = append(out, lease)
	}
	return out, nil
}

// RevokeLease releases a lease, freeing its address for reuse.
func (s *DHCPService) RevokeLease(ctx context.Context, id string) error {
	lid, err := parseUUID(id, "lease")
	if err != nil {
		return err
	}
	resp, err := s.c.rest.RevokeLeaseWithResponse(ctx, lid)
	if err != nil {
		return err
	}
	if resp.JSON200 == nil {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

// Reservations returns every static MAC-to-IP binding.
func (s *DHCPService) Reservations(ctx context.Context) ([]Reservation, error) {
	resp, err := s.c.rest.ListReservationsWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]Reservation, 0, len(resp.JSON200.Reservations))
	for i := range resp.JSON200.Reservations {
		out = append(out, reservationFromREST(&resp.JSON200.Reservations[i]))
	}
	return out, nil
}

// AddReservation pins an address to a MAC.
func (s *DHCPService) AddReservation(ctx context.Context, params CreateReservationParams) (*Reservation, error) {
	body := rest.CreateReservationJSONRequestBody{
		MacAddress: params.MAC,
		IpAddress:  params.IP,
	}
	if params.Hostname != "" {
		body.Hostname = ptr(params.Hostname)
	}
	if params.Description != "" {
		body.Description = ptr(params.Description)
	}
	resp, err := s.c.rest.CreateReservationWithResponse(ctx, body)
	if err != nil {
		return nil, err
	}
	if resp.JSON201 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	r := reservationFromREST(&resp.JSON201.Reservation)
	return &r, nil
}

// RemoveReservation deletes a static MAC-to-IP binding.
func (s *DHCPService) RemoveReservation(ctx context.Context, id string) error {
	rid, err := parseUUID(id, "reservation")
	if err != nil {
		return err
	}
	resp, err := s.c.rest.DeleteReservationWithResponse(ctx, rid)
	if err != nil {
		return err
	}
	if resp.JSON200 == nil {
		return apiError(resp.HTTPResponse, resp.Body)
	}
	return nil
}

func dhcpConfigFromREST(c *rest.DhcpConfig) DHCPConfig {
	return DHCPConfig{
		Enabled:           c.Enabled,
		GatewayIP:         c.GatewayIp,
		RouterIP:          deref(c.RouterIp),
		PoolStart:         c.PoolStart,
		PoolEnd:           c.PoolEnd,
		SubnetMask:        c.SubnetMask,
		UpstreamDNS:       c.UpstreamDns,
		LeaseDurationSecs: c.LeaseDurationSecs,
	}
}

func reservationFromREST(r *rest.DhcpReservation) Reservation {
	return Reservation{
		ID:          r.Id.String(),
		MAC:         r.MacAddress,
		IP:          r.IpAddress,
		Hostname:    deref(r.Hostname),
		Description: deref(r.Description),
		CreatedAt:   r.CreatedAt,
	}
}
