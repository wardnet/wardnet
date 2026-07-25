package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

func newStatusCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show system status",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			st, err := client.System.Status(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(st)
			}
			fmt.Printf("version:       %s\n", st.ReleaseVersion)
			fmt.Printf("build:         %s\n", st.Version)
			fmt.Printf("uptime:        %s\n", humanDuration(st.UptimeSeconds))
			fmt.Printf("devices:       %d\n", st.DeviceCount)
			fmt.Printf("tunnels:       %d (%d active)\n", st.TunnelCount, st.TunnelActiveCount)
			fmt.Printf("cpu:           %.1f%%\n", st.CPUUsagePercent)
			fmt.Printf("memory:        %s / %s\n", humanBytes(st.MemoryUsedBytes), humanBytes(st.MemoryTotalBytes))
			fmt.Printf("disk free:     %s / %s\n", humanBytes(st.DiskFreeBytes), humanBytes(st.DiskTotalBytes))
			fmt.Printf("database:      %s\n", humanBytes(st.DBSizeBytes))
			fmt.Printf("last shutdown: %s (%s)\n", st.LastShutdown.State, timeOrDash(st.LastShutdown.At))
			return nil
		},
	}
}
