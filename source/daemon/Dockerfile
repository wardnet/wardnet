# syntax=docker/dockerfile:1
#
# wardnetd production image.
#
# Downloads a signed GitHub release and installs it via deploy/install.sh,
# the same script used for bare-metal installs. Systemd runs as PID 1 so
# auto-update and the crash-loop rollback (OnFailure=wardnetd-rollback.service)
# both work end-to-end inside the container — identical to bare metal. The
# only difference is that a fresh `docker run` from this image always starts
# at the baked-in version; `docker restart` preserves any auto-updated binary.
#
# Build context: repo root
#   docker build -f source/daemon/Dockerfile .
#   docker build --build-arg WARDNET_VERSION=0.2.0 -f source/daemon/Dockerfile .
#   make image
#   make image VERSION=0.2.0
#
# For the test image that builds from source and adds wardnet-test-agent,
# see source/daemon/Dockerfile.test.
#
# Required runtime flags (see source/daemon/examples/docker-compose.yaml):
#   --cap-add NET_ADMIN --cap-add NET_RAW
#   --device /dev/net/tun
#   --sysctl net.ipv4.ip_forward=1
#   --tmpfs /run --tmpfs /run/lock

# Pinned by digest for Scorecard / supply-chain integrity.
# Dependabot (.github/dependabot.yml) watches this and opens PRs
# when Debian publishes a new bookworm-slim image.
FROM debian:bookworm-slim@sha256:f9c6a2fd2ddbc23e336b6257a5245e31f996953ef06cd13a59fa0a1df2d5c252

# Runtime dependencies that stay in the final image.
# wget is kept for the HEALTHCHECK; curl and minisign are purged after install.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        systemd \
        systemd-sysv \
        iproute2 \
        nftables \
        wireguard-tools \
        ca-certificates \
        wget \
        curl \
        minisign \
    && rm -rf /var/lib/apt/lists/* \
    # Remove default-enabled systemd units that cannot work inside a container
    # (hardware enumeration, real-disk remounts, etc.). wardnetd is re-enabled
    # by install.sh below.
    && rm -f \
        /lib/systemd/system/multi-user.target.wants/* \
        /etc/systemd/system/*.wants/* \
        /lib/systemd/system/local-fs.target.wants/* \
        /lib/systemd/system/sockets.target.wants/*udev* \
        /lib/systemd/system/sockets.target.wants/*initctl* \
        /lib/systemd/system/sysinit.target.wants/* \
        /lib/systemd/system/systemd-remount-fs.service

# wardnet release version to install (no `v` prefix, e.g. "0.2.0").
# "latest" resolves to the most recent stable release via the GitHub Releases API.
ARG WARDNET_VERSION=latest

# The deploy/ directory is copied so install.sh can find the service unit
# files locally, avoiding a second fetch from GitHub raw during the build.
COPY deploy/ /tmp/wardnet-deploy/

# Download the signed release bundle, run install.sh in container mode,
# then purge the build-time tools. Everything in one layer so the downloaded
# tarballs don't persist in intermediate layers.
#
# --container-mode tells install.sh to skip systemctl daemon-reload, start,
# and restart (systemd is not running during `docker build`). It still calls
# `systemctl enable wardnetd` which creates the WantedBy symlink so systemd
# starts the service when it initialises as PID 1 at container runtime.
#
# In offline mode (--from) install.sh does NOT need curl — only minisign,
# tar, sha256sum, and systemctl. curl is used only for the bundle download
# step below, after which it is purged from the image.
# Run as a bash heredoc so `set -o pipefail` works. /bin/sh on debian-slim
# is dash, which doesn't support pipefail; the SHELL directive is silently
# ignored by podman's default OCI format. Heredocs with an explicit
# interpreter work across docker, buildx, and podman.
# `<<"EOF"` (quoted delimiter) prevents Dockerfile-level variable expansion;
# the heredoc body is passed to bash verbatim and bash expands $VARS itself.
RUN <<"EOF" bash
set -euo pipefail
# uname -m returns aarch64 or x86_64, matching release asset filenames.
ARCH="$(uname -m)"
if [ "$WARDNET_VERSION" = "latest" ]; then
    VERSION="$(curl -fsSL https://api.github.com/repos/wardnet/wardnet/releases/latest \
        | grep '"tag_name"' | head -1 \
        | sed 's/.*"v\([^"]*\)".*/\1/')"
else
    VERSION="$WARDNET_VERSION"
fi
echo "wardnetd: installing v${VERSION} (${ARCH})"
BUNDLE=/tmp/wardnet-bundle
mkdir "$BUNDLE"
BASE="https://github.com/wardnet/wardnet/releases/download/v${VERSION}"
TARBALL="wardnetd-${VERSION}-${ARCH}.tar.gz"
curl -fsSL "${BASE}/${TARBALL}"         -o "${BUNDLE}/${TARBALL}"
curl -fsSL "${BASE}/${TARBALL}.sha256"  -o "${BUNDLE}/${TARBALL}.sha256"
curl -fsSL "${BASE}/${TARBALL}.minisig" -o "${BUNDLE}/${TARBALL}.minisig"
LAN_INTERFACE=eth0 bash /tmp/wardnet-deploy/install.sh --from "$BUNDLE" --container-mode
apt-get purge -y curl minisign
apt-get autoremove -y
rm -rf /var/lib/apt/lists/* "$BUNDLE" /tmp/wardnet-deploy
EOF

# Container-specific service drop-in.
# ProtectSystem=strict and PrivateTmp=yes set up mount namespaces that require
# CAP_SYS_ADMIN, which Docker strips from its default capability set.
# Everything else — capabilities, NoNewPrivileges, auto-update, and the
# crash-loop rollback via OnFailure= — is left exactly as-is.
RUN mkdir -p /etc/systemd/system/wardnetd.service.d \
    && printf '[Service]\nProtectSystem=no\nPrivateTmp=no\n' \
        > /etc/systemd/system/wardnetd.service.d/container.conf \
    && chmod 0644 /etc/systemd/system/wardnetd.service.d/container.conf

# DNS (53/tcp+udp), DHCP (67/udp), daemon API + embedded web UI (7411),
# dev-proxy port used by wardnetd-mock (7412)
EXPOSE 53/udp 53/tcp 67/udp 7411/tcp 7412/tcp

# systemd's graceful-shutdown signal when running as PID 1 inside a container
STOPSIGNAL SIGRTMIN+3

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD wget -qO- http://127.0.0.1:7411/api/info > /dev/null || exit 1

CMD ["/lib/systemd/systemd"]
