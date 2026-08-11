package main

import (
	"fmt"

	"github.com/spf13/cobra"

	wardnet "wardnet.network/go"
)

func newAnomaliesCmd(c *cli) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "anomalies",
		Short: "Inspect the daemon's anomalies",
		Long: "Anomalies are typed conditions the daemon raises about itself — a blocklist " +
			"that keeps failing to refresh, a tunnel that will not come up. Each is opened " +
			"once when first observed and resolved when the condition stops holding.",
	}
	cmd.AddCommand(anomaliesListCmd(c), anomaliesReevaluateCmd(c))
	return cmd
}

func anomaliesListCmd(c *cli) *cobra.Command {
	var status string
	var subject string

	cmd := &cobra.Command{
		Use:   "list",
		Short: "List anomalies (open by default)",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			anomalies, err := client.Anomalies.List(cmd.Context(), wardnet.ListAnomaliesOptions{
				Status:    wardnet.AnomalyStatusFilter(status),
				SubjectID: subject,
			})
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(anomalies)
			}
			if len(anomalies) == 0 {
				fmt.Println("no anomalies")
				return nil
			}
			tw := newTabWriter()
			fmt.Fprintln(tw, "OPENED\tSEVERITY\tTYPE\tSUBJECT\tSEEN\tMESSAGE")
			for _, a := range anomalies {
				fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%d\t%s\n",
					a.OpenedAt.Format("2006-01-02T15:04:05Z07:00"),
					a.Severity, a.Type, orDash(a.SubjectID), a.Occurrences, a.Message)
			}
			return tw.Flush()
		},
	}

	cmd.Flags().StringVar(&status, "status", "",
		"which anomalies to list: open (default), resolved, or all")
	cmd.Flags().StringVar(&subject, "subject", "",
		"only anomalies about this entity (a tunnel or blocklist id)")
	return cmd
}

func anomaliesReevaluateCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "reevaluate",
		Short: "Re-check every open anomaly and close the ones that no longer hold",
		Long: "Runs the check the daemon performs on a schedule, now. Closing an anomaly " +
			"that was alerted also sends its recovery notification.",
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			result, err := client.Anomalies.Reevaluate(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(result)
			}
			fmt.Printf("evaluated %d, resolved %d\n", result.Evaluated, result.Resolved)
			return nil
		},
	}
}
