#!/bin/bash -eu
#
# Build all wardnet fuzz targets and copy them into $OUT for
# ClusterFuzzLite to run. Invoked by the base-builder-rust image.
#
# The fuzz workspace lives at source/daemon/fuzz and is explicitly
# excluded from the daemon workspace so `libfuzzer-sys` + nightly
# sanitizer rustflags never leak into normal `cargo build` paths.

cd "$SRC/wardnet/source/daemon/fuzz"

# The OSS-Fuzz base-builder-rust image sets `RUSTUP_TOOLCHAIN` as a
# container env var (currently nightly-2025-09-05 → rustc 1.91-nightly).
# That env var takes precedence over rust-toolchain.toml in every
# rustup proxy call, so the pinned nightly in this directory is
# silently ignored and builds fail with:
#
#   error: rustc 1.91.0-nightly is not supported by the following
#   packages: wardnet-common@0.2.0 requires rustc 1.94, ...
#
# Parse the channel out of rust-toolchain.toml (kept as the single
# source of truth), install it explicitly, then override the image's
# RUSTUP_TOOLCHAIN for the rest of the script so cargo-fuzz and every
# nested cargo/rustc invocation pick up our pin.
TOOLCHAIN=$(awk -F '"' '/^channel/ {print $2}' rust-toolchain.toml)
rustup toolchain install "$TOOLCHAIN" --profile minimal --component rust-src
export RUSTUP_TOOLCHAIN="$TOOLCHAIN"

cargo fuzz build -O

FUZZ_TARGETS=(archiver_unpack sqlite_restore bundle_manifest)
for target in "${FUZZ_TARGETS[@]}"; do
    cp "target/x86_64-unknown-linux-gnu/release/${target}" "$OUT/"
done
