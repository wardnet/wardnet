# Wardnet Makefile
# Unified build commands for local development and CI.

# Recipes use bash (not /bin/sh → dash on Ubuntu) so `set -o pipefail`,
# `[[ ... ]]`, and other bash-isms work consistently across macOS and CI.
SHELL := /bin/bash

# ---------- Configuration ----------

PI_TARGET    := aarch64-unknown-linux-gnu
DAEMON_DIR   := source/daemon
SDK_DIR      := source/sdk/wardnet-js
WEBUI_DIR    := source/web-ui
SITE_DIR     := source/site
SYSTEST_DIR  := source/system-tests

# Override on CLI: make deploy PI_HOST=wardnet.local
PI_HOST      ?= gateway
PI_USER      ?= wardnet
PI_BIN_DIR   ?= /usr/local/bin
PI_LAN_IF    ?= eth0
PI_REMOTE     = $(PI_USER)@$(PI_HOST)
OTEL         ?= false
OTEL_HOST    ?=

# Container runtime: prefer podman, fall back to docker.
CONTAINER_RT := $(shell command -v podman 2>/dev/null || command -v docker 2>/dev/null)
CONTAINER_RT_NAME := $(notdir $(CONTAINER_RT))
RUST_IMAGE   := docker.io/library/rust:1.94
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

.PHONY: all init build build-daemon build-sdk build-web build-site build-pi \
        check check-sdk check-web check-site check-daemon check-daemon-native check-daemon-container \
        coverage-daemon coverage-daemon-native coverage-daemon-container \
        openapi check-openapi \
        fmt clippy test \
        deploy run-pi run-dev system-test system-test-setup system-test-teardown \
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
	rustup target add $(PI_TARGET)
	@echo "Installing cross-compilation toolchain..."
	sudo apt-get update && sudo apt-get install -y gcc-aarch64-linux-gnu
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

# Convenience wrapper for local "build everything for the Pi in one shot":
# builds the web UI first so the embedded assets are fresh, then cross-compiles
# the daemon. CI skips this — it downloads the web-ui-dist artifact and calls
# `build-daemon` directly with TARGET set.
build-pi: build-web
	$(MAKE) build-daemon TARGET=$(PI_TARGET) CRATE=wardnetd

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

# ---------- Deploy & Run ----------

RESUME ?= false
LOCAL_DIR := $(CURDIR)/.wardnet-local

# Run the mock daemon + web UI dev server locally.
#
# wardnetd-mock serves the HTTP API on 127.0.0.1:7411 with no-op network
# backends and seeded demo data. Vite dev server runs on :7412 and proxies
# /api to the mock. Ctrl+C stops the dev server and tears down the mock
# via the EXIT trap. Database is in-memory by default (ephemeral).
# Use RESUME=true to persist the database at .wardnet-local/wardnet.db.
run-dev:
	@mkdir -p $(LOCAL_DIR)
	@if [ "$(RESUME)" = "true" ]; then \
		DB_ARG="--database $(LOCAL_DIR)/wardnet.db --no-seed"; \
		[ -f $(LOCAL_DIR)/wardnet.db ] || DB_ARG="--database $(LOCAL_DIR)/wardnet.db"; \
		echo "Using on-disk DB at $(LOCAL_DIR)/wardnet.db"; \
	else \
		DB_ARG=""; \
		echo "Using in-memory DB (use RESUME=true for on-disk persistence)"; \
	fi; \
	echo "=== Starting wardnetd-mock + web UI dev server ==="; \
	echo "Mock API : http://127.0.0.1:7411"; \
	echo "Web UI   : http://127.0.0.1:7412  (proxies /api to mock)"; \
	echo ""; \
	set -e; \
	cargo run --manifest-path=$(DAEMON_DIR)/Cargo.toml --bin wardnetd-mock -- --verbose $$DB_ARG & \
	DAEMON_PID=$$!; \
	trap "kill $$DAEMON_PID 2>/dev/null; wait $$DAEMON_PID 2>/dev/null; true" EXIT INT TERM; \
	cd $(WEBUI_DIR) && yarn dev

run-pi: build-pi
	@test -n "$(PI_HOST)" || { echo "Error: PI_HOST is required"; exit 1; }
	@PI_HOME=$$(ssh $(PI_REMOTE) 'echo $$HOME') && \
	OTEL_SECTION="" && \
	if [ "$(OTEL)" = "true" ]; then \
		OTEL_EP="$(OTEL_HOST)"; \
		if [ -z "$$OTEL_EP" ]; then \
			OTEL_EP=$$(ipconfig getifaddr en0 2>/dev/null || hostname -I 2>/dev/null | awk '{print $$1}'); \
		fi; \
		OTEL_SECTION=$$(printf '\n[otel]\nenabled = true\nendpoint = "http://%s:4317"\n\n[otel.metrics]\nenabled = true\n\n[pyroscope]\nenabled = true\nendpoint = "http://%s:4040"\n' "$$OTEL_EP" "$$OTEL_EP"); \
	fi && \
	printf '[database]\npath = "%s/wardnet-dev/wardnet.db"\n\n[logging]\npath = "%s/wardnet-dev/logs/wardnetd.log"\nlevel = "debug"\n\n[network]\nlan_interface = "%s"\n\n[tunnel]\nkeys_dir = "%s/wardnet-dev/keys"\n%s' \
		"$$PI_HOME" "$$PI_HOME" "$(PI_LAN_IF)" "$$PI_HOME" "$$OTEL_SECTION" > /tmp/wardnet-dev.toml && \
	ssh $(PI_REMOTE) 'sudo systemctl stop wardnetd-dev 2>/dev/null; true' && \
	ssh $(PI_REMOTE) 'mkdir -p ~/wardnet-dev/keys ~/wardnet-dev/logs' && \
	if [ "$(RESUME)" != "true" ]; then \
		echo "Cleaning database (use RESUME=true to keep it)..." && \
		ssh $(PI_REMOTE) 'rm -f ~/wardnet-dev/wardnet.db ~/wardnet-dev/wardnet.db-wal ~/wardnet-dev/wardnet.db-shm'; \
	else \
		echo "Resuming with existing database..."; \
	fi && \
	scp $(DAEMON_DIR)/target/$(PI_TARGET)/release/wardnetd $(PI_REMOTE):~/wardnetd && \
	scp /tmp/wardnet-dev.toml $(PI_REMOTE):~/wardnet-dev/wardnet.toml && \
	rm /tmp/wardnet-dev.toml && \
	sed "s|WARDNET_USER|$(PI_USER)|g; s|WARDNET_HOME|$$PI_HOME|g" deploy/wardnetd-dev.service > /tmp/wardnetd-dev.service && \
	scp /tmp/wardnetd-dev.service $(PI_REMOTE):/tmp/wardnetd-dev.service && \
	rm /tmp/wardnetd-dev.service && \
	ssh $(PI_REMOTE) 'sudo mv /tmp/wardnetd-dev.service /etc/systemd/system/wardnetd-dev.service && sudo systemctl daemon-reload' && \
	ssh -t $(PI_REMOTE) 'sudo systemctl start wardnetd-dev && journalctl -u wardnetd-dev -f --no-hostname'

# ---------- Production Deploy ----------

deploy-prod: build-pi
	@test -n "$(PI_HOST)" || { echo "Error: PI_HOST is required"; exit 1; }
	@echo "=== Deploying wardnet to production on $(PI_HOST) ==="
	@echo "Installing prerequisites..."
	ssh $(PI_REMOTE) 'sudo apt-get install -y -qq procps iproute2 nftables wireguard-tools conntrack 2>/dev/null'
	@echo "Running install script..."
	scp deploy/install.sh deploy/wardnetd.service $(PI_REMOTE):/tmp/
	ssh $(PI_REMOTE) 'sudo bash /tmp/install.sh --lan-interface $(PI_LAN_IF) && rm /tmp/install.sh /tmp/wardnetd.service'
	@echo "Stopping existing daemons..."
	ssh $(PI_REMOTE) 'sudo systemctl stop wardnetd-dev 2>/dev/null; sudo systemctl disable wardnetd-dev 2>/dev/null; true'
	ssh $(PI_REMOTE) 'sudo systemctl stop wardnetd 2>/dev/null; true'
	@echo "Deploying binary..."
	scp $(DAEMON_DIR)/target/$(PI_TARGET)/release/wardnetd $(PI_REMOTE):/tmp/wardnetd
	ssh $(PI_REMOTE) 'sudo mv /tmp/wardnetd /usr/local/bin/wardnetd && sudo chmod 755 /usr/local/bin/wardnetd'
	@echo "Starting service..."
	ssh $(PI_REMOTE) 'sudo systemctl start wardnetd'
	@sleep 2
	ssh $(PI_REMOTE) 'sudo systemctl is-active wardnetd && echo "wardnetd is running"'
	@echo ""
	@echo "=== Production deploy complete ==="
	@echo "Web UI: http://$(PI_HOST):7411"
	@echo "Logs:   ssh $(PI_REMOTE) 'sudo journalctl -u wardnetd -f'"

# ---------- System Tests ----------
#
# Full end-to-end tests against a real Pi with WireGuard, ip rules, nftables.
# Requires: PI_HOST set, SSH access to the Pi, cross-compilation toolchain.
#
#   make system-test PI_HOST=10.232.1.1
#   make system-test-setup PI_HOST=10.232.1.1   (deploy + start, leave running)
#   make system-test-teardown PI_HOST=10.232.1.1 (stop + clean up)

SYSTEST_REMOTE_DIR = ~/wardnet-system-tests

# Cross-compile both wardnetd and wardnet-test-agent for the Pi.
build-system-test:
	cd $(DAEMON_DIR) && cargo build --release --target $(PI_TARGET) -p wardnetd -p wardnet-test-agent

# Deploy binaries and test infrastructure to the Pi, then start the environment.
system-test-setup: build-system-test
	@test -n "$(PI_HOST)" || { echo "Error: PI_HOST is required. Usage: make system-test-setup PI_HOST=<ip>"; exit 1; }
	@echo "Deploying system test environment to $(PI_REMOTE)..."
	ssh $(PI_REMOTE) 'mkdir -p $(SYSTEST_REMOTE_DIR)/fixtures'
	scp $(DAEMON_DIR)/target/$(PI_TARGET)/release/wardnetd $(PI_REMOTE):$(SYSTEST_REMOTE_DIR)/
	scp $(DAEMON_DIR)/target/$(PI_TARGET)/release/wardnet-test-agent $(PI_REMOTE):$(SYSTEST_REMOTE_DIR)/
	scp $(SYSTEST_DIR)/run-tests.sh $(PI_REMOTE):$(SYSTEST_REMOTE_DIR)/
	scp $(SYSTEST_DIR)/compose.yaml $(PI_REMOTE):$(SYSTEST_REMOTE_DIR)/
	scp $(SYSTEST_DIR)/wardnet-test.env $(PI_REMOTE):$(SYSTEST_REMOTE_DIR)/
	ssh $(PI_REMOTE) 'chmod +x $(SYSTEST_REMOTE_DIR)/run-tests.sh'
	ssh $(PI_REMOTE) 'sudo WARDNETD_BIN=$(SYSTEST_REMOTE_DIR)/wardnetd TEST_AGENT_BIN=$(SYSTEST_REMOTE_DIR)/wardnet-test-agent $(SYSTEST_REMOTE_DIR)/run-tests.sh setup'
	@echo ""
	@echo "Test environment is up. Run tests with:"
	@echo "  cd $(SYSTEST_DIR) && WARDNET_PI_IP=$(PI_HOST) yarn test"

# Stop the test environment and clean up on the Pi.
system-test-teardown:
	@test -n "$(PI_HOST)" || { echo "Error: PI_HOST is required"; exit 1; }
	ssh $(PI_REMOTE) 'sudo $(SYSTEST_REMOTE_DIR)/run-tests.sh teardown'

# Full workflow: build, deploy, setup, run tests, teardown.
system-test: build-system-test
	@test -n "$(PI_HOST)" || { echo "Error: PI_HOST is required. Usage: make system-test PI_HOST=<ip>"; exit 1; }
	@echo "=== Deploying to $(PI_REMOTE) ==="
	ssh $(PI_REMOTE) 'mkdir -p $(SYSTEST_REMOTE_DIR)/fixtures'
	scp $(DAEMON_DIR)/target/$(PI_TARGET)/release/wardnetd $(PI_REMOTE):$(SYSTEST_REMOTE_DIR)/
	scp $(DAEMON_DIR)/target/$(PI_TARGET)/release/wardnet-test-agent $(PI_REMOTE):$(SYSTEST_REMOTE_DIR)/
	scp $(SYSTEST_DIR)/run-tests.sh $(PI_REMOTE):$(SYSTEST_REMOTE_DIR)/
	scp $(SYSTEST_DIR)/compose.yaml $(PI_REMOTE):$(SYSTEST_REMOTE_DIR)/
	scp $(SYSTEST_DIR)/wardnet-test.env $(PI_REMOTE):$(SYSTEST_REMOTE_DIR)/
	ssh $(PI_REMOTE) 'chmod +x $(SYSTEST_REMOTE_DIR)/run-tests.sh'
	@echo "=== Starting test environment ==="
	ssh $(PI_REMOTE) 'sudo WARDNETD_BIN=$(SYSTEST_REMOTE_DIR)/wardnetd TEST_AGENT_BIN=$(SYSTEST_REMOTE_DIR)/wardnet-test-agent $(SYSTEST_REMOTE_DIR)/run-tests.sh setup'
	@echo "=== Installing test dependencies ==="
	cd $(SYSTEST_DIR) && yarn install
	@echo "=== Running tests ==="
	@cd $(SYSTEST_DIR) && WARDNET_PI_IP=$(PI_HOST) yarn test; \
		TEST_EXIT=$$?; \
		echo "=== Tearing down ==="; \
		ssh $(PI_REMOTE) 'sudo $(SYSTEST_REMOTE_DIR)/run-tests.sh teardown'; \
		exit $$TEST_EXIT

# ---------- Utilities ----------

clean:
	cd $(DAEMON_DIR) && cargo clean
	rm -rf $(WEBUI_DIR)/dist $(WEBUI_DIR)/node_modules/.cache $(SDK_DIR)/dist $(SITE_DIR)/dist

help:
	@echo "Wardnet build targets:"
	@echo ""
	@echo "  init           Install all dev dependencies (Rust target, cross-linker, yarn)"
	@echo ""
	@echo "  build          Build web UI + daemon (host target)"
	@echo "  build-web      Build web UI (depends on SDK check)"
	@echo "  build-daemon   Build daemon for host target"
	@echo "  build-pi       Cross-compile daemon for Pi (aarch64-linux-gnu)"
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
	@echo ""
	@echo "  run-pi         Build, deploy, and run on Pi via SSH (interactive)"
	@echo "                 Deletes the database by default for a clean start."
	@echo "                 make run-pi PI_HOST=10.232.1.1 PI_USER=pgomes PI_LAN_IF=end0"
	@echo "                 make run-pi ... RESUME=true              (keep existing database)"
	@echo "                 make run-pi ... OTEL=true                (auto-detect local IP)"
	@echo "                 make run-pi ... OTEL=true OTEL_HOST=10.232.1.189  (explicit)"
	@echo ""
	@echo "  system-test    Full system tests: build, deploy to Pi, run, teardown"
	@echo "                 make system-test PI_HOST=10.232.1.1"
	@echo "  system-test-setup    Deploy and start test environment (leave running)"
	@echo "  system-test-teardown Stop test environment on Pi"
	@echo ""
	@echo "  sync-version   Propagate ./VERSION into daemon Cargo.toml + package.json files"
	@echo "  check-version  Verify all versioned files match ./VERSION (CI gate)"
	@echo ""
	@echo "  clean          Clean all build artifacts"
