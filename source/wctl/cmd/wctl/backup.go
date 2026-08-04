package main

import (
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/spf13/cobra"
	"golang.org/x/term"
	wardnet "wardnet.network/go"
)

// minPassphraseLen mirrors the daemon's backup passphrase minimum so a
// too-short passphrase is rejected before any request.
const minPassphraseLen = 12

func newBackupCmd(c *cli) *cobra.Command {
	cmd := &cobra.Command{Use: "backup", Short: "Export and restore encrypted backup bundles"}
	cmd.AddCommand(
		backupStatusCmd(c),
		backupExportCmd(c),
		backupImportCmd(c),
		backupSnapshotsCmd(c),
	)
	return cmd
}

func backupStatusCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show the current backup subsystem phase",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			s, err := client.Backup.Status(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(s)
			}
			fmt.Println(backupStatusString(s))
			return nil
		},
	}
}

func backupStatusString(s *wardnet.BackupStatus) string {
	switch s.State {
	case wardnet.BackupImporting:
		return fmt.Sprintf("importing (%s)", s.Phase)
	case wardnet.BackupFailed:
		return "failed: " + s.Reason
	default:
		return string(s.State)
	}
}

func backupExportCmd(c *cli) *cobra.Command {
	var out, passphraseFile string
	cmd := &cobra.Command{
		Use:   "export",
		Short: "Export an encrypted bundle to --out",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			passphrase, err := readPassphrase(passphraseFile, true)
			if err != nil {
				return err
			}
			if len(passphrase) < minPassphraseLen {
				return fmt.Errorf("passphrase must be at least %d characters", minPassphraseLen)
			}
			client, err := c.client()
			if err != nil {
				return err
			}
			bundle, err := client.Backup.Export(cmd.Context(), passphrase)
			if err != nil {
				return err
			}
			if err := os.WriteFile(out, bundle, 0o600); err != nil {
				return fmt.Errorf("cannot write bundle to %s: %w", out, err)
			}
			if c.jsonOut {
				return printJSON(map[string]any{"out": out, "bytes": len(bundle)})
			}
			fmt.Printf("wrote %s to %s\n", humanBytes(int64(len(bundle))), out)
			return nil
		},
	}
	cmd.Flags().StringVar(&out, "out", "", "destination path for the bundle")
	cmd.Flags().StringVar(&passphraseFile, "passphrase-file", "",
		"read the passphrase from this file instead of prompting (use - for stdin)")
	_ = cmd.MarkFlagRequired("out")
	return cmd
}

func backupImportCmd(c *cli) *cobra.Command {
	var passphraseFile string
	cmd := &cobra.Command{
		Use:   "import <bundle>",
		Short: "Restore a previously-exported bundle",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			passphrase, err := readPassphrase(passphraseFile, false)
			if err != nil {
				return err
			}
			data, err := os.ReadFile(args[0])
			if err != nil {
				return fmt.Errorf("cannot read bundle %s: %w", args[0], err)
			}
			client, err := c.client()
			if err != nil {
				return err
			}
			preview, err := client.Backup.PreviewImport(cmd.Context(), data, passphrase)
			if err != nil {
				return err
			}
			if !preview.Compatible {
				reason := preview.IncompatibilityReason
				if reason == "" {
					reason = "incompatible bundle"
				}
				return fmt.Errorf("bundle cannot be restored: %s", reason)
			}
			if !c.jsonOut {
				fmt.Printf("restoring bundle from host %s (schema v%d)\n",
					preview.Manifest.HostID, preview.Manifest.SchemaVersion)
				for _, f := range preview.FilesToReplace {
					fmt.Printf("  will replace: %s\n", f)
				}
			}
			result, err := client.Backup.ApplyImport(cmd.Context(), preview.PreviewToken)
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(result)
			}
			fmt.Println("restore applied; restart the daemon to load the new state")
			for _, s := range result.Snapshots {
				fmt.Printf("  snapshot: %s (%s)\n", s.Path, s.Kind)
			}
			return nil
		},
	}
	cmd.Flags().StringVar(&passphraseFile, "passphrase-file", "",
		"read the passphrase from this file instead of prompting (use - for stdin)")
	return cmd
}

func backupSnapshotsCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "snapshots",
		Short: "List retained restore snapshots",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			client, err := c.client()
			if err != nil {
				return err
			}
			snaps, err := client.Backup.Snapshots(cmd.Context())
			if err != nil {
				return err
			}
			if c.jsonOut {
				return printJSON(snaps)
			}
			if len(snaps) == 0 {
				fmt.Println("no snapshots")
				return nil
			}
			tw := newTabWriter()
			fmt.Fprintln(tw, "KIND\tSIZE\tCREATED\tPATH")
			for i := range snaps {
				s := &snaps[i]
				fmt.Fprintf(tw, "%s\t%s\t%s\t%s\n",
					s.Kind, humanBytes(s.SizeBytes),
					s.CreatedAt.Format("2006-01-02T15:04:05Z07:00"), s.Path)
			}
			return tw.Flush()
		},
	}
}

// readPassphrase obtains the export/import passphrase from a file (or "-" for
// stdin) when passphraseFile is set, otherwise via a no-echo prompt. When
// confirm is set (export), the prompt is asked twice and the two must match.
func readPassphrase(passphraseFile string, confirm bool) (string, error) {
	if passphraseFile != "" {
		var raw []byte
		var err error
		if passphraseFile == "-" {
			raw, err = io.ReadAll(os.Stdin)
		} else {
			raw, err = os.ReadFile(passphraseFile)
		}
		if err != nil {
			return "", fmt.Errorf("cannot read passphrase: %w", err)
		}
		return strings.TrimRight(string(raw), "\r\n"), nil
	}

	fmt.Fprint(os.Stderr, "Passphrase: ")
	first, err := term.ReadPassword(int(os.Stdin.Fd()))
	fmt.Fprintln(os.Stderr)
	if err != nil {
		return "", fmt.Errorf("reading passphrase: %w", err)
	}
	if confirm {
		fmt.Fprint(os.Stderr, "Confirm passphrase: ")
		again, err := term.ReadPassword(int(os.Stdin.Fd()))
		fmt.Fprintln(os.Stderr)
		if err != nil {
			return "", fmt.Errorf("reading passphrase: %w", err)
		}
		if string(first) != string(again) {
			return "", fmt.Errorf("passphrases do not match")
		}
	}
	return string(first), nil
}
