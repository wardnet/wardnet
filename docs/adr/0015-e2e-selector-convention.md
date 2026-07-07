# ADR: `data-testid`-primary selectors for the web-ui Playwright suite

**Status**: Accepted  
**Date**: 2026-06-27  
**Issue**: #617 (epic #614)

---

## Context

The web-ui Playwright suite (`source/end2end-tests/web-ui/`) covers three
frontend surfaces. The PW-0 scaffold and the A1 auth/setup specs adopted
**role/label-first selectors** (`getByRole`, `getByLabel`) — Playwright's
own recommended default — documented only in code comments
(`fixtures/ui.ts`, `tests/admin-site/setup.spec.ts`), never in an ADR.

Two forces made that default a liability for this codebase:

1. **A branding re-skin and copy churn are pending.** Role/label/text
   locators are coupled to visible copy and DOM/ARIA structure, so a
   re-skin or a wording change breaks specs that test unrelated
   behaviour — the locator changes when the *presentation* changes, not
   when the *contract* changes.
2. **The shell has many low-text, repeated controls** (stat tiles, nav
   items, icon-only triggers) where a role/label locator is either
   ambiguous or absent, pushing specs toward brittle text matching.

We needed a locator strategy stable under re-skin without losing the
accessibility/intent coverage that role/label assertions give.

---

## Decision

**`data-testid` is the primary locator across the whole suite**, and
specs **additionally assert** the human-facing label/role/text where it
is meaningful. This reverses the role/label-first approach.

- **Attribute**: `data-testid` (Playwright's zero-config `getByTestId()`
  default — no `testIdAttribute` override).
- **Naming**: flat, kebab-case, area-prefixed (`nav-devices`,
  `mobile-menu-trigger`, `stat-devices`, `page-title`, `login-username`,
  `notfound-page`). Per-surface project scoping removes the need for
  namespacing.
- **Placement**: testids are declared on app components and the shared
  `@wardnet/web` components (e.g. `LoginForm`), and forwarded through
  `@wardnet/ui` primitives via their existing `...props` spread
  (`Button`, `Input`, `StatTile` already forward). Generic primitives
  carry no consumer-specific test contracts.
- **Label assertion**: applied only to elements with a meaningful label —
  interactive controls and headings. Structural containers get a testid
  but no text assertion.
- **Scope**: testids are added as specs need them, not pre-seeded.

The full, operational version of these rules lives in the suite's
[`README.md`](../source/end2end-tests/web-ui/README.md) ("Selector
convention"); `.agents/testing.md` links to it.

---

## Consequences

- The A1 specs (`login.spec.ts`, `setup.spec.ts`) and the `ui.ts` login
  helper were retrofitted to the new convention in the #617 PR. The
  per-surface `smoke.spec.ts` files use only `#root` and were unaffected.
- App and shared components now carry `data-testid` attributes that ship
  to production. This is an accepted, negligible cost (a few bytes per
  attribute) for locator stability.
- Setup-wizard step transitions remain gated on `getByRole("heading", …)`
  — the heading is the meaningful label, so the role assertion both gates
  the step and satisfies the label-assertion rule; the interactive
  controls within each step are located by testid.
- Because the locator is decoupled from copy, a spec failing after a
  re-skin now signals a *real* contract change, not incidental wording
  drift — the property this decision was made to buy.
