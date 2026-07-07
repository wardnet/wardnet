import { useEffect, useMemo, useRef, useState } from "react";
import { Navbar } from "@/components/layouts/Navbar";

const BASE = import.meta.env.BASE_URL;
const SCALAR_SRC = `${BASE}api-docs/scalar.js`;
const VERSIONS_URL = `${BASE}releases/openapi-versions.json`;

/** One row of `public/releases/openapi-versions.json` (see the generator). */
interface OpenapiVersion {
  sha256: string;
  openapi_url: string;
  /** Site-relative path (no leading slash) to the same-origin spec copy. */
  spec_path: string;
  versions: string[];
  first_version: string;
  latest_version: string;
  includes_prerelease: boolean;
}

/** A single selectable daemon version, resolved to its bundled spec. */
interface VersionChoice {
  version: string;
  sha256: string;
  specUrl: string;
  prerelease: boolean;
}

// The vendored Scalar standalone bundle (copied from the daemon at build time)
// exposes its factory as `window.Scalar.createApiReference`.
declare global {
  interface Window {
    Scalar?: {
      createApiReference: (target: Element | string, config: Record<string, unknown>) => unknown;
    };
  }
}

// Load the (large) Scalar bundle once, on demand — only visitors who open the
// API reference pay for it. Concurrent callers share the same promise.
let scalarPromise: Promise<void> | null = null;
function ensureScalar(): Promise<void> {
  if (typeof window !== "undefined" && window.Scalar) return Promise.resolve();
  if (scalarPromise) return scalarPromise;
  scalarPromise = new Promise<void>((resolve, reject) => {
    const script = document.createElement("script");
    script.src = SCALAR_SRC;
    script.async = true;
    script.onload = () => resolve();
    script.onerror = () => {
      // Reset so a later retry (e.g. navigating back) can try again.
      scalarPromise = null;
      reject(new Error("Failed to load the API reference viewer."));
    };
    document.head.appendChild(script);
  });
  return scalarPromise;
}

/** A version string is a pre-release when it carries a `-suffix` (e.g. `-beta.2`). */
function isPrerelease(version: string): boolean {
  return version.includes("-");
}

/**
 * Flatten the distinct-spec rows into one selectable entry per daemon
 * version, newest-first. Rows arrive sorted newest-spec-first with their
 * `versions` ascending, so reversing each row's versions and keeping row
 * order yields a stable newest-first list without a full version sort.
 */
function toChoices(rows: OpenapiVersion[]): VersionChoice[] {
  return rows.flatMap((row) =>
    [...row.versions].reverse().map((version) => ({
      version,
      sha256: row.sha256,
      specUrl: `${BASE}${row.spec_path}`,
      prerelease: isPrerelease(version),
    })),
  );
}

/**
 * Palette + fonts for the Scalar viewer, injected through its `customCss`
 * config so the overrides land inside Scalar's own scope — setting these from
 * a stylesheet only reached some of its surfaces (sidebar and some panels kept
 * Scalar's defaults). The values map onto the site's Forge tokens, which
 * resolve here because Scalar renders in the light DOM and custom properties
 * inherit from `:root`, so the docs share the article background, ink, accent
 * selection colour, and the Inter Tight / JetBrains Mono fonts. `.light-mode`
 * is what actually renders (the page pins light mode); `.dark-mode` mirrors it
 * defensively.
 */
const SCALAR_CUSTOM_CSS = `
.light-mode, .dark-mode {
  --scalar-font: var(--font-sans);
  --scalar-font-code: var(--font-mono);
  --scalar-background-1: var(--bg);
  --scalar-background-2: var(--bg-elev);
  --scalar-background-3: var(--bg-sunken);
  --scalar-border-color: var(--line);
  --scalar-color-1: var(--ink);
  --scalar-color-2: var(--ink-2);
  --scalar-color-3: var(--ink-3);
  --scalar-color-accent: var(--accent);
  --scalar-sidebar-background-1: var(--bg);
  --scalar-sidebar-border-color: var(--line);
  --scalar-sidebar-color-1: var(--ink);
  --scalar-sidebar-color-2: var(--ink-2);
  --scalar-sidebar-color-active: var(--accent);
  --scalar-sidebar-item-active-background: var(--accent-soft);
  --scalar-sidebar-item-hover-background: var(--bg-elev);
  --scalar-sidebar-item-hover-color: var(--ink);
  --scalar-sidebar-search-background: var(--bg-elev);
  --scalar-sidebar-search-border-color: var(--line);
  --scalar-sidebar-search-color: var(--ink);
}
`;

type LoadState = "loading" | "ready" | "empty" | "error";

/**
 * Interactive, multi-version API reference. Unlike the daemon — which serves
 * the single spec it's running — this page lets visitors pick any published
 * daemon version and renders that version's OpenAPI spec via the vendored
 * Scalar viewer. Specs are bundled same-origin at build time (see
 * `generate-release-manifests.ts`), so nothing is fetched cross-origin.
 *
 * Reached at `/docs/api-reference` (the docs catalogue "API reference" entry).
 */
export function ApiReference() {
  const [rows, setRows] = useState<OpenapiVersion[]>([]);
  const [state, setState] = useState<LoadState>("loading");
  const [selected, setSelected] = useState<string>("");
  const containerRef = useRef<HTMLDivElement>(null);

  const choices = useMemo(() => toChoices(rows), [rows]);
  const current = choices.find((c) => c.version === selected);
  const specUrl = current?.specUrl;

  // Fetch the manifest of published spec versions once on mount.
  useEffect(() => {
    let cancelled = false;
    fetch(VERSIONS_URL)
      .then((resp) => {
        if (!resp.ok) throw new Error(`manifest fetch failed (${resp.status})`);
        return resp.json() as Promise<OpenapiVersion[]>;
      })
      .then((data) => {
        if (cancelled) return;
        const list = Array.isArray(data) ? data : [];
        setRows(list);
        const all = toChoices(list);
        if (all.length === 0) {
          setState("empty");
          return;
        }
        // Default to the newest stable version; fall back to newest overall
        // (e.g. only pre-releases exist yet).
        const preferred = all.find((c) => !c.prerelease) ?? all[0]!;
        setSelected(preferred.version);
        setState("ready");
      })
      .catch(() => {
        if (!cancelled) setState("empty");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // (Re)render Scalar whenever the selected spec changes. Keyed on the spec
  // URL (not the choice object) so picking two versions that share one spec
  // doesn't needlessly re-mount the viewer.
  useEffect(() => {
    if (!specUrl || !containerRef.current) return;
    const host = containerRef.current;
    let cancelled = false;
    ensureScalar()
      .then(() => {
        if (cancelled || !window.Scalar) return;
        // Scalar mutates the target and doesn't expose a clean teardown, so
        // swap in a fresh child element on each render to avoid stacking.
        host.replaceChildren();
        const mount = document.createElement("div");
        host.appendChild(mount);
        window.Scalar.createApiReference(mount, {
          url: specUrl,
          // The site has no light/dark toggle, so pin the viewer to light and
          // hide its theme switch. Palette + fonts are matched to the site via
          // `customCss`; `withDefaultFonts: false` stops Scalar loading its own
          // typefaces so our Inter Tight / JetBrains Mono take over.
          forceDarkModeState: "light",
          hideDarkModeToggle: true,
          withDefaultFonts: false,
          customCss: SCALAR_CUSTOM_CSS,
          // Keep the viewer honest and lightweight for a public docs page.
          // There's no way to authenticate against a live daemon from here, so
          // hide the "Test Request" / API-client affordances — every call would
          // just 401. This stays a pure read-only reference.
          agent: { disabled: true },
          mcp: { disabled: true },
          showDeveloperTools: "never",
          hideClientButton: true,
          hideTestRequestButton: true,
        });
      })
      .catch(() => {
        if (!cancelled) setState("error");
      });
    return () => {
      cancelled = true;
    };
  }, [specUrl]);

  return (
    <div className="flex min-h-screen flex-col bg-bg">
      <Navbar showBack backTo="/docs" />

      <div className="border-b border-line bg-elev px-6 py-4">
        <div className="mx-auto flex max-w-[80rem] flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h1 className="t-size-xl t-weight-bold tracking-tight text-ink">API reference</h1>
            <p className="t-size-sm text-ink-3">
              Pick your daemon version to see the matching REST API.
            </p>
          </div>
          {state === "ready" && (
            <label className="flex items-center gap-2 t-size-sm t-weight-medium text-ink-2">
              Daemon version
              <select
                value={selected}
                onChange={(e) => setSelected(e.target.value)}
                className="rounded-md border border-line bg-card px-3 py-1.5 t-size-sm text-ink transition-colors hover:border-line-strong focus:border-accent focus:outline-none"
                aria-label="Daemon version"
              >
                {choices.map((choice) => (
                  <option key={choice.version} value={choice.version}>
                    {choice.version}
                    {choice.prerelease ? " (beta)" : ""}
                  </option>
                ))}
              </select>
            </label>
          )}
        </div>
      </div>

      <main className="flex-1">
        {state === "loading" && (
          <p className="px-6 py-16 text-center t-size-sm text-ink-3">Loading API reference…</p>
        )}
        {state === "empty" && (
          <div className="empty px-6 py-16">
            <h2 className="h-title">API reference</h2>
            <p className="h-sub mx-auto max-w-md">
              Published API specs aren&apos;t available here yet. Your gateway always serves its own
              interactive reference at <code className="kbd">/api/docs</code>.
            </p>
          </div>
        )}
        {state === "error" && (
          <p className="px-6 py-16 text-center t-size-sm text-ink-3">
            The API reference viewer failed to load. Please refresh to try again.
          </p>
        )}
        {/* Scalar mounts its full viewer here once a version is selected;
            palette + fonts are themed via the `customCss` config above. */}
        <div ref={containerRef} className={state === "ready" ? "block" : "hidden"} />
      </main>
    </div>
  );
}
