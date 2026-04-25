#!/bin/bash -eu
#
# Build all wardnet fuzz targets and copy them into $OUT for
# ClusterFuzzLite to run. Invoked by the base-builder-rust image.
#
# The fuzz workspace lives at source/daemon/fuzz and is explicitly
# excluded from the daemon workspace so `libfuzzer-sys` + nightly
# sanitizer rustflags never leak into normal `cargo build` paths.

cd "$SRC/wardnet/source/daemon/fuzz"

# ── Toolchain override ───────────────────────────────────────────
# The OSS-Fuzz base-builder-rust image sets `RUSTUP_TOOLCHAIN` as a
# container env var (currently nightly-2025-09-05 → rustc 1.91-nightly).
# That env var takes precedence over rust-toolchain.toml in every
# rustup proxy call, so the pinned nightly in this directory is
# silently ignored and builds fail with:
#
#   error: rustc 1.91.0-nightly is not supported by the following
#   packages: wardnet-common@0.2.0 requires rustc 1.95, ...
#
# Parse the channel out of rust-toolchain.toml (kept as the single
# source of truth), install it explicitly, then override the image's
# RUSTUP_TOOLCHAIN for the rest of the script so every nested
# cargo/rustc invocation picks up our pin.
TOOLCHAIN=$(awk -F '"' '/^channel/ {print $2}' rust-toolchain.toml)
rustup toolchain install "$TOOLCHAIN" --profile minimal --component rust-src
export RUSTUP_TOOLCHAIN="$TOOLCHAIN"

# ── Target-specific RUSTFLAGS ────────────────────────────────────
# cargo-fuzz sets sanitizer + coverage flags via global RUSTFLAGS,
# which also instruments proc-macro crates (sqlx-macros, etc.).
# Proc-macros are host-side compiler plugins loaded via dlopen —
# they don't link against the sanitizer runtime, so symbols like
# __sancov_lowest_stack are undefined at load time, failing the
# build.
#
# Fix: bypass `cargo fuzz build` and call `cargo build` directly,
# placing all instrumentation flags in the *target-specific*
# CARGO_TARGET_<triple>_RUSTFLAGS so they apply only to the fuzz
# binaries, not to proc-macros compiled for the host.
TARGET=x86_64-unknown-linux-gnu

TARGET_RUSTFLAGS=""
TARGET_RUSTFLAGS+=" -Cpasses=sancov-module"
TARGET_RUSTFLAGS+=" -Cllvm-args=-sanitizer-coverage-level=4"
TARGET_RUSTFLAGS+=" -Cllvm-args=-sanitizer-coverage-inline-8bit-counters"
TARGET_RUSTFLAGS+=" -Cllvm-args=-sanitizer-coverage-pc-table"
TARGET_RUSTFLAGS+=" -Cllvm-args=-sanitizer-coverage-trace-compares"
TARGET_RUSTFLAGS+=" --cfg fuzzing"
TARGET_RUSTFLAGS+=" -Cllvm-args=-simplifycfg-branch-fold-threshold=0"
TARGET_RUSTFLAGS+=" -Zsanitizer=address"
TARGET_RUSTFLAGS+=" -Cllvm-args=-sanitizer-coverage-stack-depth"
TARGET_RUSTFLAGS+=" -Ccodegen-units=1"
TARGET_RUSTFLAGS+=" -Cdebuginfo=1"
TARGET_RUSTFLAGS+=" -Cforce-frame-pointers"

export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="$TARGET_RUSTFLAGS"
unset RUSTFLAGS 2>/dev/null || true

export ASAN_OPTIONS="${ASAN_OPTIONS:+$ASAN_OPTIONS:}detect_odr_violation=0"

cargo build \
    --manifest-path Cargo.toml \
    --target "$TARGET" \
    --release \
    --config 'profile.release.debug="line-tables-only"' \
    --bins

FUZZ_TARGETS=(archiver_unpack sqlite_restore bundle_manifest)
for target in "${FUZZ_TARGETS[@]}"; do
    cp "target/${TARGET}/release/${target}" "$OUT/"
done
