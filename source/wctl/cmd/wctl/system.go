package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

func newSystemCmd(c *cli) *cobra.Command {
	cmd := &cobra.Command{Use: "system", Short: "Inspect and control the daemon"}
	cmd.AddCommand(systemStatusCmd(c), systemRestartCmd(c))
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
