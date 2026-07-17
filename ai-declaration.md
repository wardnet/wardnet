# AI declaration

Wardnet is developed with AI assistance. This file states where and how
much, so you can weigh it yourself rather than guess from the repo.

It follows the disclosure vocabulary used by
[c/selfhosted@lemmy.world](https://lemmy.world/post/49151085): each
category is declared at one of four levels — **Hint** (AI suggested,
human did the task), **Assisted** (AI did part, human did the bulk),
**Pair** (roughly 50/50), **Generated** (human prompted, AI generated).

Project tag: **[AIP]** — AI Project.

## Declaration

| Category | Level | What that means here |
| --- | --- | --- |
| **Design** | Assisted | Architecture, subsystem boundaries, and the [ADRs](docs/adr/) are human-driven. AI is used to pressure-test designs, surface alternatives, and draft sections — the decisions, and the trade-offs recorded in each ADR, are mine. |
| **Implementation** | Pair | Roughly an even split between hand-written and generated code across the Rust daemon, the web surfaces, and the SDK. Everything is human-reviewed before it lands. |
| **Testing** | Generated | Unit, service, and end-to-end suites are largely AI-generated from human-specified intent. Coverage gates and the cases that matter are human-chosen. |
| **Documentation** | Generated | The user docs, release notes, and the agent-facing conventions under [`.agents/`](.agents/) are largely AI-generated, then human-reviewed for accuracy. |
| **Review** | Assisted | AI reviews changes and surfaces findings before merge; the substantive review and every merge decision are mine. |
| **Deployment** | Generated | CI workflows, the release and signing pipeline, and the installer are largely AI-generated against human-specified requirements. |

## What this does not change

Every line that ships is reviewed by a human before merge, and the
project's correctness gates apply identically to generated and
hand-written code: `cargo clippy` at deny-warnings, the full test suite,
end-to-end tests against a real kernel in Docker, `cargo audit` on every
build, CodeQL, and OpenSSF Scorecard. AI assistance changed how the code
was written; it did not lower the bar for what gets in.

The honest caveat that matters more than any of the above: Wardnet is
**beta**, and it is daily-driven on exactly one Raspberry Pi — mine.
Treat it accordingly, whoever or whatever wrote it.

## Why this file exists

Because the alternative is you finding [`.agents/`](.agents/) and
[`AGENTS.md`](AGENTS.md) on your own and wondering what else wasn't
mentioned. If you think a declaration here is wrong, or you find
generated code that doesn't hold up, open an issue — that's a bug
report, and I want it.
