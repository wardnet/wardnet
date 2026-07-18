import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Newest-spec-first, matching what the generator emits. Row 0 is a beta spec,
// row 1 a stable spec that shipped across two releases.
const ROWS = [
  {
    sha256: "aaaa1111",
    openapi_url: "https://example.com/a",
    spec_path: "api-specs/aaaa1111.json",
    versions: ["2026.06.00-beta.2"],
    first_version: "2026.06.00-beta.2",
    latest_version: "2026.06.00-beta.2",
    includes_prerelease: true,
  },
  {
    sha256: "bbbb2222",
    openapi_url: "https://example.com/b",
    spec_path: "api-specs/bbbb2222.json",
    versions: ["2026.05.00", "2026.05.03"],
    first_version: "2026.05.00",
    latest_version: "2026.05.03",
    includes_prerelease: false,
  },
];

function stubFetch(payload: unknown, ok = true) {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({
      ok,
      status: ok ? 200 : 500,
      json: () => Promise.resolve(payload),
    }),
  );
}

let createApiReference: ReturnType<typeof vi.fn>;
// Re-imported fresh each test (see beforeEach). ApiReference keeps a
// module-level `scalarPromise` singleton — correct in production (the page loads
// the viewer bundle once for its lifetime), but it would otherwise leak the
// script-load outcome from one test into the next. Resetting the module gives
// every test a clean singleton, so the two injection tests below are order- and
// state-independent.
let ApiReference: (typeof import("@/pages/ApiReference"))["ApiReference"];

beforeEach(async () => {
  vi.resetModules();
  ({ ApiReference } = await import("@/pages/ApiReference"));
  createApiReference = vi.fn();
  // The vi.fn() mock is callable but doesn't structurally match Scalar's
  // declared signature; cast so the stub satisfies the global type.
  window.Scalar = { createApiReference } as unknown as Window["Scalar"];
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  delete window.Scalar;
  // ensureScalar() appends its <script> straight to document.head, outside the
  // React tree, so RTL's auto-cleanup never removes it. Strip any leftover here
  // to keep the injection tests independent — otherwise a later test's
  // querySelector matches an earlier (already-settled) script instead of its
  // own freshly-injected one.
  document.querySelectorAll('script[src$="api-docs/scalar.js"]').forEach((el) => el.remove());
});

function renderPage() {
  render(
    <MemoryRouter>
      <ApiReference />
    </MemoryRouter>,
  );
}

/** URL Scalar was last asked to render. */
function lastRenderedUrl(): string | undefined {
  const call = createApiReference.mock.calls.at(-1);
  return call?.[1]?.url as string | undefined;
}

describe("ApiReference", () => {
  it("renders a daemon-version picker with one entry per published version", async () => {
    stubFetch(ROWS);
    renderPage();

    const select = await screen.findByRole("combobox", { name: /daemon version/i });
    const options = select.querySelectorAll("option");
    expect(options).toHaveLength(3);
    // Beta versions are tagged so operators don't pick one by accident.
    expect(screen.getByRole("option", { name: "2026.06.00-beta.2 (beta)" })).toBeInTheDocument();
  });

  it("defaults to the newest stable version and renders its spec", async () => {
    stubFetch(ROWS);
    renderPage();

    const select = await screen.findByRole("combobox", { name: /daemon version/i });
    expect((select as HTMLSelectElement).value).toBe("2026.05.03");
    await waitFor(() => expect(createApiReference).toHaveBeenCalled());
    expect(lastRenderedUrl()).toBe("/api-specs/bbbb2222.json");
  });

  it("renders the matching spec when a different version is picked", async () => {
    stubFetch(ROWS);
    renderPage();

    const select = await screen.findByRole("combobox", { name: /daemon version/i });
    await waitFor(() => expect(createApiReference).toHaveBeenCalled());

    await userEvent.selectOptions(select, "2026.06.00-beta.2");
    await waitFor(() => expect(lastRenderedUrl()).toBe("/api-specs/aaaa1111.json"));
  });

  it("shows an empty state when no specs are published", async () => {
    stubFetch([]);
    renderPage();

    expect(await screen.findByText(/aren't available here yet/i)).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: /daemon version/i })).not.toBeInTheDocument();
    expect(createApiReference).not.toHaveBeenCalled();
  });

  it("falls back to the empty state when the manifest fetch fails", async () => {
    stubFetch(null, false);
    renderPage();

    expect(await screen.findByText(/aren't available here yet/i)).toBeInTheDocument();
  });

  // The two tests below exercise ensureScalar()'s script-injection path, so
  // window.Scalar must be absent when the component's effect first runs (the
  // other tests pre-seed it in beforeEach to skip straight to "already loaded").
  // They no longer depend on run order — beforeEach resets the module, so each
  // starts with a fresh scalarPromise singleton.
  // jsdom never actually runs the injected <script>, so these two tests drive
  // its onload/onerror by hand. Everything that follows a dispatched event is a
  // synchronous chain of microtasks (promise settle → catch/then → setState →
  // re-render), so we flush it through act() rather than polling for the result
  // with a timeout — the state is committed by the time act() resolves, which
  // is both deterministic and fast (no CI-load-sensitive wait window).
  async function injectedScalarScript(): Promise<HTMLScriptElement> {
    // The <script> is appended by the viewer effect, which only runs after the
    // manifest fetch resolves and commits `ready` — a multi-turn chain, so we
    // retry the query with waitFor rather than a fixed act() drain (which can
    // land a turn short). This wait is for a cheap DOM presence check; the
    // load/error state transition it precedes is asserted deterministically via
    // act() below, with no polling window of its own.
    return waitFor(() => {
      const scripts = document.querySelectorAll<HTMLScriptElement>(
        'script[src$="api-docs/scalar.js"]',
      );
      const el = scripts[scripts.length - 1];
      if (!el) throw new Error("Scalar script was never injected");
      return el;
    });
  }

  it("shows an error state when the Scalar script fails to load", async () => {
    delete window.Scalar;
    stubFetch(ROWS);
    renderPage();

    const script = await injectedScalarScript();
    await act(async () => {
      script.onerror?.(new Event("error"));
    });

    expect(screen.getByText(/failed to load\. please refresh to try again/i)).toBeInTheDocument();
  });

  it("loads the Scalar script and renders once it loads", async () => {
    delete window.Scalar;
    stubFetch(ROWS);
    renderPage();

    const script = await injectedScalarScript();
    // The real bundle would set window.Scalar as a side effect of loading.
    window.Scalar = { createApiReference } as unknown as Window["Scalar"];
    await act(async () => {
      script.onload?.(new Event("load"));
    });

    expect(createApiReference).toHaveBeenCalled();
  });
});
