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
	// OwnerUserID is the household user this device belongs to, empty when
	// unassigned.
	//
	// Attribution, never authority (ADR-0031 §4): it answers "whose iPad is
	// this?" and grants nothing. A device caller resolves to the device
	// principal whatever its owner's role is, so never treat this as evidence
	// that the caller is that user.
	OwnerUserID string `json:"owner_user_id"`
	// ConnectionMode is how the device is currently reachable.
	ConnectionMode ConnectionMode `json:"connection_mode"`
	// DHCPStatus reports how the device's IP is managed.
	DHCPStatus DHCPStatus `json:"dhcp_status"`
	// Rule is the device's own routing rule. Nil means it has no rule of its
	// own and follows the gateway default policy.
	Rule *RoutingTarget `json:"rule"`
	// Managed reports whether an admin has decided to control this device's
	// configuration (issue #1181).
	//
	// Set by any admin configuration act — naming, locking, a routing rule or
	// profile, DNS filter settings, DNS capture, a Private-DNS grant, a Remote
	// peer credential, a DHCP reservation, a zone exception, an explicit zone
	// reassignment. Deliberately not derived from Name: a device can be
	// configured without ever being named.
	//
	// Latching: it never clears on its own, only through Release. Only
	// unmanaged devices are subject to device retention (deleted after 30 days
	// away).
	Managed bool `json:"managed"`
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

// SetOwner assigns or clears the household user a device belongs to (ADR-0031
// §4). Pass an empty ownerUserID to clear the assignment.
//
// Attribution, never authentication. The owner's role has no effect on what
// the device may do: a device caller resolves to the Device principal whoever
// owns it, including an admin. Device identity is derived from the source IP,
// so treating ownership as a credential would collapse admin access to IP
// spoofing.
func (s *DevicesService) SetOwner(ctx context.Context, id, ownerUserID string) (*Device, error) {
	uid, err := parseUUID(id, "device")
	if err != nil {
		return nil, err
	}
	var body rest.SetDeviceOwnerJSONRequestBody
	if ownerUserID != "" {
		owner, err := parseUUID(ownerUserID, "user")
		if err != nil {
			return nil, err
		}
		body.OwnerUserId = &owner
	}
	resp, err := s.c.rest.SetDeviceOwnerWithResponse(ctx, uid, body)
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return deviceFromREST(&resp.JSON200.Device, resp.JSON200.CurrentRule)
}

// Release stops managing a device: it reverts every admin-set configuration to
// default and returns the device to unmanaged (issue #1181).
//
// Destructive. It revokes the device's Private-DNS grant and its Remote peer
// credential, disconnecting it, and clears the device's name, admin lock, DNS
// capture, routing rule and profiles, DNS-filter settings, DHCP reservation,
// and zone exceptions, returning it to the default-for-new zone.
//
// Once unmanaged the device becomes subject to device retention and is deleted
// after 30 days away. Idempotent: a retry after a partial failure completes,
// and a failure part-way leaves the device still managed rather than
// half-released.
func (s *DevicesService) Release(ctx context.Context, id string) (*Device, error) {
	uid, err := parseUUID(id, "device")
	if err != nil {
		return nil, err
	}
	resp, err := s.c.rest.ReleaseDeviceWithResponse(ctx, uid)
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
		OwnerUserID:    ownerUserID(d.OwnerUserId),
		ConnectionMode: ConnectionMode(d.ConnectionMode),
		DHCPStatus:     DHCPStatus(d.DhcpStatus),
		Rule:           r,
		Managed:        d.Managed,
	}, nil
}

// ownerUserID renders an optional owner id, using the empty string for
// "unassigned" so callers need not deal with a nil pointer for the common case.
// `openapi_types.UUID` is a true alias of `uuid.UUID`, so no extra import.
func ownerUserID(id *uuid.UUID) string {
	if id == nil {
		return ""
	}
	return id.String()
}

// parseUUID parses an ID argument, wrapping failures in a clear message.
func parseUUID(id, kind string) (uuid.UUID, error) {
	u, err := uuid.Parse(id)
	if err != nil {
		return uuid.Nil, fmt.Errorf("wardnet: invalid %s id %q: %w", kind, id, err)
	}
	return u, nil
}
