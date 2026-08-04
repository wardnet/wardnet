// Command wctl is the command-line client for the wardnet gateway. It drives
// the same HTTP API as the web UI, reading the daemon URL and an API token
// from ~/.config/wardnet/wctl.toml (or the WARDNET_DAEMON_URL / WARDNET_TOKEN
// environment variables).
package main

import (
	"errors"
	"fmt"
	"os"

	"github.com/spf13/cobra"
	wardnet "wardnet.network/go"
)

// version is the build version, overridden at release time via
// -ldflags "-X main.version=<tag>". It defaults to "dev" for local builds.
var version = "dev"

func main() {
	if err := rootCmd().Execute(); err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(exitCode(err))
	}
}

// cli holds resolved global flags shared across commands.
type cli struct {
	jsonOut    bool
	configPath string
}

func rootCmd() *cobra.Command {
	c := &cli{}
	root := &cobra.Command{
		Use:           "wctl",
		Short:         "Command-line client for the wardnet gateway",
		Version:       version,
		SilenceUsage:  true,
		SilenceErrors: true,
	}
	root.PersistentFlags().BoolVar(&c.jsonOut, "json", false, "output as JSON")
	root.PersistentFlags().StringVar(&c.configPath, "config", "",
		"path to the wctl config file (default $XDG_CONFIG_HOME/wardnet/wctl.toml)")

	root.AddCommand(
		newStatusCmd(c),
		newDevicesCmd(c),
		newTunnelsCmd(c),
		newUpdateCmd(c),
		newBackupCmd(c),
		newSelfUpdateCmd(c),
		newVersionCmd(c),
	)
	return root
}

// exitCode maps an error to a process exit code so scripts can branch on the
// failure kind: 2 config, 3 auth, 4 not found, 1 otherwise.
func exitCode(err error) int {
	var cfgErr *configError
	if errors.As(err, &cfgErr) {
		return 2
	}
	var apiErr *wardnet.APIError
	if errors.As(err, &apiErr) {
		switch apiErr.StatusCode {
		case 401, 403:
			return 3
		case 404:
			return 4
		default:
			return 1
		}
	}
	return 1
}
