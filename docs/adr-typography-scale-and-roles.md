# ADR: Typography scale and semantic text roles

**Status**: Accepted — implemented on branch `chore/storybook-web` (single PR)
**Date**: 2026-06-19
**Issue**: n/a — design-system hardening alongside the CSS-Modules migration

> **Revision (2026-06-19, mid-implementation).** A second challenge session
> widened the app-facing scope from "kill raw-px only; leave `text-*` to
> inherit" to **full component adoption** — app markup adopts the `<Text>` /
> `<Heading>` primitive and the size/weight `text-*`/`font-*` utilities are
> retired from markup (colour utilities are kept, per decision 4). This flips
> **decision 5**, refines **decision 2** (one `<Text>` primitive; role now
> supplies a *default* element), and adds the **primitive API** section below.
> The original narrow-scope wording is preserved struck-through inline so the
> change of mind is legible.

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

2. **Roles are a default bundle; delivered as CSS classes + one primitive.**
   Roles ship as `.t-*` CSS classes (the source of truth, usable from both
   markup and component CSS) **and** as a single `<Text role>` React primitive
   in `@wardnet/ui` (with a thin `<Heading level={n}>` alias that forwards to
   `role="h{n}"`). A role sets a **default for every typographic property** —
   size, weight, colour, **and** the rendered element — and each default is
   overridable per call via the `size` / `weight` / `color` / `as` props (see
   the *Primitive API* section). The element stays a separate decision: the
   role only supplies a *sensible default* `as`, always overridable
   (`<Text role="h2" as="div">`), which is what un-couples the old
   `CardTitle`-always-`<h3>` assumption.
   *(Originally: "Roles are element-agnostic … a role never dictates the HTML
   element", delivered as separate `<Text>` + `<Heading>` primitives. The
   refinement makes the element a role-supplied default rather than something
   the role must stay silent about, and collapses the two primitives into one
   since the original split existed only because roles carried no element.)*

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

5. **Scope: one pass, full component adoption.** Foundation (scale + roles +
   the `<Text>` / `<Heading>` primitive + Storybook) + migrate the
   design-system's own CSS off the literals + **convert app markup to the
   primitive** + a 4-app visual-QA pass — all in this single (mega-)PR. App
   markup adopts `<Text>` / `<Heading>`: the ~318 `text-*` **size** utilities
   and ~112 raw `text-[Npx]` literals are replaced either by a *role* (a named
   voice) or by the `size` / `weight` props (off-role one-offs), and the
   size/weight utilities are **retired from markup**. **Colour utilities are
   retained** as the recolour escape (decision 4) and mirrored by the `color`
   prop. Empirically the feared long tail is small: **zero** responsive size
   utilities exist across the apps, and nothing exceeds `4xl` (32px), so no
   `display` role and no responsive-size API are needed.
   *(Originally: "text-* utility usages are **not** rewritten; they inherit the
   new scale automatically" — the sweep touched only raw-px / inline sizes. The
   2026-06-19 revision widened this to full adoption; the override in decision 3
   still applies to any `text-*` that survives, e.g. inside `@wardnet/web`'s
   shipped CSS.)*

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
(not just the `font-mono` utility) for ergonomics. The `h1`/`h2`/`h3` "levels"
are roles too — `<Heading level={2}>` is sugar for `<Text role="h2">`.

### Primitive API

One primitive, `<Text>`, plus a thin `<Heading>` alias. `variant` is the only
required input for the common case; it bakes a default for **every** property,
and each is individually overridable for exceptions. The prop is named
**`variant`, not `role`** — `role` is reserved for the native ARIA attribute,
which passes straight through to the DOM (so `<Text variant="body" role="alert">`
is both a body voice *and* an ARIA alert):

```tsx
type Variant =                         // every named voice, headings included
  | "label" | "body" | "body-strong" | "caption" | "micro"
  | "metric" | "metric-unit" | "mono" | "h1" | "h2" | "h3";

interface TextProps {
  variant?: Variant;                   // default bundle: size + weight + colour + element
  size?: "2xs" | "xs" | "sm" | "base" | "lg" | "xl" | "2xl" | "3xl" | "4xl";
  weight?: "normal" | "medium" | "semibold" | "bold";
  color?: Ink | Status;               // ink | ink-2…ink-5 | accent | danger | warn | info | …
  as?: React.ElementType;             // override the variant's default element
  className?: string;                 // genuine one-offs (e.g. truncate); colour utilities still valid
  // …plus all native HTMLAttributes, incl. `role` (ARIA) and aria-* / data-*.
}

// <Heading level={n}> ≡ <Text variant={`h${n}`}>
```

Semantics:

- **`variant` sets defaults for `size`, `weight`, `color`, and `as`.** Omitting
  `variant` yields plain body-ish text driven entirely by the override props.
- **Override props win over the variant.** `<Text variant="body" weight="semibold">`
  is a 600-weight body line without inventing a `body-strong` call-site. (The
  `.t-*` variant classes carry zero specificity via `:where()`, so the
  `t-size-*`/`t-weight-*` helpers win structurally, not by source order.)
- **`as` is the element, decoupled from the voice.** The variant supplies a
  sensible default element (`h2`→`<h2>`, `body`/`caption`→`<p>`, inline voices
  like `label`/`micro`/`mono`/`metric-unit`→`<span>`); `as` overrides it.
- **`size` / `weight` replace the retired markup utilities.** They map to the
  scale / weight tokens (mechanism lives in `@wardnet/styles`, so `@wardnet/ui`
  still ships no CSS of its own). Off-variant one-offs use these instead of
  `text-sm` / `font-medium` in markup.
- **`color` mirrors the colour utilities** rather than replacing them: it emits
  the same `text-*` colour class the utility would, so the `@layer components`
  vs `utilities` cascade from decision 4 keeps working whether a caller reaches
  for the prop or a raw `className="text-danger"`.

This is the API the Stage-5 sweep migrates every app text element onto.

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
- **App markup becomes role-driven.** After the sweep, app `.tsx` expresses
  typography through `<Text>` / `<Heading>` rather than `text-*` / `font-*`
  size utilities, so the scale and the named voices are the only knobs a new
  screen reaches for. Cost: a large (~430-site), mostly-mechanical diff
  concentrated in admin-site, carried in this one PR by explicit choice (the
  alternative — staging the sweep behind the foundation — was considered and
  rejected in favour of landing the end state at once).

## Implementation checklist (status: complete)

1. [x] **Scale tokens** — `tokens.ts` (text scale + line-heights);
   `styles/styles.css` `:root` (`--text-*` + line-height vars, after
   `--font-mono`); `styles/theme.css` `@theme` (`--text-*` +
   `--text-*--line-height`, overriding Tailwind defaults).
2. [x] **Roles + primitive** — new `styles/typography.css` with `.t-*` /
   `.t-h*` role classes **and** `t-size-*`/`t-weight-*` helper classes in
   `@layer components`; a single `ui/src/primitives/text` `<Text>` (`role` +
   `size`/`weight`/`color`/`as` overrides) plus a thin `<Heading level>` alias;
   exported from `ui/src/index.ts`. `@wardnet/ui` emits no CSS — it references
   the `@wardnet/styles` classes by string.
   **Implementation note:** `typography.css` is imported from **`styles.css`**,
   not `theme.css`. The apps load `@wardnet/styles` (= `styles.css`) directly,
   so the roles must ride that single entry to reach app markup; `theme.css`
   and Storybook then pick them up transitively. (Importing only from
   `theme.css` would have left the four apps without the role classes.)
3. [x] **Storybook** — `Foundations/Typography` story (scale ramp + role
   specimens + `<Heading>` levels + primitive override-prop usage + recolour
   example). Roles load via the existing preview chain (`preview.css` →
   `theme.css` → `styles.css` → `typography.css`); no extra import needed.
4. [x] **Design-system CSS** — replaced every font-size literal in `styles.css`
   component blocks + the `*.module.css` files with `var(--text-*)`; deduped the
   `label` voice (card title, stat label, table head) onto a single shared rule
   mirroring `.t-label`; decoupled `CardTitle` from `<h3>` (now wears `t-label`
   + takes an `as` prop).
5. [x] **App sweep (full adoption)** — admin-site, admin-app, user-app adopt
   `<Text>` / `<Heading>` from `@wardnet/web`; size/weight utilities and raw
   `text-[Npx]` literals moved onto `size`/`weight` props, colour utilities
   kept. **marketing-site** has no `@wardnet/ui`/`@wardnet/web` dependency, so
   it adopts the same system via the `@wardnet/styles` helper classes it already
   loads (`text-sm` → `t-size-sm`, `font-medium` → `t-weight-medium`, …). Sites
   on interactive elements, custom components, and dynamic (clsx-conditional)
   classNames were intentionally left in place. **API note:** `<Text>`'s `role`
   prop shadows the ARIA `role` attribute; elements needing a native ARIA role
   (e.g. `role="alert"`) stay native, driven by `size`/`weight` utilities.
6. [x] **This ADR** — status flipped to Accepted.
7. [x] **Validate** — `type-check` + build green for each app + `@wardnet/ui`;
   Storybook build green; Playwright spot-checks / screenshots for the per-app
   visual-QA pass.

### Notes for the resuming session

- Source of truth for tokens is mirrored in three places that must stay in
  sync: `tokens.ts` (TS, also consumed by charts at runtime) → `styles.css`
  (CSS vars) → `theme.css` (Tailwind `@theme`).
- **Why the apps don't need per-`@theme` edits (verified empirically).** The
  apps `@import "@wardnet/styles"` (= `styles.css`) and each declares its own
  `@theme inline` block; only Storybook imports `theme.css`. Tailwind v4 emits
  `.text-sm { font-size: var(--text-sm); line-height: var(--tw-leading,
  var(--text-sm--line-height)); }` — the utilities **reference** the vars, and
  Tailwind's defaults live in `@layer theme`. `styles.css`'s `--text-*` vars
  sit in an **unlayered** `:root` imported after `tailwindcss`, so they beat
  the layered defaults and the override lands app-wide **through `styles.css`
  alone**. `theme.css`'s `@theme` copy is only what makes Storybook match. So
  Stage 1 needed no edits to the four app `index.css` `@theme` blocks.
- **`text-2xs` caveat.** `2xs` (11px) is not a Tailwind size, and the apps
  don't add it to `@theme`, so a `text-2xs` *utility* won't generate in apps.
  The `size="2xs"` prop / `micro` role therefore must drive font-size via a
  `@wardnet/styles` class (the `size`/`weight` helpers in `typography.css`),
  **not** by emitting a `text-2xs` Tailwind class. Same reasoning keeps
  `@wardnet/ui` CSS-free.
- The new API (one `<Text role>` + `<Heading level>` alias; `role` is a default
  bundle; `size`/`weight`/`color`/`as` override) and the full-adoption scope
  are recorded in the **Primitive API** section and the 2026-06-19 revision
  note — read those, not the original decisions 2/5 wording.
- `styles.css` component classes are **unlayered** (they win over utilities).
  Role classes must be **explicitly** wrapped in `@layer components` or the
  baked colour won't be overridable.
- `ui` is built as a library that externalises `@wardnet/styles`; the `Text` /
  `Heading` primitives reference role classes by **string** (`"t-label"`),
  emitting no CSS of their own — consistent with how `ui` consumes tokens.
- Build/validate `ui` with `NODE_OPTIONS=--preserve-symlinks yarn workspace
  @wardnet/ui build` (and `type-check`); Storybook static can be driven with
  Playwright via the latest chrome-headless-shell for interaction checks.
