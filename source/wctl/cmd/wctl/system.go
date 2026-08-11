package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

func newSystemCmd(c *cli) *cobra.Command {
	cmd := &cobra.Command{Use: "system", Short: "Inspect and control the daemon"}
	cmd.AddCommand(systemStatusCmd(c), systemErrorsCmd(c), systemRestartCmd(c))
	return cmd
}

func systemStatusCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show system status",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			return runStatus(cmd, c)
		},
	}
}

func systemErrorsCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "errors",
		Short: "List the daemon's recent diagnostics",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			diags, err := client.System.RecentErrors(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(diags)
			}
			if len(diags) == 0 {
				fmt.Println("no recent errors")
				return nil
			}
			tw := newTabWriter()
			fmt.Fprintln(tw, "TIME\tSEVERITY\tCOMPONENT\tCODE\tMESSAGE\tHINT")
			for _, d := range diags {
				fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\t%s\n",
					d.Timestamp.Format("2006-01-02T15:04:05Z07:00"),
					d.Severity, d.Component, d.Code, d.Message, orDash(d.Hint))
			}
			return tw.Flush()
		},
	}
}

func systemRestartCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "restart",
		Short: "Ask the daemon to exit so its supervisor restarts it",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			if err := client.System.Restart(cmd.Context()); err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(map[string]any{"restart": "scheduled"})
			}
			// The daemon answers before it exits, so it is briefly
			// unreachable after this returns.
			fmt.Println("restart scheduled")
			return nil
		},
	}
}
