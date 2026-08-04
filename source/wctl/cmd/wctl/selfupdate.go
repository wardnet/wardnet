package main

import (
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"time"

	"github.com/spf13/cobra"
)

// releaseBaseURL is the GitHub repository whose releases carry the wctl
// binaries. A var so tests can point it at a local server.
var releaseBaseURL = "https://github.com/wardnet/wardnet"

func newSelfUpdateCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "self-update",
		Short: "Update the wctl binary in place to the latest release",
		Long: "Download the latest wctl release, verify its checksum, and " +
			"atomically replace the running binary.",
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			return runSelfUpdate(cmd.Context(), c.jsonOut)
		},
	}
}

func runSelfUpdate(ctx context.Context, jsonOut bool) error {
	if version == "dev" {
		return fmt.Errorf("wctl was built from source; install a release build to self-update")
	}

	exe, err := os.Executable()
	if err != nil {
		return fmt.Errorf("locating the running binary: %w", err)
	}
	// Resolve symlinks so we replace the real file, not a symlink into it.
	if resolved, rerr := filepath.EvalSymlinks(exe); rerr == nil {
		exe = resolved
	}

	latest, err := latestReleaseVersion(ctx)
	if err != nil {
		return err
	}
	if !versionNewer(latest, version) {
		if jsonOut {
			return printJSON(map[string]any{
				"updated": false,
				"version": version,
				"latest":  latest,
			})
		}
		fmt.Printf("wctl %s is already up to date\n", version)
		return nil
	}

	if err := selfUpdate(ctx, latest, exe); err != nil {
		return err
	}
	if jsonOut {
		return printJSON(map[string]any{
			"updated":  true,
			"previous": version,
			"version":  latest,
			"path":     exe,
		})
	}
	fmt.Printf("updated wctl %s -> %s\n", version, latest)
	return nil
}

// latestReleaseVersion resolves the newest published version by following the
// releases/latest redirect (avoiding the GitHub API and its rate limits).
func latestReleaseVersion(ctx context.Context) (string, error) {
	client := &http.Client{
		Timeout: 30 * time.Second,
		// Stop at the redirect so we can read the target tag from Location.
		CheckRedirect: func(*http.Request, []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, releaseBaseURL+"/releases/latest", nil)
	if err != nil {
		return "", err
	}
	resp, err := client.Do(req)
	if err != nil {
		return "", fmt.Errorf("checking for the latest release: %w", err)
	}
	defer func() { _ = resp.Body.Close() }()

	loc := resp.Header.Get("Location")
	if loc == "" {
		return "", fmt.Errorf("could not resolve the latest release (no redirect)")
	}
	tag := loc[strings.LastIndex(loc, "/")+1:]
	version := strings.TrimPrefix(tag, "v")
	if version == "" {
		return "", fmt.Errorf("could not parse a version from %q", loc)
	}
	return version, nil
}

// selfUpdate downloads the release binary for this platform, verifies it
// against the release checksums, and atomically replaces exe.
func selfUpdate(ctx context.Context, version, exe string) error {
	asset := fmt.Sprintf("wctl_%s_%s_%s", version, runtime.GOOS, runtime.GOARCH)
	base := fmt.Sprintf("%s/releases/download/v%s", releaseBaseURL, version)

	// Stage in the target directory so the final rename is atomic and never
	// truncates the running binary (a cross-filesystem rename would degrade to
	// copy+truncate).
	tmp, err := os.CreateTemp(filepath.Dir(exe), ".wctl-update-*")
	if err != nil {
		return fmt.Errorf("creating a staging file: %w", err)
	}
	tmpPath := tmp.Name()
	defer func() { _ = os.Remove(tmpPath) }()

	sum, err := download(ctx, base+"/"+asset, tmp)
	if err != nil {
		_ = tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}

	want, err := fetchChecksum(ctx, base+"/checksums.txt", asset)
	if err != nil {
		return err
	}
	if sum != want {
		return fmt.Errorf("checksum mismatch for %s: got %s, want %s", asset, sum, want)
	}

	if err := os.Chmod(tmpPath, 0o755); err != nil {
		return err
	}
	if err := os.Rename(tmpPath, exe); err != nil {
		return fmt.Errorf("replacing %s: %w", exe, err)
	}
	return nil
}

// download streams url into w and returns the SHA-256 of the bytes written.
func download(ctx context.Context, url string, w io.Writer) (string, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return "", err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return "", fmt.Errorf("downloading %s: %w", url, err)
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("downloading %s: unexpected status %d", url, resp.StatusCode)
	}

	h := sha256.New()
	if _, err := io.Copy(io.MultiWriter(w, h), resp.Body); err != nil {
		return "", fmt.Errorf("downloading %s: %w", url, err)
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

// fetchChecksum downloads a checksums.txt and returns the SHA-256 recorded for
// asset.
func fetchChecksum(ctx context.Context, url, asset string) (string, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return "", err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return "", fmt.Errorf("downloading checksums: %w", err)
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("downloading checksums: unexpected status %d", resp.StatusCode)
	}

	scanner := bufio.NewScanner(resp.Body)
	for scanner.Scan() {
		fields := strings.Fields(scanner.Text())
		if len(fields) == 2 && fields[1] == asset {
			return fields[0], nil
		}
	}
	if err := scanner.Err(); err != nil {
		return "", err
	}
	return "", fmt.Errorf("no checksum for %s", asset)
}

// versionNewer reports whether the CalVer latest is strictly newer than
// current. An unparseable latest never triggers an update; an unparseable
// current (e.g. a dev build) is always considered behind a real release.
func versionNewer(latest, current string) bool {
	lc, lp, lok := splitCalver(latest)
	cc, cp, cok := splitCalver(current)
	if !lok {
		return false
	}
	if !cok {
		return true
	}
	for i := range lc {
		if lc[i] != cc[i] {
			return lc[i] > cc[i]
		}
	}
	// Equal cores: a release outranks its own pre-release (2026.08.00 beats
	// 2026.08.00-beta.1); otherwise fall back to comparing the pre-release
	// labels.
	switch {
	case lp == "" && cp != "":
		return true
	case lp != "" && cp == "":
		return false
	default:
		return lp > cp
	}
}

// splitCalver parses "YYYY.MM.DD[-prerelease]" into its numeric core and
// optional pre-release label.
func splitCalver(v string) (core [3]int, prerelease string, ok bool) {
	s := v
	if i := strings.IndexByte(s, '-'); i >= 0 {
		prerelease = s[i+1:]
		s = s[:i]
	}
	parts := strings.Split(s, ".")
	if len(parts) != 3 {
		return core, "", false
	}
	for i, p := range parts {
		n, err := strconv.Atoi(p)
		if err != nil {
			return core, "", false
		}
		core[i] = n
	}
	return core, prerelease, true
}
