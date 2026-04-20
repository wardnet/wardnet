# Security Policy

## Supported Versions

Wardnet is in active development (Phase 1 MVP). Only the latest code on the `main` branch receives security fixes. There are no versioned releases yet.

## Reporting a Vulnerability

Please **do not** report security vulnerabilities through public GitHub issues, as this exposes the vulnerability to everyone before a fix is available.

Instead, use GitHub's private vulnerability reporting:

**[Report a vulnerability](https://github.com/pedromvgomes/wardnet/security/advisories/new)**

You can expect:
- Acknowledgment within **48 hours**
- A status update within **7 days**
- A fix or mitigation plan for critical issues within **14 days**

## Scope

This policy covers the `wardnetd` daemon, the `wctl` CLI, and the web UI served by the daemon. Third-party dependencies are tracked via Dependabot; if you find a vulnerability in a dependency that is not yet covered by an advisory, please report it upstream to the relevant project as well.
