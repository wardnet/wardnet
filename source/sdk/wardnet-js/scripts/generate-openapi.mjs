// Regenerates the internal OpenAPI type module (`src/internal/openapi-schema.ts`)
// from the daemon's canonical spec at `docs/openapi.json`.
//
// The output is an implementation detail of `@wardnet/js`: it is never
// exported from the package entrypoint. Its only job is to give the
// hand-authored services a typed picture of the wire contract, so that a
// change in the daemon's DTOs surfaces as a TypeScript error in the mapping
// layer instead of drifting silently.
//
// Run via `yarn generate`. CI re-runs it and fails on any diff (see the
// `check-sdk-openapi` target in the repo Makefile, and the `generate:check`
// package script), mirroring the daemon's own `check-openapi` gate.

import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import openapiTS, { astToString } from "openapi-typescript";

const here = dirname(fileURLToPath(import.meta.url));
const SPEC_PATH = resolve(here, "../../../../docs/openapi.json");
const OUT_PATH = resolve(here, "../src/internal/openapi-schema.ts");

// Schemas the daemon references from `$ref`s but does not register under
// `components.schemas`. This happens for enums that appear only as path/query
// parameters: utoipa's `IntoParams` emits the `$ref` without pulling the enum
// into the components section. Regenerating the spec from the daemon won't add
// them, so we can't fix it by editing `docs/openapi.json` (that would fail the
// daemon's own drift gate). Instead we splice in the definition the daemon
// omitted, matching its serde serialization exactly.
//
// Each entry is applied only if the name is genuinely absent, so the shim
// disappears automatically if the daemon ever starts registering the schema.
const MISSING_SCHEMA_SHIMS = {
  // `StatsBucket` — #[serde(rename_all = "snake_case")] over Minute/Hour/Day.
  StatsBucket: { type: "string", enum: ["minute", "hour", "day"] },
};

const HTTP_METHODS = ["get", "put", "post", "delete", "options", "head", "patch", "trace"];

// The daemon reuses `operationId`s across handlers (six endpoints answer to
// `status`, `get_config` appears three times, …). openapi-typescript keys its
// `operations` interface by `operationId`, so those collisions would collapse
// distinct endpoints onto one shared type — silently mistyping every request
// and response involved. Since we consume the `paths` map (not `operations`)
// and the ids are an internal implementation detail, rewrite each one to a
// deterministic `<method>_<path>` slug: unique by construction, stable across
// regenerations, and independent of the order the daemon emits handlers in.
function normalizeOperationIds(spec) {
  for (const [path, item] of Object.entries(spec.paths ?? {})) {
    for (const method of HTTP_METHODS) {
      const op = item[method];
      if (op && typeof op === "object") {
        const slug = `${method}_${path}`.replace(/[^a-zA-Z0-9]+/g, "_").replace(/_+$/g, "");
        op.operationId = slug;
      }
    }
  }
}

// utoipa's `IntoParams` defaults a handler's params to `in: path` unless the
// handler annotates otherwise. A couple of GET endpoints (notably `/stats`)
// carry query-string params that were left at that default, so the spec marks
// them `in: path` even though the path template has no matching `{placeholder}`
// — which is invalid OpenAPI, and openapi-typescript would then type them as
// required path segments the client can never fill. The daemon actually reads
// them from the query string (via `serde_html_form`), so reclassify any such
// orphaned path param as a query param before codegen.
function fixOrphanPathParams(spec) {
  const fixed = [];
  for (const [path, item] of Object.entries(spec.paths ?? {})) {
    for (const method of HTTP_METHODS) {
      const op = item[method];
      for (const param of op?.parameters ?? []) {
        if (param.in === "path" && !path.includes(`{${param.name}}`)) {
          param.in = "query";
          param.required = false;
          fixed.push(`${method.toUpperCase()} ${path} ?${param.name}`);
        }
      }
    }
  }
  return fixed;
}

function collectMissingRefs(spec) {
  const defined = new Set(Object.keys(spec.components?.schemas ?? {}));
  const missing = new Set();
  const re = /#\/components\/schemas\/([A-Za-z0-9_.]+)/g;
  let match;
  const text = JSON.stringify(spec);
  while ((match = re.exec(text)) !== null) {
    if (!defined.has(match[1])) missing.add(match[1]);
  }
  return [...missing];
}

async function main() {
  const spec = JSON.parse(await readFile(SPEC_PATH, "utf8"));

  spec.components ??= {};
  spec.components.schemas ??= {};
  for (const [name, schema] of Object.entries(MISSING_SCHEMA_SHIMS)) {
    if (!(name in spec.components.schemas)) {
      spec.components.schemas[name] = schema;
    }
  }

  normalizeOperationIds(spec);
  fixOrphanPathParams(spec);

  const stillMissing = collectMissingRefs(spec);
  if (stillMissing.length > 0) {
    throw new Error(
      `openapi.json references component schemas that are neither defined nor ` +
        `shimmed: ${stillMissing.join(", ")}. Add a shim in generate-openapi.mjs ` +
        `or fix the daemon's spec.`,
    );
  }

  const ast = await openapiTS(spec, { alphabetize: true });
  const body = astToString(ast);

  const header = [
    "// AUTO-GENERATED — DO NOT EDIT.",
    "//",
    "// Internal type picture of the daemon's REST contract, generated from",
    "// docs/openapi.json by `yarn generate` (scripts/generate-openapi.mjs).",
    "//",
    "// This module is private to @wardnet/js: nothing here is re-exported from",
    "// the package entrypoint. The hand-authored services import these types to",
    "// pin their request/response mapping to the wire, so a daemon-side field",
    "// change breaks the build here instead of drifting silently.",
    "",
    "",
  ].join("\n");

  await writeFile(OUT_PATH, header + body, "utf8");
  process.stdout.write(`Wrote ${OUT_PATH}\n`);
}

main().catch((err) => {
  process.stderr.write(`${err?.stack ?? err}\n`);
  process.exit(1);
});
