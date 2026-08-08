# Update manifest fixtures

Static release manifests served by the `update_manifest_server`
compose service (busybox httpd on `10.92.0.56:80`, `wardnet_wan`).
The daemon's `[update] manifest_base_url` is rewritten to point here
by the `wardnetd` service's entrypoint, so
`UpdateService::check()` fetches `<base>/<channel>.json` from this
directory instead of `https://releases.wardnet.network`.

The shape mirrors `source/marketing-site/scripts/generate-release-manifests.ts`
— the daemon only reads `version`, `asset_base_url`, `binary`,
`published_at`, and `notes_url` (see
`crates/wardnetd-services/src/update/manifest.rs`), but the fixtures
carry the full field set so a drift in the generator is visible here.

## Files

- `stable.json` — the generator's **empty placeholder** (`version: ""`,
  `binary: null`). The daemon treats that as "no release published on
  this channel" and reports `latest_version: null`,
  `update_available: false`.
- `beta.json` — a **fixture release** at version `9999.12.31`, chosen to
  outrank any version this repo will ever build so
  `update_available` is `true` deterministically. Unlike the other two
  this one is **installable**: its `asset_base_url` points at
  `update_release_server` (`10.92.0.57`), which publishes a real signed
  tarball for it. See "Assets" below.
- `edge.json` — same placeholder as `stable.json`. Present so a box that
  somehow ends up on `edge` gets a 404-free fetch; the e2e suite never
  selects the channel (`allow_edge_channel` is left at its default
  `false`, and `update-status.spec.ts` asserts the 403).

## Why plain HTTP, not HTTPS

The daemon accepts any base URL — the fetch is a bare `reqwest::get`,
and authenticity comes from the minisign signature on the downloaded
asset, not from TLS on the manifest. Terminating TLS here would mean
minting a CA, baking it into the daemon image's trust store, and
keeping it in sync — all to exercise `reqwest`'s TLS stack rather than
any Wardnet code. The signature check that *does* matter is exercised
for real (see below), so nothing is lost.

## Assets

`stable.json` / `edge.json` are placeholders with no assets behind them.

`beta.json` is backed by the `update_release_server` compose service on
`10.92.0.57`, which serves the tarball, `.sha256`, and `.minisig` that
`auto-update-swap.spec.ts` installs. Those are **build outputs, not
fixtures**: `source/daemon/Dockerfile.test` compiles a second `wardnetd`
at `9999.12.31` and signs it with the image's ephemeral minisign key, so
the daemon (built with `WARDNET_RELEASE_PUBKEY_PATH` pointed at the same
key) accepts it with `[update] require_signature` left at `true`. See
`docs/adr/0027-e2e-auto-update-version-skew.md`.

Two things to know before editing `beta.json`:

- **The version must match the tarball the image built.** The daemon
  derives the asset filename itself, as
  `wardnetd-<manifest version>-<arch>.tar.gz` (`HttpsManifestSource::
  tarball_name`), so a bumped version here without a matching
  `E2E_RELEASE_VERSION` build arg in `Dockerfile.test` fails as a 404
  during the download phase.
- **`binary.name` is decorative.** The daemon parses it and ignores it
  (`ManifestBinary` is `#[allow(dead_code)]`); the arch in the real
  request comes from the daemon's own target. That is why this file can
  say `aarch64` while CI happily fetches the `x86_64` asset.

## Not covered

The background `UpdateRunner` auto-install trigger
(`auto_install_if_due`) is not exercised end-to-end. Driving it needs a
`check_interval_secs` override — the default is 6 hours — and leaving
`auto_update_enabled` on in a shared container would let the runner fire
inside an unrelated spec's window. Tracked as a follow-up to #319.
