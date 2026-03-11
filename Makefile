# Wardnet Makefile
# Unified build commands for local development and CI.

# ---------- Configuration ----------

PI_TARGET    := aarch64-unknown-linux-gnu
DAEMON_DIR   := source/daemon
SDK_DIR      := source/sdk/wardnet-js
WEBUI_DIR    := source/web-ui
SITE_DIR     := source/site

# Override on CLI: make deploy PI_HOST=wardnet.local
PI_HOST      ?= gateway
PI_USER      ?= wardnet
PI_BIN_DIR   ?= /usr/local/bin
PI_LAN_IF    ?= eth0
PI_REMOTE     = $(PI_USER)@$(PI_HOST)
OTEL         ?= false
OTEL_HOST    ?=

# ---------- Phony targets ----------

.PHONY: all init build build-daemon build-sdk build-web build-site build-pi \
        check check-sdk check-web check-site check-daemon fmt clippy test \
        deploy run-pi clean help

all: build

# ---------- Dev environment setup ----------

init:
	@echo "Installing development dependencies..."
	@command -v rustup >/dev/null || { echo "Error: rustup not found. Install from https://rustup.rs"; exit 1; }
	@command -v node >/dev/null || { echo "Error: node not found. Install Node.js 25+"; exit 1; }
	@command -v yarn >/dev/null || { echo "Error: yarn not found. Run: corepack enable"; exit 1; }
	rustup target add $(PI_TARGET)
	@if [ "$$(uname)" = "Darwin" ]; then \
		echo "Installing macOS cross-compilation toolchain..."; \
		brew tap messense/macos-cross-toolchains; \
		brew install aarch64-unknown-linux-gnu; \
	elif [ "$$(uname)" = "Linux" ]; then \
		echo "Installing Linux cross-compilation toolchain..."; \
		sudo apt-get update && sudo apt-get install -y gcc-aarch64-linux-gnu; \
	fi
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

build-daemon:
	cd $(DAEMON_DIR) && cargo build --release

build-pi:
	cd $(DAEMON_DIR) && cargo build --release --target $(PI_TARGET) -p wardnetd

check-daemon:
	cd $(DAEMON_DIR) && cargo fmt --check
	cd $(DAEMON_DIR) && cargo clippy --all-targets -- -D warnings
	cd $(DAEMON_DIR) && cargo test --workspace

# ---------- Compound targets ----------

build: build-web build-daemon

check: check-web check-site check-daemon

# ---------- Deploy & Run ----------

RESUME ?= false

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
	ssh $(PI_REMOTE) 'sudo setcap "cap_net_admin,cap_net_raw,cap_net_bind_service=eip" ~/wardnetd' && \
	ssh -t $(PI_REMOTE) '~/wardnetd --config ~/wardnet-dev/wardnet.toml --verbose'

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
	@echo "  check-daemon   Format + clippy + tests for daemon"
	@echo ""
	@echo "  run-pi         Build, deploy, and run on Pi via SSH (interactive)"
	@echo "                 Deletes the database by default for a clean start."
	@echo "                 make run-pi PI_HOST=10.232.1.1 PI_USER=pgomes PI_LAN_IF=end0"
	@echo "                 make run-pi ... RESUME=true              (keep existing database)"
	@echo "                 make run-pi ... OTEL=true                (auto-detect local IP)"
	@echo "                 make run-pi ... OTEL=true OTEL_HOST=10.232.1.189  (explicit)"
	@echo ""
	@echo "  clean          Clean all build artifacts"
