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
// `components.schemas`. utoipa's `IntoParams` can emit a `$ref` for an enum
// used only as a path/query parameter without pulling the enum into the
// components section, and we can't paper over that by editing
// `docs/openapi.json` (that would fail the daemon's own drift gate). Each
// entry here is the definition the daemon omitted, matching its serde
// serialization exactly.
//
// `collectMissingRefs` below is what turns a recurrence into a loud error
// pointing back here — the failure is silent from the daemon's side.
//
// A shim applies only when the name is genuinely absent, so it retires itself
// if the daemon later starts registering the schema.
const MISSING_SCHEMA_SHIMS = {
  // `GET /api/anomalies` takes this as a query parameter and nothing else
  // references it, so utoipa emits the `$ref` without registering the enum.
  // Mirrors `AnomalyQueryStatus`'s `#[serde(rename_all = "snake_case")]`.
  AnomalyQueryStatus: {
    type: "string",
    enum: ["open", "resolved", "all"],
  },
};

const HTTP_METHODS = ["get", "put", "post", "delete", "options", "head", "patch", "trace"];

// Yield the `[method, operation]` pairs of one path item, skipping the
// non-operation keys OpenAPI allows there (`parameters`, `summary`, `$ref`, …).
// Iterating the item's own entries — rather than indexing it by method name —
// keeps the traversal free of computed member access.
function operationsOf(item) {
  return Object.entries(item ?? {}).filter(
    ([method, op]) => HTTP_METHODS.includes(method) && op !== null && typeof op === "object",
  );
}

// The daemon still reuses `operationId`s across handlers — #1047 deduped the
// worst of it, but `get_me`, `list_profiles`, `create_profile`, `get_profile`,
// `update_profile` and `delete_profile` each remain claimed by two endpoints.
// openapi-typescript keys its `operations` interface by `operationId`, so those
// collisions would collapse
// distinct endpoints onto one shared type — silently mistyping every request
// and response involved. Since we consume the `paths` map (not `operations`)
// and the ids are an internal implementation detail, rewrite each one to a
// deterministic `<method>_<path>` slug: unique by construction, stable across
// regenerations, and independent of the order the daemon emits handlers in.
function normalizeOperationIds(spec) {
  for (const [path, item] of Object.entries(spec.paths ?? {})) {
    for (const [method, op] of operationsOf(item)) {
      op.operationId = `${method}_${path}`.replace(/[^a-zA-Z0-9]+/g, "_").replace(/_+$/g, "");
    }
  }
}

// utoipa's `IntoParams` defaults a handler's params to `in: path` unless the
// handler annotates otherwise, so a handler that forgets the annotation ends up
// with query-string params marked `in: path` and no matching `{placeholder}` in
// the template — invalid OpenAPI, which openapi-typescript would then type as
// required path segments the client can never fill. The daemon reads those from
// the query string (via `serde_html_form`), so reclassify any such orphaned
// path param before codegen.
//
// No endpoint currently trips this — `/stats` and `/stats/top`, which did, were
// fixed upstream in #1047. It stays as a guard because the next handler to omit
// the annotation would otherwise generate an unusable client silently.
function fixOrphanPathParams(spec) {
  const fixed = [];
  for (const [path, item] of Object.entries(spec.paths ?? {})) {
    for (const [method, op] of operationsOf(item)) {
      for (const param of op.parameters ?? []) {
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
  // Spec-defined schemas come last, so a name the daemon does register always
  // wins over its shim — the shim applies only when genuinely absent.
  spec.components.schemas = { ...MISSING_SCHEMA_SHIMS, ...(spec.components.schemas ?? {}) };

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
