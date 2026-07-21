import { describe, expect, it } from "vitest";

/**
 * Regression guard for the design-system typography convention
 * (`.agents/code-conventions.md`, "Typography"; ADR 0012). New markup must
 * express size and weight through the `<Text>` / `<Heading>` primitives'
 * `size` / `weight` props, not raw Tailwind `text-*` / `font-*` utilities —
 * the type scale is tokenised so a central change propagates everywhere.
 *
 * Explicitly still allowed, and therefore not matched here:
 *   - colour utilities (`text-ink-3`, `text-danger`, `text-accent`, …)
 *   - alignment utilities (`text-left`, `text-center`, …)
 *   - the font-family utility (`font-mono`)
 *
 * Exemptions:
 *   - shadcn primitives under `components/core/ui` are vendored and re-pulled
 *     via the CLI, so they are not our markup.
 *   - a genuinely unavoidable site (e.g. a native `<input>`'s `file:`
 *     pseudo-element, an off-scale hero numeral) can opt out with a
 *     `ds-typography-allow:` comment on, or just above, the offending line,
 *     stating why.
 */

const sources = import.meta.glob("../src/**/*.tsx", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

// Raw typography utilities that must not appear in markup. Anchored so
// colour/alignment tokens (`text-ink-3`, `text-left`) and the font-family
// utility (`font-mono`) never match. Kept as regex literals (not composed via
// `new RegExp`) so lint's `security/detect-non-literal-regexp` stays quiet.
const FORBIDDEN: readonly RegExp[] = [
  // Size utilities on the tokenised scale.
  /\btext-(?:xs|sm|base|lg|xl|[2-9]xl)\b/,
  // Arbitrary length sizes: `text-[13px]`, `text-[0.7rem]`.
  /text-\[[0-9.]+(?:px|rem|em)\]/,
  // Weight utilities.
  /\bfont-(?:thin|extralight|light|normal|medium|semibold|bold|extrabold|black)\b/,
];

const isForbidden = (line: string): boolean =>
  FORBIDDEN.some((pattern) => pattern.test(line));

const ALLOW_MARKER = "ds-typography-allow";

const isVendored = (path: string): boolean => path.includes("/core/ui/");

/**
 * A `ds-typography-allow: <reason>` comment opts a line out. It is honoured on
 * the offending line itself or among the attributes of the same JSX element —
 * i.e. anywhere between the class attribute and the element's opening tag.
 * The upward scan stops at that opening tag (`^\s*<Tag`), so a marker on a
 * neighbouring element can never leak in, and the reason comment can sit any
 * number of lines above the class without the match drifting out of a fixed
 * window.
 */
const isAllowed = (lines: string[], matchIndex: number): boolean => {
  // Walk upward from the offending line through its own element's attributes.
  const upward = lines.slice(0, matchIndex + 1).reverse();
  for (const line of upward) {
    if (line.includes(ALLOW_MARKER)) return true;
    // Opening tag of the element this attribute belongs to. `<Tag` mid-line
    // inside a comment doesn't start the line, so prose mentioning `<Text>`
    // won't trip this.
    if (/^\s*<[A-Za-z]/.test(line)) return false;
  }
  return false;
};

describe("admin-site typography conventions", () => {
  it("scans the whole source tree", () => {
    expect(Object.keys(sources).length).toBeGreaterThan(50);
  });

  it("routes size/weight through <Text>/<Heading>, not raw text-*/font-* utilities", () => {
    const violations: string[] = [];

    for (const [path, source] of Object.entries(sources)) {
      if (isVendored(path)) continue;

      const lines = source.split("\n");
      lines.forEach((line, index) => {
        if (!isForbidden(line)) return;
        if (isAllowed(lines, index)) return;

        const relative = path.replace(/^\.\.\//, "");
        violations.push(`${relative}:${index + 1}: ${line.trim()}`);
      });
    }

    expect(
      violations,
      `Raw typography utilities found (use <Text>/<Heading> size/weight props):\n${violations.join("\n")}`,
    ).toEqual([]);
  });
});
