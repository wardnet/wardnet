# Release signing keys

This directory holds the **public** minisign key used to verify Wardnet release artefacts.

- [`wardnet-release.pub`](wardnet-release.pub) — public verification key. Committed to the repo, embedded into the `wardnetd` binary at compile time, and distributed alongside every release for manual verification.

The **private** signing key is never committed. It lives as a pair of GitHub Actions secrets on the `wardnet/wardnet` repository:

| Secret name                    | Content                                                                 |
| ------------------------------ | ----------------------------------------------------------------------- |
| `WARDNET_MINISIGN_KEY`         | The password-protected `minisign.key` file, base64-encoded              |
| `WARDNET_MINISIGN_PASSWORD`    | The password that decrypts the key                                      |

The release workflow (`.github/workflows/release.yml`) pulls both secrets at build time, signs each tarball, and destroys the decoded key file before the job finishes.

## One-time setup — generating the keypair

Run this on a trusted machine (your laptop), **offline is fine**:

```sh
# Install minisign if you don't already have it.
#   macOS:   brew install minisign
#   Debian:  sudo apt-get install minisign

# Generate a new keypair. minisign will prompt for a password — pick a strong one
# and record it somewhere safe (you'll need it for the GitHub secret).
minisign -G -p wardnet-release.pub -s wardnet-release.key
```

This produces two files:

- `wardnet-release.pub` — public key, 64 bytes, safe to commit.
- `wardnet-release.key` — private key, password-encrypted. **Do not commit this file anywhere, ever.**

## Uploading the secrets

```sh
# 1. Commit the public key to the repo.
mv wardnet-release.pub deploy/keys/wardnet-release.pub
git add deploy/keys/wardnet-release.pub
git commit -m "chore: add release signing public key"

# 2. Set the GitHub Actions secrets. `gh` reads from stdin so the key never
#    hits your shell history.
gh secret set WARDNET_MINISIGN_KEY --repo wardnet/wardnet < <(base64 < wardnet-release.key)
gh secret set WARDNET_MINISIGN_PASSWORD --repo wardnet/wardnet
#    ^ gh prompts; paste the password

# 3. After you've verified the next release signs & verifies correctly, shred
#    the local private key. The only copies that should remain are the encrypted
#    GitHub secret and whatever offline backup you keep (e.g. 1Password vault).
shred -u wardnet-release.key
```

## Verifying a release manually

See [`SECURITY.md`](../../SECURITY.md#verifying-a-release-manually).

## Rotating the key

Key rotation is a two-release process so existing daemons can validate the transition:

1. Generate a new keypair locally.
2. Ship release **N** signed with the **old** key, carrying the new public key in the binary. Daemons self-update to N — they still trust the old key for this release, and they now bundle the new key for future releases.
3. Ship release **N+1** signed with the **new** key. Daemons on N verify it against the new key they just picked up.
4. After enough daemons have reached N+1, destroy the old private key.

Rotation is v3 scope and not expected in the near term.
