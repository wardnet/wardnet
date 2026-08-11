package main

import (
	"fmt"
	"strings"

	"github.com/spf13/cobra"
	wardnet "wardnet.network/go"
)

func newDHCPCmd(c *cli) *cobra.Command {
	cmd := &cobra.Command{Use: "dhcp", Short: "Inspect and configure the DHCP server"}
	cmd.AddCommand(
		dhcpStatusCmd(c),
		dhcpConfigCmd(c),
		dhcpEnabledCmd(c, true),
		dhcpEnabledCmd(c, false),
		dhcpLeasesCmd(c),
		dhcpReservationsCmd(c),
	)
	return cmd
}

func dhcpStatusCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show DHCP server state and pool usage",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			st, err := client.DHCP.Status(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(st)
			}
			fmt.Printf("enabled:       %t\n", st.Enabled)
			fmt.Printf("running:       %t\n", st.Running)
			fmt.Printf("active leases: %d\n", st.ActiveLeaseCount)
			fmt.Printf("pool:          %d / %d addresses used\n", st.PoolUsed, st.PoolTotal)
			return nil
		},
	}
}

func dhcpConfigCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "config",
		Short: "Show the DHCP pool configuration",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			cfg, err := client.DHCP.Config(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(cfg)
			}
			fmt.Printf("enabled:        %t\n", cfg.Enabled)
			fmt.Printf("gateway:        %s\n", cfg.GatewayIP)
			fmt.Printf("router:         %s\n", orDash(cfg.RouterIP))
			fmt.Printf("pool:           %s - %s\n", cfg.PoolStart, cfg.PoolEnd)
			fmt.Printf("subnet mask:    %s\n", cfg.SubnetMask)
			fmt.Printf("upstream dns:   %s\n", orDash(strings.Join(cfg.UpstreamDNS, ", ")))
			fmt.Printf("lease duration: %s\n", humanDuration(int64(cfg.LeaseDurationSecs)))
			return nil
		},
	}
}

func dhcpEnabledCmd(c *cli, enabled bool) *cobra.Command {
	use, short := "enable", "Start the DHCP server"
	if !enabled {
		use, short = "disable", "Stop the DHCP server"
	}
	return &cobra.Command{
		Use:   use,
		Short: short,
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			cfg, err := client.DHCP.SetEnabled(cmd.Context(), enabled)
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(cfg)
			}
			fmt.Printf("dhcp enabled: %t\n", cfg.Enabled)
			return nil
		},
	}
}

func dhcpLeasesCmd(c *cli) *cobra.Command {
	cmd := &cobra.Command{Use: "leases", Short: "Inspect DHCP leases"}
	cmd.AddCommand(leasesListCmd(c), leasesRevokeCmd(c))
	return cmd
}

func leasesListCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List DHCP leases",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			leases, err := client.DHCP.Leases(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(leases)
			}
			if len(leases) == 0 {
				fmt.Println("no leases")
				return nil
			}
			tw := newTabWriter()
			fmt.Fprintln(tw, "ID\tMAC\tIP\tHOSTNAME\tSTATUS\tEXPIRES")
			for _, l := range leases {
				fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\t%s\n",
					l.ID, l.MAC, l.IP, orDash(l.Hostname), l.Status,
					l.LeaseEnd.Format("2006-01-02T15:04:05Z07:00"))
			}
			return tw.Flush()
		},
	}
}

func leasesRevokeCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "revoke <id>",
		Short: "Release a lease so its address returns to the pool",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			if err := client.DHCP.RevokeLease(cmd.Context(), args[0]); err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(map[string]any{"revoked": args[0]})
			}
			fmt.Printf("revoked lease %s\n", args[0])
			return nil
		},
	}
}

func dhcpReservationsCmd(c *cli) *cobra.Command {
	cmd := &cobra.Command{Use: "reservations", Short: "Manage static MAC-to-IP reservations"}
	cmd.AddCommand(reservationsListCmd(c), reservationsAddCmd(c), reservationsRemoveCmd(c))
	return cmd
}

func reservationsListCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List static reservations",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			reservations, err := client.DHCP.Reservations(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(reservations)
			}
			if len(reservations) == 0 {
				fmt.Println("no reservations")
				return nil
			}
			tw := newTabWriter()
			fmt.Fprintln(tw, "ID\tMAC\tIP\tHOSTNAME\tDESCRIPTION")
			for _, r := range reservations {
				fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\n",
					r.ID, r.MAC, r.IP, orDash(r.Hostname), orDash(r.Description))
			}
			return tw.Flush()
		},
	}
}

func reservationsAddCmd(c *cli) *cobra.Command {
	var hostname, description string
	cmd := &cobra.Command{
		Use:   "add <mac> <ip>",
		Short: "Pin an address to a MAC",
		Args:  cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			r, err := client.DHCP.AddReservation(cmd.Context(), wardnet.CreateReservationParams{
				MAC:         args[0],
				IP:          args[1],
				Hostname:    hostname,
				Description: description,
			})
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(r)
			}
			fmt.Printf("reserved %s for %s (%s)\n", r.IP, r.MAC, r.ID)
			return nil
		},
	}
	cmd.Flags().StringVar(&hostname, "hostname", "", "hostname to hand back to the client")
	cmd.Flags().StringVar(&description, "description", "", "note describing the reservation")
	return cmd
}

func reservationsRemoveCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "remove <id>",
		Short: "Delete a static reservation",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			if err := client.DHCP.RemoveReservation(cmd.Context(), args[0]); err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(map[string]any{"removed": args[0]})
			}
			fmt.Printf("removed reservation %s\n", args[0])
			return nil
		},
	}
}
