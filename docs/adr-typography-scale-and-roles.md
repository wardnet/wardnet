# ADR: Typography scale and semantic text roles

**Status**: Accepted — implementation in progress (branch `chore/storybook-web`, single PR)
**Date**: 2026-06-19
**Issue**: n/a — design-system hardening alongside the CSS-Modules migration

---

## Context

The design system tokenises colour, radius, shadow, and motion, but **not
typography**. Only the font *families* are tokenised (`--font-sans` /
`--font-mono` in `@wardnet/styles`). Everything else — sizes, weights,
line-heights, letter-spacing — is a raw literal scattered across `styles.css`
and the component CSS modules.

Evidence of the resulting drift (grep over the current CSS):

- **16 distinct `font-size` literals**, including near-duplicates that betray
  copy-paste rather than intent: `12px`×23 next to `12.5px`×5, `13px`×23 next
  to `13.5px`×4, plus `11.5px`, `10px`, and a px/rem mix (`0.875rem`,
  `0.75rem`) — even a `font-size: 13px !important`.
- Weights cluster on `500`/`600`/`400` but with one-off `700`/`800` outliers.
- `letter-spacing: 0.06em`×8 is a single repeated "eyebrow/label" voice,
  duplicated rather than named; headings carry an ad-hoc scatter of negative
  trackings.
- **Zero `--text-*` / line-height tokens.**

There is a documented symptom in `card.module.css`: the card title "voice
matches the dashboard convention (`stat label`, `table head`)" — i.e. the same
`12px / 500 / uppercase / 0.06em / ink-3` role is reproduced in three places
with no shared definition.

A second pressure: the apps already lean on Tailwind's **default** text scale —
**318** uses of `text-sm`/`text-xs`/etc. across app `.tsx`. But Tailwind's
default `text-sm` is **14px** while the component CSS renders body at **13px**.
So app surfaces (14px) and components (13px) already disagree.

`tokens.ts` is the TS source of truth and its own header notes that anything
still living only in `styles.css` is "a known gap, not an intentional split" —
so a type scale has a designated home.

## Decision

Add typography to the design system as a **two-tier model** — a numeric base
scale plus named semantic roles — surfaced through CSS classes and thin React
primitives. This is the most widely adopted shape (Material 3, Apple HIG,
Polaris, Primer) adapted to the existing Tailwind v4 + token architecture.

The decisions, in order, were resolved through a challenge interview:

1. **Two tiers.** A base numeric scale (Tailwind's `text-*`) plus named
   semantic *roles* composed on top. Raw sizes stay available for one-offs;
   roles capture the recurring voices.

2. **Roles are element-agnostic; delivered as CSS classes + thin primitives.**
   Roles ship as `.t-*` CSS classes (the source of truth, usable from both
   markup and component CSS) **and** as `<Text as>` / `<Heading as>` React
   primitives in `@wardnet/ui` that apply those classes. **A role never
   dictates the HTML element** — the element is a separate accessibility /
   document-outline decision, chosen per use via `as`. (This explicitly
   un-couples the current `CardTitle`-always-renders-`<h3>` assumption.)

3. **Override Tailwind's `text-*` with the Forge scale, in `rem`.** Rather than
   add a parallel namespace (which would leave two competing scales and not fix
   the drift), Tailwind's scale *becomes* the Wardnet scale at the dense Forge
   values (`text-sm` = 13px). One scale everywhere; the app-wide size shift is
   accepted and gated by a visual-QA pass.

4. **Roles bake their full voice, including default colour.** A role sets size +
   line-height + weight + tracking + transform + its default colour (e.g.
   `label` → `ink-3`). To keep "recolour = add a utility" clean rather than a
   specificity fight, **role classes live in `@layer components`** so Tailwind
   utilities (`text-danger`, `text-ink-2`, …), which sit in the later
   `utilities` layer, reliably win the cascade. (This is the same source-order
   fragility that bit the combobox trigger; the layer placement structurally
   avoids it.)

5. **Scope: one pass.** Foundation (scale + roles + primitives + Storybook) +
   migrate the design-system's own CSS off the literals + sweep the apps'
   raw-px / inline sizes + a 4-app visual-QA pass — all in this single PR.
   `text-*` utility usages are **not** rewritten; they inherit the new scale
   automatically.

6. **DS extraction is a separate initiative.** A future "My Account" SPA in
   another repo will also consume the design system, and the DS will eventually
   move to its own repo (own challenge + ADR). Typography lands **only** in
   `@wardnet/styles` (scale + roles) and `@wardnet/ui` (the primitives) — the
   exact packages that extract wholesale — so none of this work is throwaway.

### The scale (rem @ 16px root; line-height paired via `--text-*--line-height`)

| token  | px | rem      | line-height | absorbs (old literals)            |
|--------|----|----------|-------------|-----------------------------------|
| `2xs`  | 11 | .6875    | 1.3         | 10, 11, 11.5                      |
| `xs`   | 12 | .75      | 1.35        | 12, 12.5  *(= Tailwind default)*  |
| `sm`   | 13 | .8125    | 1.5         | 13, 13.5  *(was 14)*              |
| `base` | 14 | .875     | 1.5         | 14        *(was 16)*              |
| `lg`   | 16 | 1        | 1.4         | 15, 17    *(was 18)*              |
| `xl`   | 18 | 1.125    | 1.3         | 18        *(was 20)*              |
| `2xl`  | 22 | 1.375    | 1.2         | 22        *(was 24)*              |
| `3xl`  | 26 | 1.625    | 1.15        | 26        *(was 30)*              |
| `4xl`  | 32 | 2        | 1.05        | 32        *(was 36)*              |

**App-shift surface** (consequence of the override): `text-sm` (166 uses)
14→13 is the dominant change; `text-xs` (107 uses) is **unchanged** (12px ==
Tailwind default); heading utilities `lg/xl/2xl/3xl/4xl` (~36 uses) each shrink
one notch. That set is the focus of the visual-QA pass.

### Roles

`<Text role>` / `.t-*` (element-agnostic; baked voice incl. default colour):

| role          | size       | weight | tracking / transform      | colour | replaces                          |
|---------------|------------|--------|---------------------------|--------|-----------------------------------|
| `label`       | xs (12)    | 500    | uppercase, 0.06em         | ink-3  | the title==stat-label==table-head dup |
| `body`        | sm (13)    | 400    | —                         | ink    | default UI / prose                |
| `body-strong` | sm (13)    | 600    | —                         | ink    | inline emphasis                   |
| `caption`     | xs (12)    | 400    | —                         | ink-3  | field help / secondary            |
| `micro`       | 2xs (11)   | 500    | —                         | ink-3  | tiny meta                         |
| `metric`      | 4xl (32)   | 600    | −0.03em, lh 1.05          | ink    | `stat__value`                     |
| `metric-unit` | xl (18)    | 500    | —                         | ink-3  | `stat__value .unit`               |
| `mono`        | sm (13)    | 400    | `font-mono`               | ink    | MAC / endpoint identifiers        |

`<Heading level>` / `.t-h*`:

| level | size     | weight | tracking | colour | maps to              |
|-------|----------|--------|----------|--------|----------------------|
| `h1`  | 2xl (22) | 600    | −0.02em  | ink    | topbar / page title  |
| `h2`  | xl (18)  | 600    | −0.01em  | ink    | section header       |
| `h3`  | lg (16)  | 600    | —        | ink    | modal title (15→16)  |

No `display` role — 26px folds into `h1`/`metric`. `mono` is kept as a role
(not just the `font-mono` utility) for ergonomics.

## Consequences

- **One scale.** App surfaces and components share a single source of truth;
  the 14-vs-13 split disappears. New screens reach for a role or a scale token,
  never a raw px.
- **An app-wide visual shift** ships with the override (mostly `text-sm`
  14→13). Mitigated by a per-app visual-QA pass in the same PR; the change is
  reversible only at the cost of re-touching every surface, hence this ADR.
- **`@layer components` placement** makes role colours overridable by utilities
  by construction — no per-call specificity hacks.
- **Extraction-ready**: everything lands in `@wardnet/styles` + `@wardnet/ui`.

## Implementation checklist (status: in progress)

1. [ ] **Scale tokens** — `tokens.ts` (text scale + line-heights);
   `styles/styles.css` `:root` (`--text-*` + line-height vars, after
   `--font-mono`); `styles/theme.css` `@theme` (`--text-*` +
   `--text-*--line-height`, overriding Tailwind defaults).
2. [ ] **Roles + primitives** — new `styles/typography.css` with `.t-*` /
   `.t-h*` in `@layer components`, imported from `theme.css` (add to package
   `files`); `ui/src/primitives` `Text` + `Heading` (role/level + `as`,
   colour-override-friendly); export from `ui/src/index.ts`.
3. [ ] **Storybook** — `Typography` story (scale ramp + role specimens +
   primitive usage + recolour example). Manager/preview already load
   `theme.css`, so roles render once `typography.css` is imported there.
4. [ ] **Design-system CSS** — replace literals in `styles.css` component
   blocks + the `*.module.css` files with `var(--text-*)` / roles; dedupe the
   `label` voice (card title, stat label, table head) onto the role.
5. [ ] **App sweep** — per app (admin-site, admin-app, user-app,
   marketing-site): kill raw-px / inline sizes, apply roles where they fit;
   leave `text-*` utilities to inherit the new scale.
6. [ ] **This ADR** — flip status to Accepted when the PR lands.
7. [ ] **Validate** — `type-check` + build each app + `@wardnet/ui`; Storybook
   build; Playwright spot-checks; screenshot key screens per app for visual
   review.

### Notes for the resuming session

- Source of truth for tokens is mirrored in three places that must stay in
  sync: `tokens.ts` (TS, also consumed by charts at runtime) → `styles.css`
  (CSS vars) → `theme.css` (Tailwind `@theme`).
- `styles.css` component classes are **unlayered** (they win over utilities).
  Role classes must be **explicitly** wrapped in `@layer components` or the
  baked colour won't be overridable.
- `ui` is built as a library that externalises `@wardnet/styles`; the `Text` /
  `Heading` primitives reference role classes by **string** (`"t-label"`),
  emitting no CSS of their own — consistent with how `ui` consumes tokens.
- Build/validate `ui` with `NODE_OPTIONS=--preserve-symlinks yarn workspace
  @wardnet/ui build` (and `type-check`); Storybook static can be driven with
  Playwright via the latest chrome-headless-shell for interaction checks.
