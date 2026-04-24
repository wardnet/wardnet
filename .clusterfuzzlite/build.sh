#!/bin/bash -eu
#
# Build all wardnet fuzz targets and copy them into $OUT for
# ClusterFuzzLite to run. Invoked by the base-builder-rust image.
#
# The fuzz workspace lives at source/daemon/fuzz and is explicitly
# excluded from the daemon workspace so `libfuzzer-sys` + nightly
# sanitizer rustflags never leak into normal `cargo build` paths.

cd "$SRC/wardnet/source/daemon/fuzz"

# Force the pinned nightly in rust-toolchain.toml to be installed.
# The OSS-Fuzz base-builder-rust image ships with a pre-baked rustc
# (1.91-nightly at the current digest) that doesn't satisfy the daemon
# workspace's MSRV (1.94). `rustup show` from a directory containing
# rust-toolchain.toml triggers rustup to install the pinned toolchain
# when it's missing — single source of truth (the .toml), no version
# string duplicated here.
rustup show

cargo fuzz build -O

FUZZ_TARGETS=(archiver_unpack sqlite_restore bundle_manifest)
for target in "${FUZZ_TARGETS[@]}"; do
    cp "target/x86_64-unknown-linux-gnu/release/${target}" "$OUT/"
done
