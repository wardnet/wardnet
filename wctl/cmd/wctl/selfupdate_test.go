package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestVersionNewer(t *testing.T) {
	cases := []struct {
		latest, current string
		want            bool
	}{
		{"2026.08.00", "2026.07.01", true},
		{"2026.07.01", "2026.07.01", false},
		{"2026.07.00", "2026.08.00", false},
		{"2026.08.00", "dev", true},               // dev is always behind a release
		{"2026.08.00", "2026.08.00-beta.1", true}, // release beats its pre-release
		{"garbage", "2026.07.00", false},          // unparseable latest never updates
	}
	for _, tc := range cases {
		if got := versionNewer(tc.latest, tc.current); got != tc.want {
			t.Errorf("versionNewer(%q, %q) = %v, want %v", tc.latest, tc.current, got, tc.want)
		}
	}
}

func TestSelfUpdate(t *testing.T) {
	const newVersion = "2026.08.00"
	newBinary := []byte("this is the new wctl binary\n")
	sum := sha256.Sum256(newBinary)
	asset := fmt.Sprintf("wctl_%s_%s_%s", newVersion, runtime.GOOS, runtime.GOARCH)

	mux := http.NewServeMux()
	mux.HandleFunc("/releases/download/v"+newVersion+"/"+asset,
		func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write(newBinary) })
	mux.HandleFunc("/releases/download/v"+newVersion+"/checksums.txt",
		func(w http.ResponseWriter, _ *http.Request) {
			fmt.Fprintf(w, "%s  %s\n", hex.EncodeToString(sum[:]), asset)
		})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	restore := releaseBaseURL
	releaseBaseURL = srv.URL
	defer func() { releaseBaseURL = restore }()

	// Stand in for the running binary.
	exe := filepath.Join(t.TempDir(), "wctl")
	if err := os.WriteFile(exe, []byte("old binary"), 0o755); err != nil {
		t.Fatal(err)
	}

	if err := selfUpdate(context.Background(), newVersion, exe); err != nil {
		t.Fatalf("selfUpdate: %v", err)
	}

	got, err := os.ReadFile(exe)
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != string(newBinary) {
		t.Errorf("binary was not replaced: got %q", got)
	}
}

func TestSelfUpdateChecksumMismatch(t *testing.T) {
	const newVersion = "2026.08.00"
	asset := fmt.Sprintf("wctl_%s_%s_%s", newVersion, runtime.GOOS, runtime.GOARCH)

	mux := http.NewServeMux()
	mux.HandleFunc("/releases/download/v"+newVersion+"/"+asset,
		func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write([]byte("tampered")) })
	mux.HandleFunc("/releases/download/v"+newVersion+"/checksums.txt",
		func(w http.ResponseWriter, _ *http.Request) {
			fmt.Fprintf(w, "%s  %s\n", "0000000000000000000000000000000000000000000000000000000000000000", asset)
		})
	srv := httptest.NewServer(mux)
	defer srv.Close()

	restore := releaseBaseURL
	releaseBaseURL = srv.URL
	defer func() { releaseBaseURL = restore }()

	exe := filepath.Join(t.TempDir(), "wctl")
	if err := os.WriteFile(exe, []byte("old binary"), 0o755); err != nil {
		t.Fatal(err)
	}

	if err := selfUpdate(context.Background(), newVersion, exe); err == nil {
		t.Fatal("expected a checksum mismatch error")
	}
	// The original binary must be left intact on a failed update.
	got, _ := os.ReadFile(exe)
	if string(got) != "old binary" {
		t.Errorf("binary was modified despite checksum failure: %q", got)
	}
}

func TestLatestReleaseVersion(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/releases/latest" {
			http.Redirect(w, r, "/releases/tag/v2026.08.00", http.StatusFound)
			return
		}
		http.NotFound(w, r)
	}))
	defer srv.Close()

	restore := releaseBaseURL
	releaseBaseURL = srv.URL
	defer func() { releaseBaseURL = restore }()

	got, err := latestReleaseVersion(context.Background())
	if err != nil {
		t.Fatalf("latestReleaseVersion: %v", err)
	}
	if got != "2026.08.00" {
		t.Errorf("got %q, want 2026.08.00", got)
	}
}
