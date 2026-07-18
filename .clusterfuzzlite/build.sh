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

# ── Custom fuzz target triple (fix for #587) ─────────────────────
# On this image the host triple equals the fuzz target triple
# (x86_64-unknown-linux-gnu), which makes it impossible to scope
# instrumentation to the fuzz binaries: every per-target knob
# (CARGO_TARGET_*_RUSTFLAGS, CFLAGS_<triple>, [target.<triple>]
# config) hits host-side builds (proc-macros, build scripts) too,
# because they share the same triple name.
#
# The actual #587 failure enters through C code: the image's global
# CFLAGS carry `-fsanitize=address ... -fsanitize=fuzzer-no-link`,
# the cc crate applies them to build-script C compiles for BOTH
# host and target, and rustc bundles native static libs wholesale
# into dylib crates — so the host sqlx-macros proc-macro .so ends up
# containing sancov-instrumented sqlite3 objects (via libsqlite3-sys)
# and fails to dlopen inside rustc with
# `undefined symbol: __sancov_lowest_stack`. RUSTFLAGS-level fixes
# can never touch that path, which is why every attempt recorded in
# #587 failed.
#
# Fix: build the fuzz binaries for a *custom* target triple that is
# byte-for-byte the x86_64-unknown-linux-gnu spec under a different
# name. The binaries stay ABI-identical to plain gnu (they run
# unmodified in the CFLite runner), but cargo/cc now see distinct
# host and target platforms, so instrumentation — Rust flags AND C
# flags — can be scoped to the fuzz target alone while host builds
# stay pristine. (x86_64-unknown-linux-musl was evaluated instead
# and is a dead end: rustc rejects `-Zsanitizer=address` with
# statically linked musl, and a dynamically linked musl binary won't
# run on the glibc runner.)
FUZZ_TRIPLE=x86_64-wardnet-linux-gnu
rustc -Zunstable-options --print target-spec-json \
    --target x86_64-unknown-linux-gnu > "/tmp/$FUZZ_TRIPLE.json"

# std for the custom triple is built from source via -Zbuild-std
# (rustc refuses the prebuilt gnu rlibs — E0461 — because rlib
# metadata embeds the triple name). That also gets us an
# ASan-instrumented std, same as `cargo fuzz` does. The prebuilt
# sanitizer runtime staticlibs carry no triple stamp though, so
# stage just those into the custom triple's rustlib dir where rustc
# looks them up.
SYSROOT=$(rustc --print sysroot)
mkdir -p "$SYSROOT/lib/rustlib/$FUZZ_TRIPLE/lib"
ln -sf "$SYSROOT/lib/rustlib/x86_64-unknown-linux-gnu/lib/"librustc-*_rt.*.a \
    "$SYSROOT/lib/rustlib/$FUZZ_TRIPLE/lib/"

# ── Scope the image's sanitizer C flags to the fuzz target ───────
# Keep the poisoned CFLAGS/CXXFLAGS for C code that is linked into
# the fuzz binaries (bundled sqlite3 — ASan there is a feature), but
# scrub them from the plain vars the cc crate uses for host builds,
# so proc-macro/build-script C stays uninstrumented and loadable.
export "CFLAGS_${FUZZ_TRIPLE//-/_}=${CFLAGS:-}"
export "CXXFLAGS_${FUZZ_TRIPLE//-/_}=${CXXFLAGS:-}"
strip_sanitize() {
    local out="" f
    for f in $1; do
        case "$f" in
        -fsanitize=*|-fsanitize-*|-fprofile-*|-fcoverage-*) ;;
        *) out+=" $f" ;;
        esac
    done
    echo "$out"
}
export CFLAGS="$(strip_sanitize "${CFLAGS:-}")"
export CXXFLAGS="$(strip_sanitize "${CXXFLAGS:-}")"

# ── Target-specific RUSTFLAGS ────────────────────────────────────
# cargo-fuzz sets sanitizer + coverage flags via global RUSTFLAGS,
# which also instruments proc-macro crates. Bypass `cargo fuzz build`
# and call `cargo build` directly, placing all instrumentation flags
# in the *target-specific* CARGO_TARGET_<triple>_RUSTFLAGS so they
# apply only to the fuzz binaries, not to host-side compiles.
#
# The flags branch on $SANITIZER (set by the CFLite action):
# `address` for the batch/prune jobs, `coverage` for the weekly
# coverage report, which needs source-based coverage instrumentation
# instead of sancov/ASan (mirrors upstream base-builder `compile`).
case "${SANITIZER:-address}" in
coverage)
    # -Cinstrument-coverage normally injects the `profiler_builtins`
    # runtime crate, which -Zbuild-std cannot provide (instrumented
    # `core` would need it before it can be built). Disable the crate
    # injection and satisfy the __llvm_profile_* references at link
    # time with compiler-rt's C profile runtime instead — same
    # runtime, minus the bootstrap cycle.
    PROFILE_RT=$("${CC:-clang}" -print-file-name=libclang_rt.profile-x86_64.a)
    if [ ! -f "$PROFILE_RT" ]; then
        PROFILE_RT=$("${CC:-clang}" -print-file-name=libclang_rt.profile.a)
    fi
    TARGET_RUSTFLAGS=""
    TARGET_RUSTFLAGS+=" --cfg fuzzing"
    TARGET_RUSTFLAGS+=" -Cinstrument-coverage"
    TARGET_RUSTFLAGS+=" -Zno-profiler-runtime"
    TARGET_RUSTFLAGS+=" -Clink-arg=$PROFILE_RT"
    TARGET_RUSTFLAGS+=" -Cdebuginfo=1"
    TARGET_RUSTFLAGS+=" -Cforce-frame-pointers"
    ;;
*)
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
    ;;
esac

export CARGO_TARGET_X86_64_WARDNET_LINUX_GNU_RUSTFLAGS="$TARGET_RUSTFLAGS"
unset RUSTFLAGS 2>/dev/null || true

# Point cargo/cc at the host toolchain for the custom triple — the
# cc crate would otherwise hunt for an x86_64-wardnet-linux-gnu-gcc
# cross-compiler. clang as linker driver mirrors the image's
# CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER and is what locates
# the image's libc++ / prebuilt libFuzzer for libfuzzer-sys.
export CARGO_TARGET_X86_64_WARDNET_LINUX_GNU_LINKER="${CC:-clang}"
export "CC_${FUZZ_TRIPLE//-/_}=${CC:-clang}"
export "CXX_${FUZZ_TRIPLE//-/_}=${CXX:-clang++}"
export "AR_${FUZZ_TRIPLE//-/_}=ar"

export ASAN_OPTIONS="${ASAN_OPTIONS:+$ASAN_OPTIONS:}detect_odr_violation=0"

cargo build \
    -Zjson-target-spec \
    -Zbuild-std \
    --manifest-path Cargo.toml \
    --target "/tmp/$FUZZ_TRIPLE.json" \
    --release \
    --config 'profile.release.debug="line-tables-only"' \
    --bins

FUZZ_TARGETS=(archiver_unpack sqlite_restore bundle_manifest)
for target in "${FUZZ_TARGETS[@]}"; do
    cp "target/${FUZZ_TRIPLE}/release/${target}" "$OUT/"
done
