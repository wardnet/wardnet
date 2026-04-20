# Security Policy

## Supported Versions

Wardnet is in active development (Phase 1 MVP). Only the latest code on the `main` branch and the most recent tagged release receive security fixes.

## Reporting a Vulnerability

Please **do not** report security vulnerabilities through public GitHub issues, as this exposes the vulnerability to everyone before a fix is available.

Instead, use GitHub's private vulnerability reporting:

**[Report a vulnerability](https://github.com/wardnet/wardnet/security/advisories/new)**

You can expect:
- Acknowledgment within **48 hours**
- A status update within **7 days**
- A fix or mitigation plan for critical issues within **14 days**

## Scope

This policy covers the `wardnetd` daemon, the `wctl` CLI, and the web UI served by the daemon. Third-party dependencies are tracked via Dependabot; if you find a vulnerability in a dependency that is not yet covered by an advisory, please report it upstream to the relevant project as well.

## Release signing and update trust

Release artefacts are published on [GitHub Releases](https://github.com/wardnet/wardnet/releases) and signed with [minisign](https://jedisct1.github.io/minisign/). Every published release ships three files per target:

- `wardnetd-<version>-<target>.tar.gz` — the tarball containing the stripped `wardnetd` binary
- `wardnetd-<version>-<target>.tar.gz.sha256` — SHA-256 digest, for integrity verification
- `wardnetd-<version>-<target>.tar.gz.minisig` — minisign signature, for authenticity verification

The public verification key is committed to the repo at [`deploy/keys/wardnet-release.pub`](deploy/keys/wardnet-release.pub) and embedded into the daemon at compile time. The auto-update subsystem refuses to swap the running binary unless the signature verifies against that key.

### Trust model (v1)

The private signing key lives as a password-protected GitHub Actions secret. This means:

- **TLS + GitHub authentication** protects release assets in transit.
- **The signing key** protects authenticity against transport or host compromise (a DNS hijack of `wardnet.network` alone cannot produce a valid update — the attacker also needs the signing key).
- **Compromise of the GitHub organisation** is effectively compromise of the signing key. We consider this an acceptable threat model for a single-maintainer project at v1, and we rely on GitHub's organisation-level protections (2FA, branch protection, required reviews) to mitigate it.

A future release will move signing to hardware-backed keys (YubiKey / air-gapped laptop) and document a key rotation plan.

### Verifying a release manually

```sh
# Fetch the three artefacts for your target, then:
minisign -Vm wardnetd-0.2.1-aarch64-unknown-linux-gnu.tar.gz \
  -p deploy/keys/wardnet-release.pub \
  -x wardnetd-0.2.1-aarch64-unknown-linux-gnu.tar.gz.minisig

# And cross-check the SHA-256:
sha256sum -c <(awk '{print $1"  wardnetd-0.2.1-aarch64-unknown-linux-gnu.tar.gz"}' \
  wardnetd-0.2.1-aarch64-unknown-linux-gnu.tar.gz.sha256)
```

Key generation, secret setup, and rotation procedures are documented in [`deploy/keys/README.md`](deploy/keys/README.md).
