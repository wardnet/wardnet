package main

import (
	"fmt"
	"os"
	"text/tabwriter"

	"github.com/spf13/cobra"
	wardnet "wardnet.network/go"
)

func newDevicesCmd(c *cli) *cobra.Command {
	cmd := &cobra.Command{Use: "devices", Short: "Manage devices"}
	cmd.AddCommand(devicesListCmd(c), devicesShowCmd(c), devicesSetRuleCmd(c))
	return cmd
}

func devicesListCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List all devices",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			devs, err := client.Devices.List(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(devs)
			}
			if len(devs) == 0 {
				fmt.Println("no devices")
				return nil
			}
			tw := newTabWriter()
			fmt.Fprintln(tw, "ID\tNAME\tIP\tTYPE\tRULE")
			for i := range devs {
				d := &devs[i]
				fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\n",
					d.ID, deviceName(d), d.LastIP, d.Type, ruleString(d.Rule))
			}
			return tw.Flush()
		},
	}
}

func devicesShowCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "show <id>",
		Short: "Show details for a specific device",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			d, err := client.Devices.Get(cmd.Context(), args[0])
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(d)
			}
			fmt.Printf("id:           %s\n", d.ID)
			fmt.Printf("name:         %s\n", deviceName(d))
			fmt.Printf("hostname:     %s\n", orDash(d.Hostname))
			fmt.Printf("mac:          %s\n", d.MAC)
			fmt.Printf("manufacturer: %s\n", orDash(d.Manufacturer))
			fmt.Printf("type:         %s\n", d.Type)
			fmt.Printf("ip:           %s\n", d.LastIP)
			fmt.Printf("dhcp:         %s\n", d.DHCPStatus)
			fmt.Printf("rule:         %s\n", ruleString(d.Rule))
			fmt.Printf("admin locked: %t\n", d.AdminLocked)
			fmt.Printf("first seen:   %s\n", d.FirstSeen.Format("2006-01-02T15:04:05Z07:00"))
			fmt.Printf("last seen:    %s\n", d.LastSeen.Format("2006-01-02T15:04:05Z07:00"))
			return nil
		},
	}
}

func devicesSetRuleCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "set-rule <id> <target>",
		Short: "Set routing rule for a device (target: direct, default, or a tunnel ID)",
		Args:  cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			target := parseTarget(args[1])
			d, err := client.Devices.SetRule(cmd.Context(), args[0], target)
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(d)
			}
			fmt.Printf("device %s routing set to %s\n", d.ID, ruleString(d.Rule))
			return nil
		},
	}
}

// parseTarget maps a set-rule argument to a routing target. Anything that is
// not "direct" or "default" is treated as a tunnel ID, which the SDK validates.
func parseTarget(s string) wardnet.RoutingTarget {
	switch s {
	case "direct":
		return wardnet.RouteDirect()
	case "default":
		return wardnet.RouteDefault()
	default:
		return wardnet.RouteTunnel(s)
	}
}

func deviceName(d *wardnet.Device) string {
	if d.Name != "" {
		return d.Name
	}
	if d.Hostname != "" {
		return d.Hostname
	}
	return "-"
}

func newTabWriter() *tabwriter.Writer {
	return tabwriter.NewWriter(os.Stdout, 0, 2, 2, ' ', 0)
}
