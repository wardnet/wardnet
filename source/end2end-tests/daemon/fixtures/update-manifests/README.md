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
  `update_available` is `true` deterministically.
- `edge.json` — same placeholder as `stable.json`. Present so a box that
  somehow ends up on `edge` gets a 404-free fetch; the e2e suite never
  selects the channel (`allow_edge_channel` is left at its default
  `false`, and `update-status.spec.ts` asserts the 403).

## Why plain HTTP, not HTTPS

The daemon accepts any base URL — the fetch is a bare `reqwest::get`,
and authenticity in production comes from the minisign signature on the
downloaded asset, not from TLS on the manifest. Terminating TLS here
would mean minting a CA, baking it into the daemon image's trust store,
and keeping it in sync — all to exercise `reqwest`'s TLS stack rather
than any Wardnet code. The specs never install, so no asset is ever
fetched or verified.

## Assets

`asset_base_url` points back at this same server, but nothing under it
exists: `update-status.spec.ts` exercises `status` / `check` /
`updateConfig` only. A future install spec would need real tarball +
`.sha256` + `.minisig` fixtures and a daemon built with
`[update] require_signature = false` (or the image's ephemeral key).
