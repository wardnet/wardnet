package main

import (
	"io"
	"os"
	"testing"
)

// runCLI executes wctl with args against daemonURL and returns whatever the
// command wrote to stdout. The environment is pinned so a developer's real
// ~/.config/wardnet/wctl.toml cannot influence the result.
func runCLI(t *testing.T, daemonURL string, args ...string) (string, error) {
	t.Helper()
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	t.Setenv("WARDNET_DAEMON_URL", daemonURL)
	t.Setenv("WARDNET_TOKEN", "test-token")

	r, w, err := os.Pipe()
	if err != nil {
		t.Fatalf("pipe: %v", err)
	}
	realStdout := os.Stdout
	os.Stdout = w

	done := make(chan string, 1)
	go func() {
		out, _ := io.ReadAll(r)
		done <- string(out)
	}()

	cmd := rootCmd()
	cmd.SetArgs(args)
	cmd.SetOut(io.Discard)
	cmd.SetErr(io.Discard)
	runErr := cmd.Execute()

	os.Stdout = realStdout
	_ = w.Close()
	out := <-done
	_ = r.Close()
	return out, runErr
}
