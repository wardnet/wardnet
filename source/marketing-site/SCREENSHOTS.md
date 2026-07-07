# Capturing docs screenshots

How the screenshots under `public/docs/<slug>/` are produced. Follow
this so every image is the same size, crops identically, and looks like
one coherent set.

## The one rule that matters: fixed Chrome window size

**Every screenshot must be taken with the Chrome window at the exact
same size.** The crop below is calibrated to one window size; change the
window and the crop no longer lines up. When driving with the
claude-in-chrome tools, set it explicitly before capturing:

```
resize_window → width: 1600, height: 1100
```

Do not resize, maximize, or full-screen the window between shots. Do not
toggle the bookmarks bar or otherwise change Chrome's toolbar height
mid-session; the crop offset assumes a constant browser chrome height.

## Environment

- Boot the local stack: `make -C <worktree-root> run-dev` (mock daemon +
  admin-site on `:7412/admin/`, seeded demo data). The mock never seeds
  an admin account, so complete the setup wizard once per launch, then
  the seeded devices / tunnels / DNS data is reachable.
- If a screen looks thin, enrich
  `source/daemon/crates/wardnetd-mock/src/seed.rs` before capturing.
- Dismiss transient banners (e.g. the "did not shut down cleanly"
  alert) before the shot.

## Capture

macOS window capture gives a clean retina (2x) PNG of just the Chrome
window:

- **⌘⇧4 → Space → ⌥-click the Chrome window.** Holding Option drops the
  window shadow. Saves to the Desktop as
  `Screenshot <date> at <time>.png`.

At a 1600x1100 window on a 2x display this yields a **3336x2336** PNG
that still includes the browser tabs and URL bar. Those (and any
personal tabs / extensions) must not ship, so crop next.

## Crop (removes browser chrome + window margins)

ImageMagick, offset crop. Calibrated for the **1600x1100** window at 2x:

```
magick <capture>.png -crop 3130x2012+100+238 +repage <slug>-<name>.png
```

- `+100+238` skips the left window margin and the tab + URL bar.
- `3130x2012` keeps the full app viewport (sidebar + content) and trims
  the right/bottom window margin.

Result is a ~3130x2012 content-only PNG. If Chrome's chrome height ever
changes, re-calibrate the offset once against a fresh capture and update
this file.

## Place and reference

- Save to `public/docs/<slug>/<name>.png`.
- Reference from `content/docs/<slug>.md` with
  `![alt](/docs/<slug>/<name>.png "wide")`. The `"wide"` title renders
  the image full-width in `DocsArticle.tsx`.

## Style notes

- Show the full app viewport (dark sidebar + content) for context.
- No browser chrome, no personal tabs, no OS cursor over a control.
- Keep filenames short and descriptive (`devices-list`, `routing-edit`,
  `query-log`).
