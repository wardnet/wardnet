#!/usr/bin/env node
/**
 * Build-time release manifest generator.
 *
 * Fetches GitHub Releases from wardnet/wardnet and emits:
 *
 *   public/releases/stable.json      — served at https://wardnet.network/releases/stable.json
 *   public/releases/beta.json        — served at https://wardnet.network/releases/beta.json
 *   src/generated/release-info.ts    — typed module consumed by the homepage badge
 *
 * Channel rules (SemVer-driven):
 *   - stable  = highest release whose version has no pre-release suffix AND is not marked prerelease
 *   - beta    = highest release overall (draft releases are always skipped)
 *
 * The daemon's auto-update runner reads the JSON files. The homepage imports
 * the TS module and renders a "Latest release" badge.
 *
 * Behaviour:
 *   - Uses GITHUB_TOKEN from env for a higher rate limit (5000/hr vs 60/hr).
 *     Unauthenticated calls still work for small repos but may rate-limit
 *     when CI runs land close together.
 *   - If the API is unreachable (offline dev, CI transient failure), writes
 *     empty placeholder manifests and logs a warning — never fails the build.
 */

import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { Octokit } from "@octokit/rest";
import semver from "semver";

const REPO_OWNER = "wardnet";
const REPO_NAME = "wardnet";

const ROOT = resolve(fileURLToPath(import.meta.url), "../..");
const PUBLIC_RELEASES = resolve(ROOT, "public/releases");
const GENERATED_DIR = resolve(ROOT, "src/generated");

/** Shape emitted to `public/releases/<channel>.json`. Consumed by the daemon. */
interface Manifest {
  version: string;
  tag: string;
  prerelease: boolean;
  published_at: string | null;
  asset_base_url: string;
  binary: {
    name: string;
    size_bytes: number;
  } | null;
  notes_url: string;
}

/** Shape emitted to `src/generated/release-info.ts`. Consumed by the homepage. */
interface ReleaseInfo {
  stable: Manifest | null;
  beta: Manifest | null;
  generated_at: string;
}

interface GithubRelease {
  tag_name: string;
  name: string | null;
  prerelease: boolean;
  draft: boolean;
  html_url: string;
  published_at: string | null;
  assets: Array<{
    name: string;
    browser_download_url: string;
    size: number;
  }>;
}

async function fetchReleases(): Promise<GithubRelease[]> {
  const octokit = new Octokit({ auth: process.env.GITHUB_TOKEN });
  // Paginate in case the release history grows beyond one page.
  const releases = await octokit.paginate(octokit.rest.repos.listReleases, {
    owner: REPO_OWNER,
    repo: REPO_NAME,
    per_page: 100,
  });
  return releases as GithubRelease[];
}

/** Strip a leading `v` and validate as semver. Returns null if not valid. */
function parseVersion(tag: string): semver.SemVer | null {
  const stripped = tag.replace(/^v/, "");
  return semver.parse(stripped);
}

/** Build a per-release manifest. Returns null if the release has no tarball. */
function buildManifest(release: GithubRelease): Manifest | null {
  const parsed = parseVersion(release.tag_name);
  if (!parsed) {
    console.warn(`skipping release with non-semver tag: ${release.tag_name}`);
    return null;
  }

  // Find the .tar.gz asset. Naming convention is
  // wardnetd-<version>-<target>.tar.gz; pick the first one (v1 has just one
  // target; later versions with multiple targets will want a per-arch manifest
  // shape — a TODO worth noting but not solving yet).
  const tarball = release.assets.find((a) => a.name.endsWith(".tar.gz"));
  if (!tarball) {
    console.warn(`skipping release ${release.tag_name}: no .tar.gz asset`);
    return null;
  }

  // Derive the asset base URL from the tarball's download URL.
  const asset_base_url = tarball.browser_download_url.replace(`/${tarball.name}`, "");

  return {
    version: parsed.version,
    tag: release.tag_name,
    prerelease: release.prerelease,
    published_at: release.published_at,
    asset_base_url,
    binary: {
      name: tarball.name,
      size_bytes: tarball.size,
    },
    notes_url: release.html_url,
  };
}

/**
 * Classify releases by channel.
 *
 * - `stable`: latest release with a non-prerelease tag AND prerelease=false.
 * - `beta`:   latest release overall (prereleases and stable both considered).
 */
function classifyChannels(releases: GithubRelease[]): {
  stable: GithubRelease | null;
  beta: GithubRelease | null;
} {
  const nonDraft = releases.filter((r) => !r.draft);

  // Pre-compute parsed versions for sorting. Drop releases whose tag is not
  // valid semver.
  const withVersions = nonDraft
    .map((r) => ({ release: r, version: parseVersion(r.tag_name) }))
    .filter(
      (entry): entry is { release: GithubRelease; version: semver.SemVer } =>
        entry.version !== null,
    );

  // Descending by semver precedence (pre-release sorts before release of same base).
  withVersions.sort((a, b) => semver.rcompare(a.version, b.version));

  const stable =
    withVersions.find((entry) => !entry.release.prerelease && !entry.version.prerelease.length)
      ?.release ?? null;

  const beta = withVersions[0]?.release ?? null;

  return { stable, beta };
}

function emptyManifest(): Manifest {
  return {
    version: "",
    tag: "",
    prerelease: false,
    published_at: null,
    asset_base_url: "",
    binary: null,
    notes_url: "",
  };
}

async function writeManifest(path: string, manifest: Manifest | null): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  const body = manifest ?? emptyManifest();
  await writeFile(path, `${JSON.stringify(body, null, 2)}\n`, "utf8");
  console.log(`wrote ${path}`);
}

async function writeReleaseInfo(info: ReleaseInfo): Promise<void> {
  const path = resolve(GENERATED_DIR, "release-info.ts");
  await mkdir(GENERATED_DIR, { recursive: true });

  // Emit a typed module so consumers get autocomplete + fail-closed when the
  // schema changes. Keep the type alongside the data so it's self-contained.
  const body = `// AUTO-GENERATED by source/site/scripts/generate-manifests.ts — do not edit by hand.
// Regenerated on every \`yarn build\` (CI and local).

export interface ReleaseManifest {
  version: string;
  tag: string;
  prerelease: boolean;
  published_at: string | null;
  asset_base_url: string;
  binary: { name: string; size_bytes: number } | null;
  notes_url: string;
}

export interface ReleaseInfo {
  stable: ReleaseManifest | null;
  beta: ReleaseManifest | null;
  generated_at: string;
}

export const releaseInfo: ReleaseInfo = ${JSON.stringify(info, null, 2)};
`;

  await writeFile(path, body, "utf8");
  console.log(`wrote ${path}`);
}

async function main(): Promise<void> {
  let releases: GithubRelease[] = [];
  try {
    releases = await fetchReleases();
    console.log(`fetched ${releases.length} releases from ${REPO_OWNER}/${REPO_NAME}`);
  } catch (error) {
    // Don't fail the build on transient API issues. Emit empty manifests
    // and a warning — CI will succeed, the homepage just won't show a version.
    console.warn(
      `warning: failed to fetch releases from GitHub (${(error as Error).message}). emitting empty manifests.`,
    );
  }

  const { stable, beta } = classifyChannels(releases);

  const stableManifest = stable ? buildManifest(stable) : null;
  const betaManifest = beta ? buildManifest(beta) : null;

  await writeManifest(resolve(PUBLIC_RELEASES, "stable.json"), stableManifest);
  await writeManifest(resolve(PUBLIC_RELEASES, "beta.json"), betaManifest);

  await writeReleaseInfo({
    stable: stableManifest,
    beta: betaManifest,
    generated_at: new Date().toISOString(),
  });

  if (stableManifest) {
    console.log(`stable: ${stableManifest.version}`);
  } else {
    console.log("stable: (none)");
  }
  if (betaManifest) {
    console.log(`beta: ${betaManifest.version}`);
  } else {
    console.log("beta: (none)");
  }
}

await main();
