# ClusterFuzzLite build container.
#
# Inherits the OSS-Fuzz Rust base image (nightly toolchain + libFuzzer
# + sanitizer rustflags preconfigured). CFLite copies the repo into
# $SRC/wardnet and invokes build.sh at image-build time.
# Pinned by digest for Scorecard / supply-chain integrity. The
# OSS-Fuzz base images are rebuilt frequently; refresh this digest on
# a regular cadence when upstream ships a Rust toolchain bump worth
# picking up. Resolve a fresh digest with:
#   docker buildx imagetools inspect gcr.io/oss-fuzz-base/base-builder-rust
FROM gcr.io/oss-fuzz-base/base-builder-rust@sha256:253eff2ba145e0a2bf8fe7d98c289243f5ade48f85d929f561c44341cd80b642

COPY . $SRC/wardnet
COPY .clusterfuzzlite/build.sh $SRC/
WORKDIR $SRC/wardnet
