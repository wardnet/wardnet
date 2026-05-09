/**
 * Wardnet Forge — Tailwind config
 * --------------------------------
 * The design system's source of truth is `styles.css` (CSS custom properties
 * under :root + [data-theme="dark"] + [data-density="compact"]).
 *
 * This config maps every token to a Tailwind utility so a Claude-Code-built
 * page can use either:
 *   - hand-written classes        (.card, .btn--primary, .pill--ok)
 *   - or Tailwind utilities       (bg-card, text-ink, rounded-lg, shadow-card)
 *
 * Both compile against the same CSS variables, so theme + density toggles
 * still flip everything in lockstep — Tailwind never bakes hex values.
 *
 * Usage:
 *   <html data-theme="light" data-density="comfortable">
 *   <link rel="stylesheet" href="styles.css">    // tokens + components
 *   <script src="https://cdn.tailwindcss.com"></script>   // dev
 *   // OR run a real build with this config
 *
 * Dark mode strategy is `data-theme` (selector), not class — matches our
 * existing token blocks without rewriting them.
 */

/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ['./**/*.{html,jsx,tsx,js,ts}'],
  darkMode: ['selector', '[data-theme="dark"]'],
  theme: {
    /* ── Colors ───────────────────────────────────────────────────────── */
    colors: {
      transparent: 'transparent',
      current: 'currentColor',
      white: '#ffffff',
      black: '#000000',

      // Surfaces
      bg:        'var(--bg)',
      elev:      'var(--bg-elev)',
      sunken:    'var(--bg-sunken)',
      card:      'var(--bg-card)',
      line:      'var(--line)',
      'line-strong': 'var(--line-strong)',

      // Ink (text)
      ink:    'var(--ink)',
      'ink-2': 'var(--ink-2)',
      'ink-3': 'var(--ink-3)',
      'ink-4': 'var(--ink-4)',

      // Sidebar (always navy regardless of theme)
      side: {
        DEFAULT: 'var(--side-bg)',
        line:    'var(--side-line)',
        ink:     'var(--side-ink)',
        'ink-2': 'var(--side-ink-2)',
        active:  'var(--side-active-bg)',
      },

      // Brand + status — every status hue gets DEFAULT + soft + soft-ink
      accent: {
        DEFAULT:    'var(--accent)',
        ink:        'var(--accent-ink)',
        soft:       'var(--accent-soft)',
        'soft-ink': 'var(--accent-soft-ink)',
      },
      warn: {
        DEFAULT:    'var(--warn)',
        soft:       'var(--warn-soft)',
        'soft-ink': 'var(--warn-soft-ink)',
      },
      danger: {
        DEFAULT:    'var(--danger)',
        soft:       'var(--danger-soft)',
        'soft-ink': 'var(--danger-soft-ink)',
      },
      info: {
        DEFAULT: 'var(--info)',
        soft:    'var(--info-soft)',
      },
    },

    /* ── Type ─────────────────────────────────────────────────────────── */
    fontFamily: {
      sans: 'var(--font-sans)',
      mono: 'var(--font-mono)',
    },

    /* Named scales — pull straight from §03 of the design system doc.
       Each entry is [size, { lineHeight, letterSpacing, fontWeight }]. */
    fontSize: {
      // Wardnet-named scales
      display:  ['48px', { lineHeight: '56px', letterSpacing: '-0.03em',  fontWeight: '600' }],
      h1:       ['28px', { lineHeight: '36px', letterSpacing: '-0.025em', fontWeight: '600' }],
      h2:       ['18px', { lineHeight: '24px', letterSpacing: '-0.015em', fontWeight: '600' }],
      stat:     ['32px', { lineHeight: '36px', letterSpacing: '-0.025em', fontWeight: '600' }],
      body:     ['14px', { lineHeight: '22px', letterSpacing: '-0.005em' }],
      label:    ['11px', { lineHeight: '14px', letterSpacing: '0.06em',  fontWeight: '500' }],
      'mono-body': ['12px', { lineHeight: '18px' }],
      'mono-log':  ['12px', { lineHeight: '16px' }],

      // Plain numeric scale, in case you need it
      xs: '11px', sm: '12px', base: '13px', md: '14px', lg: '16px', xl: '20px', '2xl': '24px', '3xl': '32px', '4xl': '48px',
    },

    /* ── Spacing & radius ─────────────────────────────────────────────── */
    spacing: {
      0:  '0',
      px: '1px',
      1:  '4px',   // xs
      2:  '8px',   // sm
      3:  '12px',
      4:  '16px',  // md
      5:  '20px',
      6:  '24px',  // lg
      8:  '32px',
      10: '40px',
      12: '48px',  // xl
      pad: 'var(--pad)',           // density unit (auto-shrinks under [data-density="compact"])
      row: 'var(--row-h)',         // list-row height (52px / 42px compact)
    },
    borderRadius: {
      none: '0',
      sm:   'var(--radius-sm)',    // 6
      DEFAULT: 'var(--radius)',    // 10
      md:   'var(--radius)',       // 10
      lg:   'var(--radius-lg)',    // 14
      xl:   'var(--radius-xl)',    // 20
      full: '9999px',
    },

    /* ── Elevation ────────────────────────────────────────────────────── */
    boxShadow: {
      none: 'none',
      card: 'var(--shadow-card)',
      pop:  'var(--shadow-pop)',
      // Focus ring — matches .field input:focus
      focus: '0 0 0 3px color-mix(in oklab, var(--accent) 25%, transparent)',
    },

    /* ── Borders ──────────────────────────────────────────────────────── */
    borderColor: ({ theme }) => ({
      ...theme('colors'),
      DEFAULT: 'var(--line)',
    }),

    /* ── Motion ───────────────────────────────────────────────────────── */
    transitionDuration: {
      snap:   '120ms',
      slide:  '200ms',
      stream: '600ms',
      DEFAULT: '120ms',
    },
    transitionTimingFunction: {
      DEFAULT: 'cubic-bezier(.2, .8, .2, 1)',
    },

    extend: {
      // Tabular figures for stat tiles
      fontVariantNumeric: { tabular: 'tabular-nums' },
    },
  },

  plugins: [
    /**
     * Component shortcuts — these are the patterns that show up in §05 and §09
     * of the design system doc. They're the same selectors as `styles.css` so
     * either approach renders identically. Use these when you want to write
     * <button class="btn btn--primary"> in markup or @apply them in your own
     * CSS.
     */
    function ({ addComponents }) {
      addComponents({
        // ── Card
        '.card-tw': {
          background: 'var(--bg-card)',
          border: '1px solid var(--line)',
          borderRadius: 'var(--radius-lg)',
          padding: 'var(--pad)',
          boxShadow: 'var(--shadow-card)',
        },

        // ── Buttons (Tailwind-flavored — class composition still works)
        '.btn-tw': {
          display: 'inline-flex', alignItems: 'center', gap: '6px',
          fontSize: '13px', fontWeight: '500',
          padding: '6px 12px',
          borderRadius: '8px',
          border: '1px solid var(--line)',
          background: 'var(--bg-elev)',
          color: 'var(--ink)',
          cursor: 'pointer',
          transition: 'background 120ms, border-color 120ms',
        },
        '.btn-primary-tw': {
          background: 'var(--accent)',
          color: 'var(--accent-ink)',
          borderColor: 'color-mix(in oklab, var(--accent) 70%, #000)',
          fontWeight: '600',
        },
        '.btn-danger-tw': {
          background: 'transparent',
          color: 'var(--danger)',
          borderColor: 'var(--line)',
        },

        // ── Pills
        '.pill-tw': {
          display: 'inline-flex', alignItems: 'center', gap: '6px',
          fontSize: '11.5px', fontWeight: '500',
          padding: '3px 8px', borderRadius: '9999px',
          background: 'var(--bg-sunken)', color: 'var(--ink-2)',
        },
        '.pill-ok-tw':   { background: 'var(--accent-soft)', color: 'var(--accent-soft-ink)' },
        '.pill-warn-tw': { background: 'var(--warn-soft)',   color: 'var(--warn-soft-ink)' },
        '.pill-down-tw': { background: 'var(--danger-soft)', color: 'var(--danger-soft-ink)' },

        // ── Field (label + description + control)
        '.field-label-tw': {
          fontSize: '13px', fontWeight: '500', color: 'var(--ink)',
        },
        '.field-desc-tw': {
          fontSize: '12px', color: 'var(--ink-3)',
          marginTop: '2px', marginBottom: '8px',
        },
        '.field-input-tw': {
          width: '100%',
          padding: '9px 12px',
          borderRadius: '8px',
          border: '1px solid var(--line)',
          background: 'var(--bg-elev)',
          color: 'var(--ink)',
          fontFamily: 'inherit', fontSize: '13px',
          outline: 'none',
        },
        '.field-input-tw:focus': {
          borderColor: 'var(--accent)',
          boxShadow: '0 0 0 3px color-mix(in oklab, var(--accent) 25%, transparent)',
        },

        // ── Read-mode label (uppercase, mono-tracked) — used for data points
        '.read-label-tw': {
          fontSize: '11px',
          fontWeight: '500',
          textTransform: 'uppercase',
          letterSpacing: '0.06em',
          color: 'var(--ink-3)',
        },

        // ── Tabs — underline (page nav)
        '.tabs-bar-tw': {
          display: 'flex', gap: '24px',
          borderBottom: '1px solid var(--line)',
        },
        '.tab-tw': {
          padding: '10px 0',
          fontSize: '13.5px', fontWeight: '500',
          color: 'var(--ink-3)',
          borderBottom: '2px solid transparent',
          marginBottom: '-1px',
          background: 'transparent', border: 0, cursor: 'pointer',
        },
        '.tab-active-tw': {
          color: 'var(--ink)',
          fontWeight: '600',
          borderBottomColor: 'var(--ink)',
        },
      });
    },
  ],
};
