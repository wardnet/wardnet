package main

import (
	"fmt"

	"github.com/spf13/cobra"
	wardnet "wardnet.network/go"
)

func newUpdateCmd(c *cli) *cobra.Command {
	cmd := &cobra.Command{Use: "update", Short: "Manage the daemon's auto-update"}
	cmd.AddCommand(
		updateStatusCmd(c),
		updateCheckCmd(c),
		updateInstallCmd(c),
		updateRollbackCmd(c),
	)
	return cmd
}

func printUpdateStatus(s *wardnet.UpdateStatus) {
	fmt.Printf("current version:  %s\n", s.CurrentVersion)
	fmt.Printf("latest version:   %s\n", orDash(s.LatestVersion))
	fmt.Printf("update available: %t\n", s.UpdateAvailable)
	fmt.Printf("channel:          %s\n", s.Channel)
	fmt.Printf("auto-update:      %t\n", s.AutoUpdateEnabled)
	fmt.Printf("phase:            %s\n", phaseString(s.Phase))
	fmt.Printf("pending version:  %s\n", orDash(s.PendingVersion))
	fmt.Printf("rollback ready:   %t\n", s.RollbackAvailable)
	fmt.Printf("last check:       %s\n", timeOrDash(s.LastCheckAt))
	fmt.Printf("last install:     %s\n", timeOrDash(s.LastInstallAt))
}

func phaseString(p wardnet.InstallPhase) string {
	switch p.Kind {
	case wardnet.InstallDownloading:
		if p.Total != nil {
			return fmt.Sprintf("downloading (%s / %s)", humanBytes(p.Bytes), humanBytes(*p.Total))
		}
		return fmt.Sprintf("downloading (%s)", humanBytes(p.Bytes))
	case wardnet.InstallFailed:
		return "failed: " + p.Reason
	default:
		return string(p.Kind)
	}
}

func updateStatusCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show auto-update status",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			s, err := client.Update.Status(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(s)
			}
			printUpdateStatus(s)
			return nil
		},
	}
}

func updateCheckCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "check",
		Short: "Force a manifest refresh against the active channel",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			s, err := client.Update.Check(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(s)
			}
			printUpdateStatus(s)
			return nil
		},
	}
}

func updateInstallCmd(c *cli) *cobra.Command {
	var version string
	cmd := &cobra.Command{
		Use:   "install",
		Short: "Install the latest known release (or a specific version)",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			handle, err := client.Update.Install(cmd.Context(), version)
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(handle)
			}
			fmt.Printf("install %s started, targeting %s\n", handle.InstallID, handle.TargetVersion)
			return nil
		},
	}
	cmd.Flags().StringVar(&version, "version", "", "specific version to install (default: channel latest)")
	return cmd
}

func updateRollbackCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "rollback",
		Short: "Roll back to the previously installed binary",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			if err := client.Update.Rollback(cmd.Context()); err != nil {
				return err
			}
			fmt.Println("rollback initiated")
			return nil
		},
	}
}
