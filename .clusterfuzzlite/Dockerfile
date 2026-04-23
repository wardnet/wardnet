# ClusterFuzzLite build container.
#
# Inherits the OSS-Fuzz Rust base image (nightly toolchain + libFuzzer
# + sanitizer rustflags preconfigured). CFLite copies the repo into
# $SRC/wardnet and invokes build.sh at image-build time.
FROM gcr.io/oss-fuzz-base/base-builder-rust

COPY . $SRC/wardnet
COPY .clusterfuzzlite/build.sh $SRC/
WORKDIR $SRC/wardnet
