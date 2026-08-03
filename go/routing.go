package wardnet

import (
	"encoding/json"
	"fmt"

	"wardnet.network/go/internal/rest"
)

// RoutingKind identifies which routing target a [RoutingTarget] holds.
type RoutingKind string

const (
	// RoutingDirect sends the device's traffic straight to the internet,
	// bypassing every tunnel.
	RoutingDirect RoutingKind = "direct"
	// RoutingDefault follows the gateway's default routing policy.
	RoutingDefault RoutingKind = "default"
	// RoutingTunnel routes the device through a specific tunnel (see
	// [RoutingTarget.TunnelID]).
	RoutingTunnel RoutingKind = "tunnel"
)

// RoutingTarget is where a device's traffic is routed. Build one with
// [RouteDirect], [RouteDefault], or [RouteTunnel]; inspect it via [RoutingTarget.Kind].
type RoutingTarget struct {
	// Kind is the variant this target holds.
	Kind RoutingKind
	// TunnelID is the target tunnel, set only when Kind is [RoutingTunnel].
	TunnelID string
}

// RouteDirect targets a direct (untunneled) internet path.
func RouteDirect() RoutingTarget { return RoutingTarget{Kind: RoutingDirect} }

// RouteDefault targets the gateway's default routing policy.
func RouteDefault() RoutingTarget { return RoutingTarget{Kind: RoutingDefault} }

// RouteTunnel targets the tunnel with the given ID.
func RouteTunnel(tunnelID string) RoutingTarget {
	return RoutingTarget{Kind: RoutingTunnel, TunnelID: tunnelID}
}

// String renders the target for display: "direct", "default", or
// "tunnel:<id>".
func (r RoutingTarget) String() string {
	if r.Kind == RoutingTunnel {
		return "tunnel:" + r.TunnelID
	}
	return string(r.Kind)
}

// MarshalJSON encodes the target in the daemon's internally-tagged form:
// {"type":"tunnel","tunnel_id":"…"}, {"type":"direct"}, or {"type":"default"}.
func (r RoutingTarget) MarshalJSON() ([]byte, error) {
	switch r.Kind {
	case RoutingTunnel:
		return json.Marshal(struct {
			Type     string `json:"type"`
			TunnelID string `json:"tunnel_id"`
		}{"tunnel", r.TunnelID})
	case RoutingDirect, RoutingDefault:
		return json.Marshal(struct {
			Type string `json:"type"`
		}{string(r.Kind)})
	default:
		return nil, fmt.Errorf("wardnet: invalid routing target kind %q", r.Kind)
	}
}

// UnmarshalJSON decodes the daemon's internally-tagged routing target form.
func (r *RoutingTarget) UnmarshalJSON(b []byte) error {
	var probe struct {
		Type     string `json:"type"`
		TunnelID string `json:"tunnel_id"`
	}
	if err := json.Unmarshal(b, &probe); err != nil {
		return err
	}
	switch probe.Type {
	case "tunnel":
		*r = RoutingTarget{Kind: RoutingTunnel, TunnelID: probe.TunnelID}
	case "direct":
		*r = RoutingTarget{Kind: RoutingDirect}
	case "default":
		*r = RoutingTarget{Kind: RoutingDefault}
	case "":
		return fmt.Errorf("wardnet: routing target missing type")
	default:
		// Preserve an unrecognised variant rather than failing: the routing
		// enum is meant to grow, and a new server-side variant must not break
		// decoding a whole device list. An unknown target can be displayed
		// (via Kind/String) but not re-sent — MarshalJSON rejects it.
		*r = RoutingTarget{Kind: RoutingKind(probe.Type), TunnelID: probe.TunnelID}
	}
	return nil
}

// validate reports whether the target is well-formed enough to send: a tunnel
// target must carry a valid UUID, matching the local validation applied to
// every other ID in the SDK.
func (r RoutingTarget) validate() error {
	if r.Kind == RoutingTunnel {
		if _, err := parseUUID(r.TunnelID, "tunnel"); err != nil {
			return err
		}
	}
	return nil
}

// toREST bridges the public target onto the generated union type.
func (r RoutingTarget) toREST() (rest.RoutingTarget, error) {
	return bridgeJSON[rest.RoutingTarget](r)
}

// routingTargetFromREST bridges the generated union type back to the public
// target. A nil input (no rule of the device's own) yields a nil result.
func routingTargetFromREST(irt *rest.RoutingTarget) (*RoutingTarget, error) {
	if irt == nil {
		return nil, nil
	}
	r, err := bridgeJSON[RoutingTarget](irt)
	if err != nil {
		return nil, err
	}
	return &r, nil
}
