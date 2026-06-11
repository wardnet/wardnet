# Wardnet — Brand Assets

**Your network. Your rules.**

The full, visual brand guide lives in **`../guide.html`** (open in a browser; print to PDF via ⌘/Ctrl-P). This file is a quick reference for engineers dropping the brand into product, sites, and docs.

---

## Logo

One shield, three forms:

| Form         | Use                                                | Files                                           |
|--------------|----------------------------------------------------|-------------------------------------------------|
| **App icon** | Squircle tile — home screen, store, favicon ≥180px | `svg/dist/wardnet-icon.svg`                     |
| **Mark**     | Standalone shield, single color                    | `svg/dist/wardnet-mark-{emerald,ink,white}.svg` |
| **Lockup**   | Mark + WARDNET wordmark, ± tagline                 | `svg/dist/wardnet-logo-*.svg`                   |

**Rules:** never stretch, rotate, recolor off-palette, add effects, or re-typeset the wordmark. Keep clear space ≥ ½ the shield height on all sides. Below 32px, use the favicon, not the open mark.

**Background → mark color**

- Ink / dark → **emerald**
- Paper / white → **emerald** or **ink**
- Emerald → **ink**

---

## Color

```
Wardnet Emerald   #12B981   primary / signal — mark, key actions, "NET"
Emerald Deep      #0E9266   hover, pressed, accent text on light
Emerald Tint      #E7F8F0   highlight surfaces, chips

Ink               #11152B   primary dark surface, body text on light
Ink 2             #1B2140   elevated dark surface
Paper             #F4F3EE   default light background
White             #FFFFFF   cards, raised surfaces

Slate             #5B6178   secondary text
Mist              #8A90A6   tertiary / muted text, hairlines
Line              #E6E4DC   borders on light
Danger            #E5484D   error states only
```

Emerald is an accent, not a flood — roughly 1 part emerald to 10 parts ink/neutral.

---

## Typography

| Role               | Family             | Weights | Use                                      |
|--------------------|--------------------|---------|------------------------------------------|
| Display / wordmark | **Archivo**        | 700–900 | Headlines, WARDNET wordmark, big numbers |
| Text / UI          | **Hanken Grotesk** | 400–600 | Body, labels, descriptions               |
| Mono / technical   | **JetBrains Mono** | 400–500 | Code, CLI, version tags, eyebrows        |

All three are free & open-source (OFL) on Google Fonts:

```html
<link href="https://fonts.googleapis.com/css2?family=Archivo:wght@500;600;700;800;900&family=Hanken+Grotesk:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
```

The wordmark is **Archivo 800, all-caps, letter-spacing -0.03em**, with `WARD` in ink/white and `NET` in Wardnet Emerald.

---

## Voice & tone

Like a sharp engineer explaining something to a peer: plain, precise, quietly confident, and respectful of the fact that the user is in control.

1. **Plainspoken** — say what it does in words people use out loud.
2. **Confident, never hype** — state facts, skip superlatives.
3. **Hand over control** — their hardware, their data, their rules.
4. **Precise where it counts** — be exact and technical for setup, config, and CLI.

> ✓ "Block ads and trackers for every device on your network."
> ✗ "Leverage best-in-class DNS-layer threat mitigation."


**Note: The lockup SVGs in the source assets reference the Archivo web font via @import. Use the files from the dist folder instead, as they are self-contained, do not import external fonts, and are intended for production use.**


_Wardnet Brand System · v1.0 · June 2026_
