package main

import (
	"encoding/json"
	"fmt"
	"io"
	"strings"
	"time"

	wardnet "wardnet.network/go"
)

// printJSON writes v as indented JSON on stdout.
func printJSON(v any) error {
	b, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return err
	}
	fmt.Println(string(b))
	return nil
}

// printJSONLine writes v to w as a single line of JSON. Streaming output uses
// it so each record is complete on its own line.
func printJSONLine(w io.Writer, v any) error {
	b, err := json.Marshal(v)
	if err != nil {
		return err
	}
	_, err = fmt.Fprintln(w, string(b))
	return err
}

// orDash renders s, or "-" when empty.
func orDash(s string) string {
	if s == "" {
		return "-"
	}
	return s
}

// timeOrDash renders an optional timestamp as RFC 3339, or "-".
func timeOrDash(t *time.Time) string {
	if t == nil {
		return "-"
	}
	return t.Format(time.RFC3339)
}

// ruleString renders a device's routing rule for display. A nil rule means the
// device follows the gateway default policy.
func ruleString(r *wardnet.RoutingTarget) string {
	if r == nil {
		return "(default)"
	}
	return r.String()
}

// humanBytes renders a byte count with a binary unit suffix.
func humanBytes(n int64) string {
	const unit = 1024
	if n < unit {
		return fmt.Sprintf("%d B", n)
	}
	div, exp := int64(unit), 0
	for x := n / unit; x >= unit; x /= unit {
		div *= unit
		exp++
	}
	units := []string{"KiB", "MiB", "GiB", "TiB", "PiB"}
	return fmt.Sprintf("%.1f %s", float64(n)/float64(div), units[exp])
}

// humanDuration renders a whole number of seconds as "1d 2h 3m 4s".
func humanDuration(sec int64) string {
	if sec <= 0 {
		return "0s"
	}
	days := sec / 86400
	sec %= 86400
	hours := sec / 3600
	sec %= 3600
	mins := sec / 60
	sec %= 60

	var parts []string
	if days > 0 {
		parts = append(parts, fmt.Sprintf("%dd", days))
	}
	if hours > 0 {
		parts = append(parts, fmt.Sprintf("%dh", hours))
	}
	if mins > 0 {
		parts = append(parts, fmt.Sprintf("%dm", mins))
	}
	if sec > 0 {
		parts = append(parts, fmt.Sprintf("%ds", sec))
	}
	return strings.Join(parts, " ")
}
