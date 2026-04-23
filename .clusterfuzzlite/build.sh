#!/bin/bash -eu
#
# Build all wardnet fuzz targets and copy them into $OUT for
# ClusterFuzzLite to run. Invoked by the base-builder-rust image.
#
# The fuzz workspace lives at source/daemon/fuzz and is explicitly
# excluded from the daemon workspace so `libfuzzer-sys` + nightly
# sanitizer rustflags never leak into normal `cargo build` paths.

cd "$SRC/wardnet/source/daemon/fuzz"

cargo fuzz build -O

FUZZ_TARGETS=(archiver_unpack sqlite_restore bundle_manifest)
for target in "${FUZZ_TARGETS[@]}"; do
    cp "target/x86_64-unknown-linux-gnu/release/${target}" "$OUT/"
done
