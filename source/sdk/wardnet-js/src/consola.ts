import { createConsola, type ConsolaInstance } from "consola";
import type { EmittedLevel, LogAdapter } from "./logger.js";

/**
 * `consola`-backed log adapter, exposed from the `@wardnet/js/consola`
 * subpath so importing the SDK's main entry never pulls `consola` in.
 * `consola` is declared as an optional peer dependency; only consumers that
 * enable this adapter need it installed.
 *
 * Wire it up once at app startup:
 *
 * ```ts
 * import { setAdapter } from "@wardnet/js";
 * import { createConsolaAdapter } from "@wardnet/js/consola";
 *
 * setAdapter(createConsolaAdapter());
 * ```
 *
 * The SDK owns level filtering, so the underlying instance runs at consola's
 * most verbose level and forwards every record it is handed. Pass a
 * pre-configured `instance` to share reporters/formatting with the host app.
 */
export function createConsolaAdapter(instance?: ConsolaInstance): LogAdapter {
  const root = instance ?? createConsola({ level: 5 });
  const scoped = new Map<string, ConsolaInstance>();

  const withTag = (tag: string): ConsolaInstance => {
    let s = scoped.get(tag);
    if (s === undefined) {
      s = root.withTag(tag);
      scoped.set(tag, s);
    }
    return s;
  };

  return {
    log(level: EmittedLevel, tag: string, args: unknown[]) {
      const write = withTag(tag)[level] as (...a: unknown[]) => void;
      write(...args);
    },
  };
}
