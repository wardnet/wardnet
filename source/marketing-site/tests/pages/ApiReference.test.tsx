import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiReference } from "@/pages/ApiReference";

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

beforeEach(() => {
  createApiReference = vi.fn();
  // The vi.fn() mock is callable but doesn't structurally match Scalar's
  // declared signature; cast so the stub satisfies the global type.
  window.Scalar = { createApiReference } as unknown as Window["Scalar"];
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  delete window.Scalar;
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
});
