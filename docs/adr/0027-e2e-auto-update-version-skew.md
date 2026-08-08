---
status: accepted
date: 2026-08-08
issue: "#319 (E2E — auto-update N-1 → N transition), follows #309 / #318"
---

# ADR: The auto-update e2e synthesises its version skew from one source tree

---

## Context

Issue #319 asked for end-to-end coverage of a real version transition:
bring up an N-1 `wardnetd`, point it at a fake release server serving the
N tarball, and drive the auto-update API through to a clean swap. The
proposed shape was to download a pinned previous release
(`wardnetd-<prev-ver>-<arch>.tar.gz`) from GitHub releases at job start
and install it into the test image.

That cannot work, for a reason that is invisible unless you go looking
for it.

**Both trust anchors are compile-time.** The daemon embeds its release
verification key via `include_str!` (`wardnetd-services/src/update/mod.rs`,
`EMBEDDED_PUBLIC_KEY`), and `wardnet-postupgrade-runner` embeds the same
key independently. The runner's check is the load-bearing one: it
re-verifies the staged tarball as root before renaming anything into
`/usr/local/bin/`, and it does so **unconditionally** — `[update]
require_signature = false` does not bypass it, by design (see
`swap.rs`'s module docs on the trust boundary).

So a genuinely-released N-1 binary trusts exactly one key: the real
release key. To hand it an N tarball it would accept, we would need the
real private half. We do not have it in CI, and putting it there would
defeat the point of having it.

## Decision

Synthesise the version skew from a single source tree.

`source/daemon/Dockerfile.test` compiles `wardnetd` twice. The first
build is installed live, as before. The second is built with
`WARDNET_VERSION_OVERRIDE=<E2E_RELEASE_VERSION>`, packaged into a release
tarball with a `.sha256` sidecar and a `.minisig`, and published by the
`update_release_server` compose service. Both builds — and the runner —
are keyed to an ephemeral minisign keypair generated during the image
build.

Making the daemon side honour that ephemeral key required a new seam:
`crates/wardnetd-services/build.rs` now resolves
`WARDNET_RELEASE_PUBKEY_PATH`, mirroring the override
`wardnet-postupgrade-runner/build.rs` has had since the post-upgrade
pipeline landed. `Dockerfile.test` is its only consumer; production
builds leave it unset and embed `deploy/keys/wardnet-release.pub`.

## Considered options

**Build the previous release tag from source.** Fetch the previous
release's tree during the Docker build and compile it with the ephemeral
key. This is the only option that delivers true *code-level* skew — old
code against a new tarball, the class of regression #309 actually was.
Rejected on cost and fragility: a second tree shares no cargo cache with
the first, roughly doubling an already 7–10 minute image build, and an
older tree can fail to compile at all once the toolchain moves under it.
The e2e suite would then be red for a reason unrelated to the change
under test.

**Set `require_signature = false` in the e2e TOML.** Avoids touching
production code. Rejected because it buys less than it looks like it
does: the daemon would skip its own verify entirely, *and*
`FsBinaryApplier::verify_postupgrade` would still run against the real
key and still fail, forcing the fixture tarball to omit the post-upgrade
payload pair. That silently disables the post-upgrade half of the
pipeline — the suite would be quieter, not more honest. The build seam
keeps `require_signature = true` and keeps both verifies exercised.

## Consequences

**What this proves.** The full mechanism across a version transition:
manifest resolution, asset download, sha256 and minisign verification,
staging, the daemon's self-restart via `shutdown_token.cancel()`, systemd
re-running the post-upgrade oneshot, the privileged re-verify and rename,
`<live>.old` preservation, and the rollback path back. Plus the negative
case — that the runner refuses bytes it cannot verify and blocks daemon
startup rather than running an unverified binary.

**What it does not prove.** That an *older build* of the daemon handles a
*newer* tarball correctly. Both binaries here come from the same commit;
only their embedded version strings differ. A regression that only
manifests as old-code-meets-new-payload would still slip through. That
is the honest limit of this design, and the reason to keep the
`Considered options` note above rather than delete it — if the cost
calculus changes (a cached previous-release build, say), the
true-skew option is the upgrade path.

**The seam is test-only.** `WARDNET_RELEASE_PUBKEY_PATH` has no
production consumer. It exists so the e2e image can trust a key it
generated, and it must always be set to the *same* key as
`WARDNET_POSTUPGRADE_PUBKEY_PATH` — the daemon verifies the tarball
before staging and the runner re-verifies the same bytes afterwards, so
a split between the two would either wedge every auto-update or break
ordinary boots.
