/**
 * Release-channel classification — the pure core of the manifest generator.
 *
 * Lives in its own module so it can be unit-tested without importing
 * `generate-release-manifests.ts`, whose top-level `await main()` would go to
 * the network on import.
 *
 * Channel rules:
 *   - stable = highest release with no pre-release suffix and prerelease=false
 *   - beta   = highest release overall, EXCLUDING edge builds
 *   - edge   = highest edge build (`-edge.<run>`), or none
 *
 * The edge exclusion is load-bearing, not defensive. `beta` is defined as the
 * highest release overall, and `-edge.N` sorts above `-beta.N` of the same
 * base — so without it the first edge build would quietly become the next
 * update for every box on beta. See docs/adr/0023-edge-release-channel.md.
 */

/** The subset of a GitHub release this module needs to classify it. */
export interface ChannelRelease {
  tag_name: string;
  prerelease: boolean;
  draft: boolean;
}

/**
 * Parsed release version, structured for component-wise comparison.
 *
 * The npm `semver` package rejects CalVer with leading zeros
 * (`2026.05.00`), the form Wardnet uses for release tags. Using a
 * dotted-numeric parser instead lets the manifest generator handle
 * both legacy SemVer (`0.1.0`) and CalVer (`2026.05.00`, `2026.5.10`,
 * etc.) uniformly. Pre-release / build suffixes are split off into
 * their own field so they can be classified separately.
 */
export interface ParsedVersion {
  /** Original version string, tag prefixes stripped. */
  version: string;
  /**
   * Numeric components (`2026.05.00` → `[2026, 5, 0]`). Leading
   * zeros are coerced; comparison is component-wise as `u64`.
   */
  parts: number[];
  /** `-beta.1`, `-edge.147`, etc. Empty array when there's no suffix. */
  prerelease: string[];
}

/**
 * Strip the tag prefix and split into base + pre-release components.
 *
 * Handles both release tags (`v2026.07.00`, `v2026.07.00-beta.5`) and edge
 * tags (`edge-v2026.07.00-edge.147`). Edge builds are deliberately tagged
 * `edge-v*` rather than `v*` so they cannot trigger `release.yml`.
 */
export function parseVersion(tag: string): ParsedVersion | null {
  const stripped = tag.replace(/^(?:edge-)?v?/, "");
  if (!stripped) return null;
  // `+build` metadata isn't carried into the manifest; drop it.
  const noBuild = stripped.split("+", 1)[0]!;
  const dashAt = noBuild.indexOf("-");
  const head = dashAt === -1 ? noBuild : noBuild.slice(0, dashAt);
  const tail = dashAt === -1 ? "" : noBuild.slice(dashAt + 1);

  const parts: number[] = [];
  for (const segment of head.split(".")) {
    if (!/^\d+$/.test(segment)) return null;
    parts.push(parseInt(segment, 10));
  }
  if (parts.length === 0) return null;

  return {
    version: noBuild,
    parts,
    prerelease: tail ? tail.split(".") : [],
  };
}

/**
 * Compare two parsed versions numerically. Mirrors the daemon's
 * `is_newer` comparator (wardnetd-services::update::service): split
 * on `.`, compare components as integers, then break ties on the
 * pre-release suffix (any pre-release sorts before the release of
 * the same base, lexicographic tiebreak otherwise).
 */
export function compareVersion(a: ParsedVersion, b: ParsedVersion): number {
  const len = Math.max(a.parts.length, b.parts.length);
  for (let i = 0; i < len; i++) {
    // eslint-disable-next-line security/detect-object-injection -- numeric loop index into a local number[] from parseVersion; not an attacker-chosen key
    const ai = a.parts[i] ?? 0;
    // eslint-disable-next-line security/detect-object-injection -- numeric loop index into a local number[] from parseVersion; not an attacker-chosen key
    const bi = b.parts[i] ?? 0;
    if (ai !== bi) return ai - bi;
  }
  if (a.prerelease.length === 0 && b.prerelease.length === 0) return 0;
  if (a.prerelease.length === 0) return 1;
  if (b.prerelease.length === 0) return -1;
  for (let i = 0; i < Math.max(a.prerelease.length, b.prerelease.length); i++) {
    // eslint-disable-next-line security/detect-object-injection -- numeric loop index into a local string[] of prerelease segments; array read
    const ai = a.prerelease[i];
    // eslint-disable-next-line security/detect-object-injection -- numeric loop index into a local string[] of prerelease segments; array read
    const bi = b.prerelease[i];
    if (ai === undefined) return -1;
    if (bi === undefined) return 1;
    const ar = /^\d+$/.test(ai);
    const br = /^\d+$/.test(bi);
    // Numeric identifiers always rank lower than alphanumeric — same
    // tiebreak rule as semver §11.4.3, kept so beta numbering still
    // sorts predictably.
    if (ar && !br) return -1;
    if (!ar && br) return 1;
    if (ar && br) {
      const diff = parseInt(ai, 10) - parseInt(bi, 10);
      if (diff !== 0) return diff;
    } else if (ai !== bi) {
      return ai < bi ? -1 : 1;
    }
  }
  return 0;
}

export function rcompareVersion(a: ParsedVersion, b: ParsedVersion): number {
  return -compareVersion(a, b);
}

/**
 * True if this version is an edge build (`<base>-edge.<run>`).
 *
 * Keyed off the parsed *version*, not the tag, so a hand-cut tag that omits
 * the `edge-` prefix still can't leak into stable or beta.
 */
export function isEdgeVersion(version: ParsedVersion): boolean {
  return version.prerelease[0] === "edge";
}

/**
 * Classify releases by channel.
 *
 * - `stable`: highest release with a non-prerelease tag AND prerelease=false.
 * - `beta`:   highest release overall, edge builds excluded.
 * - `edge`:   highest edge build, or null if none has been published.
 */
export function classifyChannels<T extends ChannelRelease>(
  releases: T[],
): { stable: T | null; beta: T | null; edge: T | null } {
  const nonDraft = releases.filter((r) => !r.draft);

  // Pre-compute parsed versions for sorting. Drop releases whose tag is
  // not parseable as dotted-numeric (legacy SemVer or CalVer).
  const withVersions = nonDraft
    .map((r) => ({ release: r, version: parseVersion(r.tag_name) }))
    .filter((entry): entry is { release: T; version: ParsedVersion } => entry.version !== null);

  // Descending by version precedence (pre-release sorts before release of same base).
  withVersions.sort((a, b) => rcompareVersion(a.version, b.version));

  const stable =
    withVersions.find(
      (entry) =>
        !isEdgeVersion(entry.version) &&
        !entry.release.prerelease &&
        entry.version.prerelease.length === 0,
    )?.release ?? null;

  const beta = withVersions.find((entry) => !isEdgeVersion(entry.version))?.release ?? null;

  const edge = withVersions.find((entry) => isEdgeVersion(entry.version))?.release ?? null;

  return { stable, beta, edge };
}
