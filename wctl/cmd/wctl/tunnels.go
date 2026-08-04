package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
	wardnet "wardnet.network/go"
)

func newTunnelsCmd(c *cli) *cobra.Command {
	cmd := &cobra.Command{Use: "tunnels", Short: "Manage tunnels"}
	cmd.AddCommand(
		tunnelsListCmd(c),
		tunnelsShowCmd(c),
		tunnelsAddCmd(c),
		tunnelsRemoveCmd(c),
		tunnelsTestCmd(c),
	)
	return cmd
}

func tunnelsListCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List all tunnels",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			tuns, err := client.Tunnels.List(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(tuns)
			}
			if len(tuns) == 0 {
				fmt.Println("no tunnels")
				return nil
			}
			tw := newTabWriter()
			fmt.Fprintln(tw, "ID\tLABEL\tCOUNTRY\tINTERFACE\tSTATUS\tRX/TX")
			for i := range tuns {
				t := &tuns[i]
				fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\t%s / %s\n",
					t.ID, t.Label, t.CountryCode, t.InterfaceName, t.Status,
					humanBytes(t.BytesRx), humanBytes(t.BytesTx))
			}
			return tw.Flush()
		},
	}
}

func tunnelsShowCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "show <id>",
		Short: "Show details for a specific tunnel",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			t, err := client.Tunnels.Get(cmd.Context(), args[0])
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(t)
			}
			fmt.Printf("id:             %s\n", t.ID)
			fmt.Printf("label:          %s\n", t.Label)
			fmt.Printf("country:        %s\n", t.CountryCode)
			fmt.Printf("provider:       %s\n", orDash(t.Provider))
			fmt.Printf("interface:      %s\n", t.InterfaceName)
			fmt.Printf("endpoint:       %s\n", t.Endpoint)
			fmt.Printf("status:         %s\n", t.Status)
			fmt.Printf("last handshake: %s\n", timeOrDash(t.LastHandshake))
			fmt.Printf("traffic:        %s rx / %s tx\n", humanBytes(t.BytesRx), humanBytes(t.BytesTx))
			fmt.Printf("dns override:   %t\n", t.OverrideDefaultDNS)
			fmt.Printf("created:        %s\n", t.CreatedAt.Format("2006-01-02T15:04:05Z07:00"))
			return nil
		},
	}
}

func tunnelsAddCmd(c *cli) *cobra.Command {
	var label, country, conf string
	cmd := &cobra.Command{
		Use:   "add",
		Short: "Add a new tunnel by importing a WireGuard .conf file",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			data, err := os.ReadFile(conf)
			if err != nil {
				return fmt.Errorf("cannot read WireGuard config %s: %w", conf, err)
			}
			client, err := c.client()
			if err != nil {
				return err
			}
			t, err := client.Tunnels.Add(cmd.Context(), wardnet.CreateTunnelParams{
				Label:       label,
				CountryCode: country,
				Config:      string(data),
			})
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(t)
			}
			fmt.Printf("created tunnel %s (%s) on interface %s\n", t.ID, t.Label, t.InterfaceName)
			return nil
		},
	}
	cmd.Flags().StringVar(&label, "label", "", "tunnel label")
	cmd.Flags().StringVar(&country, "country", "", "country code (e.g. US, DE)")
	cmd.Flags().StringVar(&conf, "conf", "", "path to the WireGuard .conf file to import")
	_ = cmd.MarkFlagRequired("label")
	_ = cmd.MarkFlagRequired("country")
	_ = cmd.MarkFlagRequired("conf")
	return cmd
}

func tunnelsRemoveCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "remove <id>",
		Short: "Remove a tunnel",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			if err := client.Tunnels.Remove(cmd.Context(), args[0]); err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(map[string]any{"id": args[0], "removed": true})
			}
			fmt.Printf("tunnel %s removed\n", args[0])
			return nil
		},
	}
}

func tunnelsTestCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "test <id>",
		Short: "Probe a tunnel for its exit IP, country, and latency",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			r, err := client.Tunnels.Test(cmd.Context(), args[0])
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(r)
			}
			fmt.Printf("exit ip:  %s\n", r.ExitIP)
			fmt.Printf("country:  %s\n", r.CountryCode)
			fmt.Printf("latency:  %d ms\n", r.LatencyMs)
			return nil
		},
	}
}
