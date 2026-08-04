package wardnet_test

import (
	"encoding/json"
	"testing"

	wardnet "wardnet.network/go"
)

func TestRoutingTargetJSON(t *testing.T) {
	cases := []struct {
		name   string
		target wardnet.RoutingTarget
		wire   string
	}{
		{"direct", wardnet.RouteDirect(), `{"type":"direct"}`},
		{"default", wardnet.RouteDefault(), `{"type":"default"}`},
		{"tunnel", wardnet.RouteTunnel("d4681478-8a90-4ce6-b220-0650a333d73c"),
			`{"type":"tunnel","tunnel_id":"d4681478-8a90-4ce6-b220-0650a333d73c"}`},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, err := json.Marshal(tc.target)
			if err != nil {
				t.Fatalf("marshal: %v", err)
			}
			if string(got) != tc.wire {
				t.Errorf("marshal = %s, want %s", got, tc.wire)
			}
			var rt wardnet.RoutingTarget
			if err := json.Unmarshal([]byte(tc.wire), &rt); err != nil {
				t.Fatalf("unmarshal: %v", err)
			}
			if rt != tc.target {
				t.Errorf("unmarshal = %+v, want %+v", rt, tc.target)
			}
		})
	}
}

func TestRoutingTargetString(t *testing.T) {
	if got := wardnet.RouteTunnel("x").String(); got != "tunnel:x" {
		t.Errorf("tunnel String() = %q", got)
	}
	if got := wardnet.RouteDirect().String(); got != "direct" {
		t.Errorf("direct String() = %q", got)
	}
	if got := wardnet.RouteDefault().String(); got != "default" {
		t.Errorf("default String() = %q", got)
	}
}

func TestRoutingTargetUnmarshalUnknownPreserved(t *testing.T) {
	// An unrecognised variant must decode (forward-compat) rather than fail a
	// whole device list...
	var rt wardnet.RoutingTarget
	if err := json.Unmarshal([]byte(`{"type":"future"}`), &rt); err != nil {
		t.Fatalf("unknown variant should decode, got %v", err)
	}
	if rt.Kind != "future" {
		t.Errorf("Kind = %q, want future", rt.Kind)
	}
	// ...but it cannot be re-sent, since the SDK doesn't know its shape.
	if _, err := json.Marshal(rt); err == nil {
		t.Error("marshalling an unknown routing target should error")
	}
}

func TestRoutingTargetUnmarshalMissingType(t *testing.T) {
	var rt wardnet.RoutingTarget
	if err := json.Unmarshal([]byte(`{}`), &rt); err == nil {
		t.Error("expected error for a routing target with no type")
	}
}
