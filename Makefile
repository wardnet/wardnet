# Wardnet Makefile
# Unified build commands for local development and CI.

# Recipes use bash (not /bin/sh → dash on Ubuntu) so `set -o pipefail`,
# `[[ ... ]]`, and other bash-isms work consistently across macOS and CI.
SHELL := /bin/bash

# ---------- Configuration ----------

DAEMON_DIR   := source/daemon
SDK_DIR      := source/sdk/wardnet-js
WEBUI_DIR    := source/web-ui
SITE_DIR     := source/site

# Container runtime: prefer podman, fall back to docker.
CONTAINER_RT := $(shell command -v podman 2>/dev/null || command -v docker 2>/dev/null)
CONTAINER_RT_NAME := $(notdir $(CONTAINER_RT))
RUST_IMAGE   := docker.io/library/rust:1.95

# Docker image build settings.
# Override IMAGE_TAG on the CLI to name the local image differently, e.g.:
#   make image IMAGE_TAG=wardnetd:v0.2.0
# Override IMAGE_VERSION to pin a specific release (no v-prefix):
#   make image IMAGE_VERSION=0.2.0
IMAGE_TAG     ?= wardnetd:dev
IMAGE_VERSION ?= latest
# Linux build artefacts live here (gitignored, persists on host for
# incremental compilation). Separate from the macOS target/ directory.
LINUX_TARGET := $(CURDIR)/.target-linux

# Coverage: files excluded from cargo-llvm-cov.  Single source of truth —
# CI calls `make coverage-daemon` with COV_FMT overridden for LCOV output.
COV_IGNORE := (main\.rs|noop_.*\.rs|db\.rs|web\.rs|api/mod\.rs|auth_context\.rs|command\.rs|policy_router_netlink\.rs|route_monitor\.rs|wardnet-test-agent/.*|wardnetd-mock/src/events\.rs|wardnetd-data/src/lib\.rs)
# Default: human-readable summary.  CI overrides:
#   make coverage-daemon COV_FMT="--lcov --output-path ../../coverage/daemon-lcov.info"
COV_FMT    ?= --summary-only

# ---------- Phony targets ----------

.PHONY: all init build build-daemon build-sdk build-web build-site \
        check check-sdk check-web check-site check-daemon check-daemon-native check-daemon-container \
        coverage-daemon coverage-daemon-native coverage-daemon-container \
        openapi check-openapi \
        fmt clippy test \
        image image-multiarch \
        run-dev run-dev-daemon run-dev-web \
        sync-version check-version \
        clean help

# ---------- Version ----------

# Single source of truth for the project version (daemon Cargo workspace +
# all three package.json files). Edit ./VERSION and then:
#   make sync-version   # propagate to daemon Cargo.toml + web-ui/sdk/site package.json
#   make check-version  # verify everything agrees with ./VERSION (used in CI)
VERSION_FILE := VERSION
VERSION      := $(shell cat $(VERSION_FILE) 2>/dev/null | tr -d '[:space:]')

all: build

# ---------- Version sync ----------

# Propagate the ./VERSION file into the daemon Cargo workspace and every
# package.json. Uses perl (available on macOS + Linux) rather than sed to
# avoid BSD/GNU flag differences.
sync-version:
	@test -n "$(VERSION)" || { echo "Error: $(VERSION_FILE) is empty or missing"; exit 1; }
	@echo "Syncing version -> $(VERSION)"
	@perl -pi -e 's/^(version = )"[^"]+"/$$1"$(VERSION)"/ if $$. < 25 && !$$done; $$done=1 if s/^(version = )"[^"]+"/$$1"$(VERSION)"/' $(DAEMON_DIR)/Cargo.toml
	@for f in $(SDK_DIR)/package.json $(WEBUI_DIR)/package.json $(SITE_DIR)/package.json; do \
		perl -pi -e 'if (!$$done && /"version":\s*"[^"]*"/) { s/"version":\s*"[^"]*"/"version": "$(VERSION)"/; $$done=1 }' $$f; \
	done
	@echo "  updated: $(DAEMON_DIR)/Cargo.toml"
	@echo "  updated: $(SDK_DIR)/package.json"
	@echo "  updated: $(WEBUI_DIR)/package.json"
	@echo "  updated: $(SITE_DIR)/package.json"
	@echo "Tip: regenerate lockfiles via 'cargo check' and 'yarn install' before committing."

# Verify every versioned file agrees with ./VERSION. Intended for CI.
check-version:
	@test -n "$(VERSION)" || { echo "Error: $(VERSION_FILE) is empty or missing"; exit 1; }
	@ok=true; \
	v=$$(awk '/^\[workspace\.package\]/{p=1; next} p && /^\[/{exit} p && /^version[[:space:]]*=/{gsub(/[" ]/, "", $$3); print $$3; exit}' $(DAEMON_DIR)/Cargo.toml); \
	if [ "$$v" != "$(VERSION)" ]; then echo "MISMATCH $(DAEMON_DIR)/Cargo.toml: $$v != $(VERSION)"; ok=false; fi; \
	for f in $(SDK_DIR)/package.json $(WEBUI_DIR)/package.json $(SITE_DIR)/package.json; do \
		v=$$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' $$f | head -1); \
		if [ "$$v" != "$(VERSION)" ]; then echo "MISMATCH $$f: $$v != $(VERSION)"; ok=false; fi; \
	done; \
	if [ "$$ok" = "true" ]; then echo "All files match $(VERSION_FILE)=$(VERSION)"; else exit 1; fi

# ---------- Dev environment setup ----------

init:
	@echo "Installing development dependencies..."
	@command -v rustup >/dev/null || { echo "Error: rustup not found. Install from https://rustup.rs"; exit 1; }
	@command -v node >/dev/null || { echo "Error: node not found. Install Node.js 25+"; exit 1; }
	@command -v yarn >/dev/null || { echo "Error: yarn not found. Run: corepack enable"; exit 1; }
	cd $(SDK_DIR) && yarn install
	cd $(WEBUI_DIR) && yarn install
	cd $(SITE_DIR) && yarn install
	@echo ""
	@echo "Dev environment ready. Run 'make help' to see available targets."

# ---------- SDK ----------

check-sdk:
	cd $(SDK_DIR) && yarn install --immutable
	cd $(SDK_DIR) && yarn type-check
	cd $(SDK_DIR) && yarn format:check

# ---------- Web UI ----------

build-web: check-sdk
	cd $(WEBUI_DIR) && yarn install --immutable && yarn build

check-web: check-sdk
	cd $(WEBUI_DIR) && yarn install --immutable
	cd $(WEBUI_DIR) && yarn type-check
	cd $(WEBUI_DIR) && yarn lint
	cd $(WEBUI_DIR) && yarn format:check

# ---------- Public Site ----------

build-site:
	cd $(SITE_DIR) && yarn install --immutable && yarn build

check-site:
	cd $(SITE_DIR) && yarn install --immutable
	cd $(SITE_DIR) && yarn type-check
	cd $(SITE_DIR) && yarn format:check
	cd $(SITE_DIR) && yarn test

# ---------- Daemon ----------

# `build-daemon` compiles the Rust workspace. Two optional vars let CI (and
# advanced local builds) reuse this target for cross-compilation without
# duplicating the recipe or re-running the web UI build:
#
#   TARGET=<rust-target-triple>   append `--target <triple>` (cross-compile)
#   CRATE=<crate-name>            append `-p <crate>` (single-crate build)
#
# Example (CI aarch64-linux job, after downloading the web-ui-dist artifact):
#   make build-daemon TARGET=aarch64-unknown-linux-gnu CRATE=wardnetd
CARGO_TARGET_FLAG := $(if $(TARGET),--target $(TARGET),)
CARGO_CRATE_FLAG  := $(if $(CRATE),-p $(CRATE),)

build-daemon:
	cd $(DAEMON_DIR) && cargo build --release $(CARGO_TARGET_FLAG) $(CARGO_CRATE_FLAG)

# check-daemon: auto-selects native (Linux) or container (macOS/other).
# The daemon uses Linux-only dependencies (netlink, rtnetlink) so it cannot
# compile natively on macOS.  On non-Linux hosts we run the checks inside a
# container using podman or docker (auto-detected).
ifeq ($(shell uname -s),Linux)
check-daemon: check-daemon-native
else
check-daemon: check-daemon-container
endif

# SARIF_OUT: when set to a path, `check-daemon-native` pipes clippy output
# through `clippy-sarif` and writes a SARIF file there (for CI upload to the
# GitHub Code Scanning tab). When unset, clippy runs plainly — no extra
# tooling required for local dev.
SARIF_OUT ?=

check-daemon-native:
	cd $(DAEMON_DIR) && cargo fmt --check
	@set -o pipefail; \
	if [ -n "$(SARIF_OUT)" ]; then \
		echo "Emitting clippy SARIF -> $(SARIF_OUT)"; \
		cd $(DAEMON_DIR) && cargo clippy --all-targets --message-format=json -- -D warnings \
			| clippy-sarif | tee "$(abspath $(SARIF_OUT))" | sarif-fmt; \
	else \
		cd $(DAEMON_DIR) && cargo clippy --all-targets -- -D warnings; \
	fi
	cd $(DAEMON_DIR) && cargo test --workspace

check-daemon-container:
	@test -n "$(CONTAINER_RT)" || { echo "Error: podman or docker is required for non-Linux daemon checks"; exit 1; }
	@echo "Using $(CONTAINER_RT_NAME) to run daemon checks in Linux container..."
	@mkdir -p $(LINUX_TARGET)
	$(CONTAINER_RT) run --rm \
		-v $(CURDIR):/workspace:z \
		-v wardnet-cargo-cache:/usr/local/cargo/registry \
		-w /workspace/$(DAEMON_DIR) \
		-e CARGO_TARGET_DIR=/workspace/.target-linux \
		$(RUST_IMAGE) \
		sh -c 'rustup component add clippy rustfmt 2>/dev/null; cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace'

# coverage-daemon: generate a line-coverage summary for the daemon workspace.
# Requires cargo-llvm-cov (installed automatically in the container path).
# Uses the same ignore regex as CI so local numbers match.
ifeq ($(shell uname -s),Linux)
coverage-daemon: coverage-daemon-native
else
coverage-daemon: coverage-daemon-container
endif

coverage-daemon-native:
	cd $(DAEMON_DIR) && cargo llvm-cov --workspace $(COV_FMT) \
		--ignore-filename-regex '$(COV_IGNORE)'

coverage-daemon-container:
	@test -n "$(CONTAINER_RT)" || { echo "Error: podman or docker is required for non-Linux coverage"; exit 1; }
	@echo "Using $(CONTAINER_RT_NAME) to run daemon coverage in Linux container..."
	@mkdir -p $(LINUX_TARGET)
	$(CONTAINER_RT) run --rm \
		-v $(CURDIR):/workspace:z \
		-v wardnet-cargo-cache:/usr/local/cargo/registry \
		-w /workspace/$(DAEMON_DIR) \
		-e CARGO_TARGET_DIR=/workspace/.target-linux \
		$(RUST_IMAGE) \
		sh -c 'rustup component add llvm-tools-preview 2>/dev/null; cargo install cargo-llvm-cov --quiet 2>/dev/null; cargo llvm-cov --workspace $(COV_FMT) --ignore-filename-regex '"'"'$(COV_IGNORE)'"'"''

# ---------- OpenAPI spec ----------
#
# The `dump_openapi` binary lives inside the `wardnetd-api` crate (at
# `crates/wardnetd-api/src/bin/dump_openapi.rs`), so Cargo rebuilds it
# whenever any handler annotation or DTO changes. Output is exactly the
# same JSON the daemon serves at runtime on `/api/openapi.json`.
#
# The checked-in `docs/openapi.json` is the canonical snapshot on every
# commit. Release tags upload this file as an asset, and the site's
# manifest generator dedupes by content hash so users only see distinct
# spec versions. Keeping the file in-tree means API consumers can diff
# it in PRs and CI can gate on drift without the release workflow being
# the only place the spec ever gets regenerated.

OPENAPI_FILE := docs/openapi.json

openapi:
	@mkdir -p $(dir $(OPENAPI_FILE))
	@cd $(DAEMON_DIR) && cargo run -p wardnetd-api --bin dump_openapi --quiet \
		> $(CURDIR)/$(OPENAPI_FILE)
	@echo "Wrote $(OPENAPI_FILE)"

# Drift gate: regenerate the spec and fail if the committed copy is
# stale. Author runs `make openapi` locally and commits the updated
# file; CI never auto-commits.
check-openapi: openapi
	@if ! git diff --exit-code -- $(OPENAPI_FILE) > /dev/null; then \
		echo ""; \
		echo "OpenAPI spec drift detected in $(OPENAPI_FILE)."; \
		echo "Run 'make openapi' locally and commit the updated file."; \
		git --no-pager diff --stat -- $(OPENAPI_FILE); \
		exit 1; \
	fi
	@echo "OpenAPI spec is in sync."

# ---------- Compound targets ----------

build: build-web build-daemon

check: check-web check-site check-daemon

# ---------- Dev loop ----------

RESUME ?= false
LOCAL_DIR := $(CURDIR)/.wardnet-local

# Run the mock daemon + web UI dev server locally.
#
# wardnetd-mock serves the HTTP API on 127.0.0.1:7411 with no-op network
# backends and seeded demo data. Vite dev server runs on :7412 and proxies
# /api to the mock. Ctrl+C stops the dev server and tears down the mock
# via the EXIT trap. Database is in-memory by default (ephemeral).
# Use RESUME=true to persist the database at .wardnet-local/wardnet.db.
# Run just the mock daemon (`wardnetd-mock`) in the foreground on :7411.
# Respects `RESUME=true` for on-disk DB persistence at
# `.wardnet-local/wardnet.db`. Use this when you only need the API (e.g.
# exercising `/api/docs`, curling endpoints) without the Vite server.
run-dev-daemon:
	@mkdir -p $(LOCAL_DIR)
	@if [ "$(RESUME)" = "true" ]; then \
		DB_ARG="--database $(LOCAL_DIR)/wardnet.db --no-seed"; \
		[ -f $(LOCAL_DIR)/wardnet.db ] || DB_ARG="--database $(LOCAL_DIR)/wardnet.db"; \
		echo "Using on-disk DB at $(LOCAL_DIR)/wardnet.db"; \
	else \
		DB_ARG=""; \
		echo "Using in-memory DB (use RESUME=true for on-disk persistence)"; \
	fi; \
	echo "Mock API : http://127.0.0.1:7411"; \
	echo ""; \
	cargo run --manifest-path=$(DAEMON_DIR)/Cargo.toml --bin wardnetd-mock -- --verbose $$DB_ARG

# Run just the Vite dev server on :7412, proxying `/api` to :7411. Use
# this when you already have a mock daemon running in another terminal.
run-dev-web:
	@echo "Web UI   : http://127.0.0.1:7412  (proxies /api to mock on :7411)"
	@echo ""
	@cd $(WEBUI_DIR) && yarn dev

# Run mock daemon + Vite dev server together. The daemon is spawned in
# the background via a recursive `$(MAKE) run-dev-daemon` call; the web
# dev server runs in the foreground. Ctrl+C stops Vite and tears down
# the mock via the EXIT trap.
#
# Database is in-memory by default (ephemeral); `RESUME=true` to
# persist at `.wardnet-local/wardnet.db`.
run-dev:
	@echo "=== Starting wardnetd-mock + web UI dev server ==="
	@set -e; \
	$(MAKE) run-dev-daemon & \
	DAEMON_PID=$$!; \
	trap "kill $$DAEMON_PID 2>/dev/null; wait $$DAEMON_PID 2>/dev/null; true" EXIT INT TERM; \
	$(MAKE) run-dev-web

# ---------- Container images ----------

# Build the production image for the local architecture and load it into the
# local container daemon. Uses the wardnet release specified by VERSION
# (default: latest stable via the GitHub Releases API).
image:
	@test -n "$(CONTAINER_RT)" || { echo "Error: podman or docker is required"; exit 1; }
	$(CONTAINER_RT) build \
		--build-arg WARDNET_VERSION=$(IMAGE_VERSION) \
		-f source/daemon/Dockerfile \
		-t $(IMAGE_TAG) \
		.

# Build multi-arch production images (linux/amd64 + linux/arm64) via buildx.
# Requires `docker buildx` or `podman buildx`. Does not load into the local
# daemon (use --push or --output to export). Intended for the release workflow.
image-multiarch:
	@test -n "$(CONTAINER_RT)" || { echo "Error: podman or docker is required"; exit 1; }
	$(CONTAINER_RT) buildx build \
		--platform linux/amd64,linux/arm64 \
		--build-arg WARDNET_VERSION=$(IMAGE_VERSION) \
		-f source/daemon/Dockerfile \
		.

# ---------- Utilities ----------

clean:
	cd $(DAEMON_DIR) && cargo clean
	rm -rf $(WEBUI_DIR)/dist $(WEBUI_DIR)/node_modules/.cache $(SDK_DIR)/dist $(SITE_DIR)/dist

help:
	@echo "Wardnet build targets:"
	@echo ""
	@echo "  init           Install all dev dependencies (yarn workspaces)"
	@echo ""
	@echo "  build          Build web UI + daemon (host target)"
	@echo "  build-web      Build web UI (depends on SDK check)"
	@echo "  build-daemon   Build daemon for host target"
	@echo ""
	@echo "  check          Run all checks (SDK + web + site + daemon)"
	@echo "  check-sdk      Typecheck + format check for SDK"
	@echo "  check-web      Typecheck + lint + format check for web UI (depends on SDK)"
	@echo "  check-site     Typecheck + format check + tests for public site"
	@echo "  check-daemon   Format + clippy + tests for daemon (auto: native on Linux, container on macOS)"
	@echo "  coverage-daemon Line-coverage summary for daemon (auto: native on Linux, container on macOS)"
	@echo ""
	@echo "  openapi        Regenerate $(OPENAPI_FILE) from the daemon's #[utoipa::path] annotations"
	@echo "  check-openapi  Drift gate: fail if $(OPENAPI_FILE) is stale (run 'make openapi' to fix)"
	@echo ""
	@echo "  run-dev        Run wardnetd-mock + web UI dev server locally"
	@echo "                 Mock API on :7411, web UI on :7412 (proxies /api)"
	@echo "                 Ctrl+C stops both. In-memory DB by default."
	@echo "                 make run-dev                    (ephemeral in-memory DB)"
	@echo "                 make run-dev RESUME=true        (persist DB at .wardnet-local/)"
	@echo "  run-dev-daemon Run just wardnetd-mock on :7411 (same RESUME flag)"
	@echo "  run-dev-web    Run just the Vite dev server on :7412"
	@echo ""
	@echo "  image          Build production container image (downloads latest release)"
	@echo "                 make image                              (latest stable release)"
	@echo "                 make image IMAGE_VERSION=0.2.0          (specific version)"
	@echo "                 make image IMAGE_TAG=wardnetd:v0.2.0"
	@echo "  image-multiarch  Build multi-arch image via buildx (amd64 + arm64; no local load)"
	@echo ""
	@echo "  sync-version   Propagate ./VERSION into daemon Cargo.toml + package.json files"
	@echo "  check-version  Verify all versioned files match ./VERSION (CI gate)"
	@echo ""
	@echo "  clean          Clean all build artifacts"
