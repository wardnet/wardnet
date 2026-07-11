/**
 * Validation for the SHA-256 that names each published OpenAPI spec.
 *
 * Deliberately kept free of Node built-ins so it can be imported by both the
 * build script and the test suite (the app's tsconfig has no `node` types).
 */

/**
 * Read the SHA-256 out of a release's `openapi.json.sha256` asset.
 *
 * The returned value is used as a PATH COMPONENT twice over — as
 * `resolve(PUBLIC_API_SPECS, `${hash}.json`)` when the spec is written to disk,
 * and as `api-specs/${hash}.json` in the manifest the public docs page fetches.
 * The asset body is whatever the release publisher put in it, so without this
 * check a hash of `../../../foo` would escape `public/api-specs` (`resolve`
 * normalises `..`) and write attacker-supplied bytes anywhere the build user
 * can reach. Nothing downstream re-validates it, and the caller only logs on
 * failure, so such a write would be silent.
 *
 * Constraining it to exactly what a SHA-256 can be — 64 lowercase hex digits —
 * closes both sinks at the single point the value enters the program.
 *
 * @throws if the asset does not contain a well-formed SHA-256.
 */
export function parseSha256Asset(body: string, tag: string): string {
  // sha256sum output is "<hash>  <filename>"; take the hash field.
  const hash = body.trim().split(/\s+/)[0]?.toLowerCase() ?? "";
  if (!/^[0-9a-f]{64}$/.test(hash)) {
    // Truncate: the input is untrusted and can be arbitrarily long.
    throw new Error(`malformed sha256 asset for ${tag}: ${JSON.stringify(hash.slice(0, 32))}`);
  }
  return hash;
}
