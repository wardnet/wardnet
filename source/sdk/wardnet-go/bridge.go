package wardnet

import "encoding/json"

// bridgeJSON converts one internally-tagged union representation into another
// by round-tripping through their shared wire JSON. The public sum types
// (RoutingTarget, InstallPhase, BackupStatus) and the generated
// json.RawMessage unions encode to identical JSON, so this bridge is exact —
// and order-independent, unlike the generated positional As*/From* helpers,
// which couple to the oneOf ordering in the spec.
func bridgeJSON[T any](src json.Marshaler) (T, error) {
	var dst T
	raw, err := src.MarshalJSON()
	if err != nil {
		return dst, err
	}
	if err := json.Unmarshal(raw, &dst); err != nil {
		return dst, err
	}
	return dst, nil
}
