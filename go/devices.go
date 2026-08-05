package wardnet

import (
	"context"
	"fmt"
	"time"

	"github.com/google/uuid"

	"wardnet.network/go/internal/rest"
)

// DevicesService lists discovered devices and manages their routing.
type DevicesService struct{ c *Client }

// DeviceType is a device's detected category (e.g. "Phone", "Laptop", "Tv").
type DeviceType string

// DHCPStatus describes how a device's IP is managed: "lease", "reservation",
// or "external".
type DHCPStatus string

// ConnectionMode is how a device is currently reachable: "lan" or "remote".
type ConnectionMode string

// Device is a discovered network device and its current routing state.
type Device struct {
	// ID is the device's UUID.
	ID string `json:"id"`
	// MAC is the device's hardware address.
	MAC string `json:"mac"`
	// Name is the admin-assigned display name, or "" if none.
	Name string `json:"name"`
	// Hostname is the device's reported hostname, or "" if unknown.
	Hostname string `json:"hostname"`
	// Manufacturer is the vendor name, or "" if unknown. Read it together with
	// ManufacturerSource: an empty value means the IEEE database has no usable
	// registrant (including the placeholder "Private" listings), not that the
	// lookup failed.
	Manufacturer string `json:"manufacturer"`
	// ManufacturerSource is where Manufacturer came from: "ieee" (the
	// registrant on record, a fact), "catalog" (Wardnet's curated mapping for a
	// privately-listed OUI, a hedged guess) or "signal" (inferred from what the
	// device announced). "" exactly when Manufacturer is "". See issue #1099.
	ManufacturerSource string `json:"manufacturer_source"`
	// IsRandomized reports whether MAC is locally administered (a privacy or
	// randomized address). Deliberately separate from Manufacturer: it says how
	// the device presents itself, not who built it.
	IsRandomized bool `json:"is_randomized"`
	// Type is the detected device category.
	Type DeviceType `json:"type"`
	// FirstSeen / LastSeen bound the device's observed presence.
	FirstSeen time.Time `json:"first_seen"`
	LastSeen  time.Time `json:"last_seen"`
	// LastIP is the most recently observed IP.
	LastIP string `json:"last_ip"`
	// AdminLocked reports whether routing changes are locked for this device.
	AdminLocked bool `json:"admin_locked"`
	// ZoneID is the Network Zone the device belongs to.
	ZoneID string `json:"zone_id"`
	// ConnectionMode is how the device is currently reachable.
	ConnectionMode ConnectionMode `json:"connection_mode"`
	// DHCPStatus reports how the device's IP is managed.
	DHCPStatus DHCPStatus `json:"dhcp_status"`
	// Rule is the device's own routing rule. Nil means it has no rule of its
	// own and follows the gateway default policy.
	Rule *RoutingTarget `json:"rule"`
}

// List returns every discovered device.
func (s *DevicesService) List(ctx context.Context) ([]Device, error) {
	resp, err := s.c.rest.ListDevicesWithResponse(ctx)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	out := make([]Device, 0, len(resp.JSON200.Devices))
	for i := range resp.JSON200.Devices {
		dev := &resp.JSON200.Devices[i]
		d, err := deviceFromREST(dev, dev.CurrentRule)
		if err != nil {
			return nil, err
		}
		out = append(out, *d)
	}
	return out, nil
}

// Get returns a single device by ID.
func (s *DevicesService) Get(ctx context.Context, id string) (*Device, error) {
	uid, err := parseUUID(id, "device")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.GetDeviceWithResponse(ctx, uid)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return deviceFromREST(&resp.JSON200.Device, resp.JSON200.CurrentRule)
}

// SetRule sets a device's routing target and returns the updated device.
//
// Only the routing target is sent; the daemon leaves the device's name, type,
// and admin-locked flag untouched (partial update).
func (s *DevicesService) SetRule(ctx context.Context, id string, target RoutingTarget) (*Device, error) {
	uid, err := parseUUID(id, "device")
	if err != nil {
		return nil, err
	}
	if err := target.validate(); err != nil {
		return nil, err
	}
	irt, err := target.toREST()
	if err != nil {
		return nil, err
	}
	body := rest.UpdateDeviceJSONRequestBody{RoutingTarget: &irt}
	resp, err := s.c.rest.UpdateDeviceWithResponse(ctx, uid, body)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return deviceFromREST(&resp.JSON200.Device, resp.JSON200.CurrentRule)
}

// deviceFromREST maps the generated device onto the public type. rule is the
// authoritative routing rule for the device — for the detail endpoints this is
// the response's top-level current_rule, not the mirror nested on the device.
func deviceFromREST(d *rest.DeviceWithStatus, rule *rest.RoutingTarget) (*Device, error) {
	r, err := routingTargetFromREST(rule)
	if err != nil {
		return nil, err
	}
	return &Device{
		ID:           d.Id.String(),
		MAC:          d.Mac,
		Name:         deref(d.Name),
		Hostname:     deref(d.Hostname),
		Manufacturer: deref(d.Manufacturer),
		ManufacturerSource: func() string {
			if d.ManufacturerSource == nil {
				return ""
			}
			return string(*d.ManufacturerSource)
		}(),
		IsRandomized:   d.IsRandomized,
		Type:           DeviceType(d.DeviceType),
		FirstSeen:      d.FirstSeen,
		LastSeen:       d.LastSeen,
		LastIP:         d.LastIp,
		AdminLocked:    d.AdminLocked,
		ZoneID:         d.ZoneId.String(),
		ConnectionMode: ConnectionMode(d.ConnectionMode),
		DHCPStatus:     DHCPStatus(d.DhcpStatus),
		Rule:           r,
	}, nil
}

// parseUUID parses an ID argument, wrapping failures in a clear message.
func parseUUID(id, kind string) (uuid.UUID, error) {
	u, err := uuid.Parse(id)
	if err != nil {
		return uuid.Nil, fmt.Errorf("wardnet: invalid %s id %q: %w", kind, id, err)
	}
	return u, nil
}
