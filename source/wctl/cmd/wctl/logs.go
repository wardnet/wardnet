package main

import (
	"context"
	"fmt"
	"os"
	"sort"
	"strings"

	"github.com/spf13/cobra"
	wardnet "wardnet.network/go"
)

func newLogsCmd(c *cli) *cobra.Command {
	var follow bool
	var filter wardnet.LogFilter

	cmd := &cobra.Command{
		Use:   "logs",
		Short: "Show daemon logs",
		Long: "Without --follow, the daemon's stored log file is written to stdout.\n" +
			"With --follow, live entries are written to stderr until interrupted,\n" +
			"leaving stdout free for whatever the surrounding script is doing.",
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			if !follow && (filter.Level != "" || filter.Target != "") {
				return fmt.Errorf("--level and --target apply to --follow only")
			}
			client, err := c.client()
			if err != nil {
				return err
			}
			if follow {
				return followLogs(cmd.Context(), c, client, filter)
			}
			body, err := client.Logs.Download(cmd.Context())
			if err != nil {
				return err
			}
			_, err = os.Stdout.Write(body)
			return err
		},
	}
	cmd.Flags().BoolVarP(&follow, "follow", "f", false, "stream live entries until interrupted")
	cmd.Flags().StringVar(&filter.Level, "level", "",
		"minimum level to stream: trace, debug, info, warn, error (default: the daemon's, info)")
	cmd.Flags().StringVar(&filter.Target, "target", "",
		"stream only entries whose target starts with this prefix")
	return cmd
}

// followLogs streams entries to stderr until the daemon hangs up or the
// context is cancelled. A cancelled context is how SIGINT arrives, so it ends
// the stream without an error.
func followLogs(ctx context.Context, c *cli, client *wardnet.Client, filter wardnet.LogFilter) error {
	stream, err := client.Logs.Stream(ctx, filter)
	if err != nil {
		return err
	}
	defer func() { _ = stream.Close() }()

	for {
		ev, err := stream.Recv(ctx)
		if err != nil {
			if ctx.Err() != nil {
				return nil
			}
			return err
		}
		if ev.Entry == nil {
			fmt.Fprintf(os.Stderr, "-- %d entries dropped: this client fell behind --\n", ev.Skipped)
			continue
		}
		if c.jsonOut {
			// One object per line, so a consumer can parse the stream
			// incrementally instead of waiting for an array to close.
			if err := printJSONLine(os.Stderr, ev.Entry); err != nil {
				return err
			}
			continue
		}
		fmt.Fprintln(os.Stderr, formatLogEntry(ev.Entry))
	}
}

// formatLogEntry renders one entry as a single human-readable line, with the
// span context and structured fields trailing the message as key=value pairs.
func formatLogEntry(e *wardnet.LogEntry) string {
	var b strings.Builder
	fmt.Fprintf(&b, "%s %-5s %s: %s", e.Timestamp, e.Level, e.Target, e.Message)
	for _, kv := range append(sortedPairs(e.Span), sortedPairs(e.Fields)...) {
		b.WriteString(" " + kv)
	}
	return b.String()
}

// sortedPairs renders a field map as "key=value" pairs in a stable order, so
// successive lines line up rather than shuffling with Go's map iteration.
func sortedPairs(m map[string]string) []string {
	if len(m) == 0 {
		return nil
	}
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	pairs := make([]string, 0, len(keys))
	for _, k := range keys {
		pairs = append(pairs, k+"="+m[k])
	}
	return pairs
}
