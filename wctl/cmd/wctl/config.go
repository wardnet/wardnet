package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"
	wardnet "wardnet.network/go"
)

// defaultDaemonURL is the daemon's plain-HTTP LAN admin endpoint.
const defaultDaemonURL = "http://127.0.0.1:7411"

// configError marks a failure to resolve configuration (missing file, bad
// TOML, or no token). main maps it to a distinct exit code.
type configError struct{ msg string }

func (e *configError) Error() string { return e.msg }

// fileConfig is the on-disk wctl.toml shape.
type fileConfig struct {
	DaemonURL string `toml:"daemon_url"`
	Token     string `toml:"token"`
}

// resolve determines the daemon URL and token from, in precedence order, the
// environment, the config file, and built-in defaults.
func (c *cli) resolve() (daemonURL, token string, err error) {
	var fc fileConfig
	path := c.configPath
	explicit := path != ""
	if path == "" {
		path = defaultConfigPath()
	}
	if path != "" {
		if _, statErr := os.Stat(path); statErr == nil {
			if _, derr := toml.DecodeFile(path, &fc); derr != nil {
				return "", "", &configError{fmt.Sprintf("invalid config %s: %v", path, derr)}
			}
		} else if explicit {
			// An explicitly requested config that doesn't exist is an error;
			// a missing default just falls through to env/defaults.
			return "", "", &configError{fmt.Sprintf("config file not found: %s", path)}
		}
	}
	daemonURL = firstNonEmpty(os.Getenv("WARDNET_DAEMON_URL"), fc.DaemonURL, defaultDaemonURL)
	token = firstNonEmpty(os.Getenv("WARDNET_TOKEN"), fc.Token)
	return strings.TrimRight(daemonURL, "/"), token, nil
}

// client builds an authenticated SDK client from the resolved configuration.
func (c *cli) client() (*wardnet.Client, error) {
	daemonURL, token, err := c.resolve()
	if err != nil {
		return nil, err
	}
	if token == "" {
		return nil, &configError{"no API token configured — set `token` in wctl.toml or the WARDNET_TOKEN environment variable"}
	}
	return wardnet.New(daemonURL,
		wardnet.WithToken(token),
		wardnet.WithUserAgent("wctl/"+version),
	)
}

// defaultConfigPath is $XDG_CONFIG_HOME/wardnet/wctl.toml, falling back to
// $HOME/.config/wardnet/wctl.toml.
func defaultConfigPath() string {
	if xdg := os.Getenv("XDG_CONFIG_HOME"); xdg != "" {
		return filepath.Join(xdg, "wardnet", "wctl.toml")
	}
	if home := os.Getenv("HOME"); home != "" {
		return filepath.Join(home, ".config", "wardnet", "wctl.toml")
	}
	return ""
}

func firstNonEmpty(vals ...string) string {
	for _, v := range vals {
		if strings.TrimSpace(v) != "" {
			return v
		}
	}
	return ""
}
