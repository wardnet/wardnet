package main

import (
	"context"
	"fmt"
	"strings"

	"github.com/spf13/cobra"
	wardnet "wardnet.network/go"
)

// defaultFilterProfile is the builtin profile the filtering subcommands act on
// when --profile is not given. It is the one the admin UI's Ad Blocking page
// shows, so `wctl dns blocklists list` matches that page by default.
const defaultFilterProfile = "Ad Blocking"

// dnsCLI carries the shared global flags plus the profile selector that every
// filtering subcommand needs.
type dnsCLI struct {
	*cli
	profile string
}

func newDNSCmd(c *cli) *cobra.Command {
	d := &dnsCLI{cli: c}
	cmd := &cobra.Command{Use: "dns", Short: "Inspect and configure the DNS resolver"}
	cmd.PersistentFlags().StringVar(&d.profile, "profile", defaultFilterProfile,
		"filter profile the blocklist, allowlist, and rule subcommands act on (name or ID)")
	cmd.AddCommand(
		dnsStatusCmd(d),
		dnsConfigCmd(d),
		dnsEnabledCmd(d, true),
		dnsEnabledCmd(d, false),
		dnsFlushCacheCmd(d),
		dnsProfilesCmd(d),
		dnsBlocklistsCmd(d),
		dnsAllowlistCmd(d),
		dnsRulesCmd(d),
	)
	return cmd
}

// profileID resolves --profile to a profile UUID. The flag takes either an ID
// or a display name, so the common case reads as `--profile "Parental
// Controls"` rather than a UUID nobody has memorised.
func (d *dnsCLI) profileID(ctx context.Context, client *wardnet.Client) (string, error) {
	profiles, err := client.DNS.Profiles(ctx)
	if err != nil {
		return "", err
	}
	for _, p := range profiles {
		if p.ID == d.profile || strings.EqualFold(p.Name, d.profile) {
			return p.ID, nil
		}
	}
	names := make([]string, 0, len(profiles))
	for _, p := range profiles {
		names = append(names, p.Name)
	}
	return "", fmt.Errorf("no filter profile matches %q (have: %s)",
		d.profile, strings.Join(names, ", "))
}

func dnsStatusCmd(d *dnsCLI) *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show resolver state, cache occupancy, and upstream latency",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			st, err := client.DNS.Status(cmd.Context())
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(st)
			}
			fmt.Printf("enabled:  %t\n", st.Enabled)
			fmt.Printf("running:  %t\n", st.Running)
			fmt.Printf("cache:    %d / %d entries (%.1f%% hit rate)\n",
				st.CacheSize, st.CacheCapacity, st.CacheHitRate*100)
			if len(st.UpstreamLatencies) == 0 {
				return nil
			}
			fmt.Println("upstreams:")
			tw := newTabWriter()
			fmt.Fprintln(tw, "  ADDRESS\tREACHABLE\tLATENCY")
			for _, u := range st.UpstreamLatencies {
				latency := "-"
				if u.AvgLatencyMs != nil {
					latency = fmt.Sprintf("%.1f ms", *u.AvgLatencyMs)
				}
				fmt.Fprintf(tw, "  %s\t%t\t%s\n", u.Address, u.Reachable, latency)
			}
			return tw.Flush()
		},
	}
}

func dnsConfigCmd(d *dnsCLI) *cobra.Command {
	return &cobra.Command{
		Use:   "config",
		Short: "Show the resolver configuration",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			cfg, err := client.DNS.Config(cmd.Context())
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(cfg)
			}
			printDNSConfig(cfg)
			return nil
		},
	}
}

func printDNSConfig(cfg *wardnet.DNSConfig) {
	fmt.Printf("enabled:             %t\n", cfg.Enabled)
	fmt.Printf("resolution mode:     %s\n", cfg.ResolutionMode)
	fmt.Printf("forwarder selection: %s\n", cfg.ForwarderSelectionMode)
	if cfg.SingleUpstream != "" {
		fmt.Printf("single upstream:     %s\n", cfg.SingleUpstream)
	}
	fmt.Printf("cache:               %d entries, TTL %d-%ds\n",
		cfg.CacheSize, cfg.CacheTTLMinSecs, cfg.CacheTTLMaxSecs)
	fmt.Printf("dnssec:              %t\n", cfg.DNSSECEnabled)
	fmt.Printf("rebind protection:   %t\n", cfg.RebindingProtection)
	fmt.Printf("rate limit:          %d/s\n", cfg.RateLimitPerSecond)
	fmt.Printf("forward timeouts:    %dms per upstream, %dms overall\n",
		cfg.UpstreamTimeoutMs, cfg.ForwardDeadlineMs)
	fmt.Printf("filtering:           %t\n", cfg.FilteringEnabled)
	fmt.Printf("query log:           %t (%d day retention)\n",
		cfg.QueryLogEnabled, cfg.QueryLogRetentionDays)
	if len(cfg.UpstreamServers) == 0 {
		return
	}
	fmt.Println("upstreams:")
	for _, u := range cfg.UpstreamServers {
		line := fmt.Sprintf("  %s (%s, %s", u.Name, u.Address, u.Protocol)
		if u.Port != 0 {
			line += fmt.Sprintf(":%d", u.Port)
		}
		if u.TLSServerName != "" {
			line += ", sni " + u.TLSServerName
		}
		fmt.Println(line + ")")
	}
}

func dnsEnabledCmd(d *dnsCLI, enabled bool) *cobra.Command {
	use, short := "enable", "Start the resolver"
	if !enabled {
		use, short = "disable", "Stop the resolver"
	}
	return &cobra.Command{
		Use:   use,
		Short: short,
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			cfg, err := client.DNS.SetEnabled(cmd.Context(), enabled)
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(cfg)
			}
			fmt.Printf("dns enabled: %t\n", cfg.Enabled)
			return nil
		},
	}
}

func dnsFlushCacheCmd(d *dnsCLI) *cobra.Command {
	return &cobra.Command{
		Use:   "flush-cache",
		Short: "Empty the resolver cache",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			cleared, err := client.DNS.FlushCache(cmd.Context())
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(map[string]any{"entries_cleared": cleared})
			}
			fmt.Printf("flushed %d cache entries\n", cleared)
			return nil
		},
	}
}

func dnsProfilesCmd(d *dnsCLI) *cobra.Command {
	return &cobra.Command{
		Use:   "profiles",
		Short: "List DNS filter profiles",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			profiles, err := client.DNS.Profiles(cmd.Context())
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(profiles)
			}
			if len(profiles) == 0 {
				fmt.Println("no profiles")
				return nil
			}
			tw := newTabWriter()
			fmt.Fprintln(tw, "ID\tNAME\tBUILTIN\tDESCRIPTION")
			for _, p := range profiles {
				fmt.Fprintf(tw, "%s\t%s\t%t\t%s\n", p.ID, p.Name, p.Builtin, orDash(p.Description))
			}
			return tw.Flush()
		},
	}
}

func dnsBlocklistsCmd(d *dnsCLI) *cobra.Command {
	cmd := &cobra.Command{Use: "blocklists", Short: "Manage a profile's blocklists"}
	cmd.AddCommand(
		blocklistsListCmd(d),
		blocklistsAddCmd(d),
		blocklistsUpdateCmd(d),
		blocklistsRemoveCmd(d),
		blocklistsRefreshCmd(d),
	)
	return cmd
}

func blocklistsListCmd(d *dnsCLI) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List the profile's blocklists",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			lists, err := client.DNS.Blocklists(cmd.Context(), profileID)
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(lists)
			}
			if len(lists) == 0 {
				fmt.Println("no blocklists")
				return nil
			}
			tw := newTabWriter()
			fmt.Fprintln(tw, "ID\tNAME\tENABLED\tENTRIES\tSCHEDULE\tUPDATED\tERROR")
			for i := range lists {
				b := &lists[i]
				fmt.Fprintf(tw, "%s\t%s\t%t\t%d\t%s\t%s\t%s\n",
					b.ID, b.Name, b.Enabled, b.EntryCount, b.CronSchedule,
					timeOrDash(b.LastUpdated), orDash(b.LastError))
			}
			return tw.Flush()
		},
	}
}

func blocklistsAddCmd(d *dnsCLI) *cobra.Command {
	var schedule string
	var disabled bool
	cmd := &cobra.Command{
		Use:   "add <name> <url>",
		Short: "Add a blocklist to the profile",
		Args:  cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			b, err := client.DNS.AddBlocklist(cmd.Context(), profileID, wardnet.CreateBlocklistParams{
				Name:         args[0],
				URL:          args[1],
				CronSchedule: schedule,
				Enabled:      !disabled,
			})
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(b)
			}
			fmt.Printf("added blocklist %s (%s)\n", b.Name, b.ID)
			return nil
		},
	}
	cmd.Flags().StringVar(&schedule, "schedule", "0 3 * * *", "cron schedule for automatic refreshes")
	cmd.Flags().BoolVar(&disabled, "disabled", false, "add the blocklist without activating it")
	return cmd
}

func blocklistsUpdateCmd(d *dnsCLI) *cobra.Command {
	var name, url, schedule string
	var enabled bool
	cmd := &cobra.Command{
		Use:   "update <id>",
		Short: "Change a blocklist's name, URL, schedule, or enabled state",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			// Only flags the operator actually typed are sent, so an update
			// of one field never resets the others to their zero values.
			var params wardnet.UpdateBlocklistParams
			if cmd.Flags().Changed("name") {
				params.Name = &name
			}
			if cmd.Flags().Changed("url") {
				params.URL = &url
			}
			if cmd.Flags().Changed("schedule") {
				params.CronSchedule = &schedule
			}
			if cmd.Flags().Changed("enabled") {
				params.Enabled = &enabled
			}
			if params == (wardnet.UpdateBlocklistParams{}) {
				return fmt.Errorf("nothing to update: pass at least one of --name, --url, --schedule, --enabled")
			}
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			b, err := client.DNS.UpdateBlocklist(cmd.Context(), profileID, args[0], params)
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(b)
			}
			fmt.Printf("updated blocklist %s\n", b.ID)
			return nil
		},
	}
	cmd.Flags().StringVar(&name, "name", "", "new display name")
	cmd.Flags().StringVar(&url, "url", "", "new source URL")
	cmd.Flags().StringVar(&schedule, "schedule", "", "new cron schedule")
	cmd.Flags().BoolVar(&enabled, "enabled", true, "whether the blocklist is applied")
	return cmd
}

func blocklistsRemoveCmd(d *dnsCLI) *cobra.Command {
	return &cobra.Command{
		Use:   "remove <id>",
		Short: "Delete a blocklist from the profile",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			if err := client.DNS.RemoveBlocklist(cmd.Context(), profileID, args[0]); err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(map[string]any{"removed": args[0]})
			}
			fmt.Printf("removed blocklist %s\n", args[0])
			return nil
		},
	}
}

func blocklistsRefreshCmd(d *dnsCLI) *cobra.Command {
	return &cobra.Command{
		Use:   "refresh <id>",
		Short: "Queue an immediate re-download of a blocklist",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			jobID, err := client.DNS.RefreshBlocklist(cmd.Context(), profileID, args[0])
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(map[string]any{"job_id": jobID})
			}
			// The download runs in the background; `blocklists list` shows
			// the outcome once it lands.
			fmt.Printf("refresh queued as job %s\n", jobID)
			return nil
		},
	}
}

func dnsAllowlistCmd(d *dnsCLI) *cobra.Command {
	cmd := &cobra.Command{Use: "allowlist", Short: "Manage a profile's allowlist"}
	cmd.AddCommand(allowlistListCmd(d), allowlistAddCmd(d), allowlistRemoveCmd(d))
	return cmd
}

func allowlistListCmd(d *dnsCLI) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List the profile's allowlist entries",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			entries, err := client.DNS.Allowlist(cmd.Context(), profileID)
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(entries)
			}
			if len(entries) == 0 {
				fmt.Println("no allowlist entries")
				return nil
			}
			tw := newTabWriter()
			fmt.Fprintln(tw, "ID\tDOMAIN\tREASON")
			for _, e := range entries {
				fmt.Fprintf(tw, "%s\t%s\t%s\n", e.ID, e.Domain, orDash(e.Reason))
			}
			return tw.Flush()
		},
	}
}

func allowlistAddCmd(d *dnsCLI) *cobra.Command {
	var reason string
	cmd := &cobra.Command{
		Use:   "add <domain>",
		Short: "Exempt a domain from the profile's blocklists",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			e, err := client.DNS.AddAllowlistEntry(cmd.Context(), profileID, args[0], reason)
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(e)
			}
			fmt.Printf("allowed %s (%s)\n", e.Domain, e.ID)
			return nil
		},
	}
	cmd.Flags().StringVar(&reason, "reason", "", "note explaining the exemption")
	return cmd
}

func allowlistRemoveCmd(d *dnsCLI) *cobra.Command {
	return &cobra.Command{
		Use:   "remove <id>",
		Short: "Delete an allowlist entry",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			if err := client.DNS.RemoveAllowlistEntry(cmd.Context(), profileID, args[0]); err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(map[string]any{"removed": args[0]})
			}
			fmt.Printf("removed allowlist entry %s\n", args[0])
			return nil
		},
	}
}

func dnsRulesCmd(d *dnsCLI) *cobra.Command {
	cmd := &cobra.Command{Use: "rules", Short: "Manage a profile's custom filter rules"}
	cmd.AddCommand(rulesListCmd(d), rulesAddCmd(d), rulesUpdateCmd(d), rulesRemoveCmd(d))
	return cmd
}

func rulesListCmd(d *dnsCLI) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List the profile's custom rules",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			rules, err := client.DNS.Rules(cmd.Context(), profileID)
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(rules)
			}
			if len(rules) == 0 {
				fmt.Println("no custom rules")
				return nil
			}
			tw := newTabWriter()
			fmt.Fprintln(tw, "ID\tENABLED\tRULE\tCOMMENT")
			for _, r := range rules {
				fmt.Fprintf(tw, "%s\t%t\t%s\t%s\n", r.ID, r.Enabled, r.Text, orDash(r.Comment))
			}
			return tw.Flush()
		},
	}
}

func rulesAddCmd(d *dnsCLI) *cobra.Command {
	var comment string
	var disabled bool
	cmd := &cobra.Command{
		Use:   "add <rule>",
		Short: "Add an AdGuard-syntax rule to the profile",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			r, err := client.DNS.AddRule(cmd.Context(), profileID, wardnet.CreateRuleParams{
				Text:    args[0],
				Comment: comment,
				Enabled: !disabled,
			})
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(r)
			}
			fmt.Printf("added rule %s\n", r.ID)
			return nil
		},
	}
	cmd.Flags().StringVar(&comment, "comment", "", "note explaining the rule")
	cmd.Flags().BoolVar(&disabled, "disabled", false, "add the rule without activating it")
	return cmd
}

func rulesUpdateCmd(d *dnsCLI) *cobra.Command {
	var text, comment string
	var enabled bool
	cmd := &cobra.Command{
		Use:   "update <id>",
		Short: "Change a custom rule's text, comment, or enabled state",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			var params wardnet.UpdateRuleParams
			if cmd.Flags().Changed("rule") {
				params.Text = &text
			}
			if cmd.Flags().Changed("comment") {
				params.Comment = &comment
			}
			if cmd.Flags().Changed("enabled") {
				params.Enabled = &enabled
			}
			if params == (wardnet.UpdateRuleParams{}) {
				return fmt.Errorf("nothing to update: pass at least one of --rule, --comment, --enabled")
			}
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			r, err := client.DNS.UpdateRule(cmd.Context(), profileID, args[0], params)
			if err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(r)
			}
			fmt.Printf("updated rule %s\n", r.ID)
			return nil
		},
	}
	cmd.Flags().StringVar(&text, "rule", "", "new rule text")
	cmd.Flags().StringVar(&comment, "comment", "", "new comment")
	cmd.Flags().BoolVar(&enabled, "enabled", true, "whether the rule is applied")
	return cmd
}

func rulesRemoveCmd(d *dnsCLI) *cobra.Command {
	return &cobra.Command{
		Use:   "remove <id>",
		Short: "Delete a custom rule",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			client, err := d.client()
			if err != nil {
				return err
			}
			profileID, err := d.profileID(cmd.Context(), client)
			if err != nil {
				return err
			}
			if err := client.DNS.RemoveRule(cmd.Context(), profileID, args[0]); err != nil {
				return err
			}
			if d.jsonOut {
				return printJSON(map[string]any{"removed": args[0]})
			}
			fmt.Printf("removed rule %s\n", args[0])
			return nil
		},
	}
}
