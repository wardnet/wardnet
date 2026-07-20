# Contributing to Wardnet

Thanks for considering it. Wardnet is a network gateway people run in
front of their homes, so the bar for what lands is deliberately high —
but the process is short, and everything it gates on can be run locally
before you push.

This file is the process. The reference material lives elsewhere and is
linked at each step, rather than duplicated here.

## Ways to contribute

**Report a bug.** [Open an issue](https://github.com/wardnet/wardnet/issues/new).
Include your install method (container / bare-metal), the daemon version
(`wctl status`, or the footer of the admin UI), what you expected, and
what happened. Relevant `journalctl -u wardnet` lines help a lot.

**Report a security vulnerability.** Do *not* open a public issue. Use
[GitHub's private vulnerability reporting](https://github.com/wardnet/wardnet/security/advisories/new).
See [SECURITY.md](SECURITY.md) for scope, response times, and release
signature verification.

**Suggest a feature.** Open an issue describing the problem you hit
before the solution you have in mind. Check the
[milestones](https://github.com/wardnet/wardnet/milestones) first — the
roadmap is public and the thing may already be scheduled.

**Improve documentation.** Docs changes are welcome and follow the same
PR flow as code, minus the test requirements.

**Send code.** Read on.

## Before you write code

For anything beyond a small fix, **open an issue first** and get
agreement on the approach. This project has opinionated architectural
boundaries (see [Architecture](.agents/architecture.md)), and it is
genuinely unpleasant to have a finished PR turned away because it put
business logic in a handler. A short design conversation up front avoids
that entirely.

Some changes need explicit sign-off before you invest in them:

- New dependencies in `Cargo.toml` or `package.json`
- Database migrations
- Changes to public API contracts or response shapes
- Removing files or functionality
- CI pipeline changes

## Setting up

Full prerequisites, build targets, and how to run the stack locally are
in the [development guide](docs/DEVELOPMENT.md#getting-started). The
short version:

```sh
make init      # install SDK / web UI / site dependencies
make run-dev   # mock daemon + web UI dev server on localhost:7412
```

`make run-dev` runs the daemon with no-op network backends and a seeded
in-memory database, so you can work on the UI without touching real
network infrastructure.

One macOS caveat: the daemon uses Linux-only kernel interfaces (netlink,
rtnetlink) and will not compile natively on macOS. `make check-daemon`
detects this and runs inside a Linux container — you need Podman or
Docker installed.

## Making the change

Conventions are documented per area; read the one that matches what
you're touching rather than all of them:

| Area | Read |
| --- | --- |
| Anything | [Architecture](.agents/architecture.md), [Code conventions](.agents/code-conventions.md) |
| Rust daemon | [Testing](.agents/testing.md), [Logging](.agents/logging.md), [Observability](.agents/observability.md) |
| Any service method | [Auth model](.agents/auth.md) — every method opens with an auth guard, no exceptions |
| HTTP handlers / DTOs | Run `make openapi` and commit `docs/openapi.json`; CI gates on it |
| Build commands | [Commands](.agents/commands.md) |

Domain terms have canonical meanings in [CONTEXT.md](CONTEXT.md) — worth
a skim so your naming matches the rest of the codebase.

A few rules that are non-negotiable, because they're the ones with teeth:

- **Parameterised SQL only** (`.bind()`). Never string-interpolate user
  input into a query.
- **SQL stays in the repository layer.** Handlers → services →
  repositories; handlers stay thin.
- **New functionality ships with tests.** Rust coverage must not
  decrease — check with `make coverage-daemon` before and after.
- **No `unsafe` Rust** without prior discussion.
- **Never skip or delete a failing test** to get green.
- Secrets, API keys, database files, and `.env` never get committed.

## Before you push

**Run the checks locally and fix everything they find.** CI runs these
exact targets, so a local failure is a guaranteed CI failure:

```sh
make check          # everything: SDK + web + site + daemon
```

If you only touched one area, the narrow targets are much faster:

```sh
make check-daemon   # fmt + clippy -D warnings + cargo test --workspace
make check-web      # typecheck + eslint + prettier
make check-site
make check-sdk
make check-version  # required if you touched VERSION or any package.json
```

Two common ways to get a false green: running `cargo build` without
`cargo test` (the test target has its own stubs that drift from service
signatures), and running scoped `cargo clippy -p <crate>`, which skips
`--all-targets`. Use the `make` targets.

Treat a rebase as a fresh change and re-run checks — dependency bumps
pulled from `main` can change lint and type rules under you.

## Opening the pull request

- Branch from `main`, named `feature/<description>`, `fix/<description>`,
  or `chore/<description>`.
- [Conventional commit](https://www.conventionalcommits.org/) subjects:
  `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`.
- Describe what changed and why, and link the issue it closes.
- Keep the PR focused. Unrelated cleanups belong in their own PR.
- If behaviour visible to users changed, say so explicitly — it needs to
  reach the release notes.

Review is by the maintainer ([CODEOWNERS](.github/CODEOWNERS)). Expect
questions about architectural fit and test coverage; they're not
gatekeeping, they're the same questions asked of every change including
the maintainer's own.

## Using AI coding agents

You're welcome to. This project is itself developed with AI assistance
and says so in detail in the [AI declaration](ai-declaration.md).

If you do, point your agent at [AGENTS.md](AGENTS.md) — the canonical
machine-readable conventions, which Claude Code picks up via
[CLAUDE.md](CLAUDE.md). Two conditions apply:

- **You are the author.** Review every line before you submit it. "The
  agent wrote it" is not a defence for a PR you can't explain.
- **No agent attribution trailers.** Commits carry the human author
  only — no `Co-Authored-By: Claude`, no equivalent. GitHub parses those
  trailers and inflates the contributor graph with bot accounts.

## Licensing your contribution

Wardnet is split across two licenses by path, and your contribution is
licensed under whichever applies where it lands:

| Path | License |
| --- | --- |
| `source/daemon/**` | GPL-3.0-or-later |
| everything else | MIT |

The daemon is GPL because it statically links
[`rustables`](https://crates.io/crates/rustables); this is an obligation,
not a preference. [LICENSING.md](LICENSING.md) has the full reasoning.

By submitting a pull request you agree to license your contribution
under the license covering the files you changed. There is no CLA.
