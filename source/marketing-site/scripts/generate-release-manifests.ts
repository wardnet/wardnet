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

// biome-ignore lint/correctness/noNodejsModules: build/codegen script, run by Node from the CLI
import { mkdir, writeFile } from "node:fs/promises";
// biome-ignore lint/correctness/noNodejsModules: build/codegen script, run by Node from the CLI
import { dirname, resolve } from "node:path";
// biome-ignore lint/correctness/noNodejsModules: build/codegen script, run by Node from the CLI
import { fileURLToPath } from "node:url";

import { Octokit } from "@octokit/rest";

import {
  classifyChannels,
  parseVersion,
  compareVersion,
  rcompareVersion,
  type ParsedVersion,
} from "./channels";
import { parseSha256Asset } from "./sha256";

const REPO_OWNER = "wardnet";
const REPO_NAME = "wardnet";

const ROOT = resolve(fileURLToPath(import.meta.url), "../..");
const PUBLIC_RELEASES = resolve(ROOT, "public/releases");
const PUBLIC_API_SPECS = resolve(ROOT, "public/api-specs");
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

/**
 * One row of `public/releases/openapi-versions.json` — the docs page
 * dropdown iterates over this list. Each row is a *distinct* OpenAPI
 * document by content hash, and carries the span of release versions
 * that shipped it so consumers can pick the right spec without
 * guessing which release changed the API.
 */
interface OpenapiVersion {
  /** SHA-256 of the openapi.json asset, hex lowercase. */
  sha256: string;
  /** Direct download URL of the openapi.json asset on GitHub. */
  openapi_url: string;
  /**
   * Site-relative path (no leading slash) to the spec after it has been
   * downloaded into `public/api-specs/` at build time. The docs page loads
   * this same-origin copy so Scalar never has to fetch GitHub cross-origin
   * (which would fail CORS) and the reference works on static Pages and in
   * local preview. Prefix with `import.meta.env.BASE_URL` when building the
   * URL.
   */
  spec_path: string;
  /** Every release version that served this exact spec, sorted ascending. */
  versions: string[];
  /** Convenience: `versions[0]`. */
  first_version: string;
  /** Convenience: `versions[versions.length - 1]`. */
  latest_version: string;
  /** True if any release in this group is a pre-release (beta channel). */
  includes_prerelease: boolean;
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
  // biome-ignore lint/correctness/noProcessGlobal: build/codegen script, run by Node from the CLI
  const octokit = new Octokit({ auth: process.env.GITHUB_TOKEN });
  // Paginate in case the release history grows beyond one page.
  const releases = await octokit.paginate(octokit.rest.repos.listReleases, {
    owner: REPO_OWNER,
    repo: REPO_NAME,
    per_page: 100,
  });
  return releases as GithubRelease[];
}

/** Build a per-release manifest. Returns null if the release has no tarball. */
function buildManifest(release: GithubRelease): Manifest | null {
  const parsed = parseVersion(release.tag_name);
  if (!parsed) {
    console.warn(`skipping release with unparseable tag: ${release.tag_name}`);
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
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- path is always resolve(PUBLIC_RELEASES, "stable.json"|"beta.json") — build-local constants
  await mkdir(dirname(path), { recursive: true });
  const body = manifest ?? emptyManifest();
  // eslint-disable-next-line security/detect-non-literal-fs-filename -- same build-local constant path as above; only the JSON body is remote, the filename is fixed
  await writeFile(path, `${JSON.stringify(body, null, 2)}\n`, "utf8");
  console.log(`wrote ${path}`);
}

/**
 * Build the list of distinct OpenAPI spec versions across all releases.
 *
 * For each release that shipped `openapi.json` + `openapi.json.sha256`,
 * read the sha256 file (small, one extra fetch per release) and group
 * by hash. The output is sorted newest-first so the docs dropdown puts
 * the current spec at the top.
 *
 * Releases without the openapi assets are skipped silently — releases
 * predating the OpenAPI publishing step will simply not appear.
 */
async function buildOpenapiVersions(releases: GithubRelease[]): Promise<OpenapiVersion[]> {
  // Walk releases newest-first so the first URL we record for a given
  // hash is the newest — stable enough for a human-friendly link.
  const ordered = releases
    .filter((r) => !r.draft)
    .map((r) => ({ release: r, version: parseVersion(r.tag_name) }))
    .filter(
      (entry): entry is { release: GithubRelease; version: ParsedVersion } =>
        entry.version !== null,
    );
  ordered.sort((a, b) => rcompareVersion(a.version, b.version));

  // Keep only entries that actually carry the openapi assets, then fan out
  // the sha256 fetches in parallel. One round-trip per release but bounded
  // by the browser/runtime's default concurrent-connection limit — the
  // previous `for await` serialisation made site builds linear in release
  // count for no good reason.
  const candidates = ordered
    .map(({ release, version }) => ({
      release,
      version,
      json: release.assets.find((a) => a.name === "openapi.json"),
      sha: release.assets.find((a) => a.name === "openapi.json.sha256"),
    }))
    .filter(
      (c): c is typeof c & { json: NonNullable<typeof c.json>; sha: NonNullable<typeof c.sha> } =>
        c.json !== undefined && c.sha !== undefined,
    );

  const settled = await Promise.allSettled(
    candidates.map(async ({ release, version, json, sha }) => {
      const resp = await fetch(sha.browser_download_url);
      if (!resp.ok) {
        throw new Error(`sha256 fetch failed (${resp.status})`);
      }
      const hash = parseSha256Asset(await resp.text(), release.tag_name);
      return { release, version, json, hash };
    }),
  );

  // Map: sha256 -> group accumulator. Iterated in release order so the
  // first `openapi_url` recorded per hash is the newest — preserves the
  // original "newest link for this spec" invariant.
  const groups = new Map<
    string,
    {
      sha256: string;
      openapi_url: string;
      versions: { version: string; parsed: ParsedVersion }[];
      includes_prerelease: boolean;
    }
  >();

  for (let i = 0; i < settled.length; i++) {
    // eslint-disable-next-line security/detect-object-injection -- numeric loop index into the local Promise.allSettled result array
    const outcome = settled[i]!;
    if (outcome.status === "rejected") {
      console.warn(
        // eslint-disable-next-line security/detect-object-injection -- numeric loop index into the local candidates array, aligned 1:1 with settled
        `openapi-versions: skipping ${candidates[i]!.release.tag_name}: ${
          (outcome.reason as Error).message
        }`,
      );
      continue;
    }
    const { release, version, json, hash } = outcome.value;

    const existing = groups.get(hash);
    if (existing) {
      existing.versions.push({ version: version.version, parsed: version });
      existing.includes_prerelease ||= release.prerelease || version.prerelease.length > 0;
    } else {
      groups.set(hash, {
        sha256: hash,
        openapi_url: json.browser_download_url,
        versions: [{ version: version.version, parsed: version }],
        includes_prerelease: release.prerelease || version.prerelease.length > 0,
      });
    }
  }

  // Sort each group's version list ascending for display, then sort the
  // groups themselves by their latest_version descending.
  const rows: OpenapiVersion[] = [];
  for (const g of groups.values()) {
    g.versions.sort((a, b) => compareVersion(a.parsed, b.parsed));
    const versions = g.versions.map((v) => v.version);
    rows.push({
      sha256: g.sha256,
      openapi_url: g.openapi_url,
      // Deterministic from the content hash; the file is written by
      // `writeOpenapiSpecs` below. Rows whose download fails are dropped
      // there so the manifest never points at a missing spec.
      spec_path: `api-specs/${g.sha256}.json`,
      versions,
      first_version: versions[0]!,
      latest_version: versions[versions.length - 1]!,
      includes_prerelease: g.includes_prerelease,
    });
  }
  // Group sort uses the latest_version string. Re-parse on the fly
  // since rcompareVersion takes ParsedVersion. Both sides are
  // guaranteed parseable (they came from `parseVersion` upstream), so
  // the non-null assertion is safe.
  rows.sort((a, b) =>
    rcompareVersion(parseVersion(a.latest_version)!, parseVersion(b.latest_version)!),
  );
  return rows;
}

/**
 * Download each distinct OpenAPI spec into `public/api-specs/<sha>.json` so
 * the docs page can load it same-origin (no CORS, works on static Pages).
 *
 * Returns the rows whose spec downloaded and parsed as JSON, preserving the
 * input order. A row whose download fails is dropped rather than kept with a
 * dangling `spec_path`, so the version picker never offers a broken option.
 * Like the rest of the generator, network failures degrade gracefully: on a
 * total outage every row is dropped and the docs page simply shows no
 * versions instead of failing the build.
 */
async function writeOpenapiSpecs(rows: OpenapiVersion[]): Promise<OpenapiVersion[]> {
  if (rows.length === 0) return [];
  await mkdir(PUBLIC_API_SPECS, { recursive: true });

  const settled = await Promise.all(
    rows.map(async (row) => {
      try {
        const resp = await fetch(row.openapi_url);
        if (!resp.ok) {
          throw new Error(`spec fetch failed (${resp.status})`);
        }
        const body = await resp.text();
        // Guard against writing a GitHub error/HTML page as if it were a
        // spec — parse before persisting.
        JSON.parse(body);
        // eslint-disable-next-line security/detect-non-literal-fs-filename -- filename is a SHA-256 validated by parseSha256Asset (64 hex chars), so it cannot traverse
        await writeFile(resolve(PUBLIC_API_SPECS, `${row.sha256}.json`), body, "utf8");
        return row;
      } catch (error) {
        console.warn(
          `openapi-spec: skipping ${row.latest_version} (${row.sha256.slice(0, 8)}): ${
            (error as Error).message
          }`,
        );
        return null;
      }
    }),
  );

  return settled.filter((row): row is OpenapiVersion => row !== null);
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

  const { stable, beta, edge } = classifyChannels(releases);

  const stableManifest = stable ? buildManifest(stable) : null;
  const betaManifest = beta ? buildManifest(beta) : null;
  const edgeManifest = edge ? buildManifest(edge) : null;

  await writeManifest(resolve(PUBLIC_RELEASES, "stable.json"), stableManifest);
  await writeManifest(resolve(PUBLIC_RELEASES, "beta.json"), betaManifest);
  await writeManifest(resolve(PUBLIC_RELEASES, "edge.json"), edgeManifest);

  const openapiVersions = await buildOpenapiVersions(releases);
  // Download each distinct spec same-origin; keep only the rows that landed
  // so the manifest and the on-disk specs stay in lockstep.
  const openapiWithSpecs = await writeOpenapiSpecs(openapiVersions);
  const openapiManifestPath = resolve(PUBLIC_RELEASES, "openapi-versions.json");
  await mkdir(dirname(openapiManifestPath), { recursive: true });
  await writeFile(openapiManifestPath, `${JSON.stringify(openapiWithSpecs, null, 2)}\n`, "utf8");
  console.log(`wrote ${openapiManifestPath} (${openapiWithSpecs.length} distinct spec versions)`);

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
