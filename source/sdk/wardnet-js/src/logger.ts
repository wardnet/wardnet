import { createConsola } from "consola";

export type LogLevel = "silent" | "error" | "warn" | "info" | "debug";

const NUMERIC: Record<LogLevel, number> = {
  silent: -1,
  error: 0,
  warn: 1,
  info: 3,
  debug: 5,
};

// consola instance at max level — we own all level filtering.
const _root = createConsola({ level: 5 });

const _overrides = new Map<string, LogLevel>();
const _resolved = new Map<string, LogLevel>();

function resolveLevel(tag: string): LogLevel {
  const cached = _resolved.get(tag);
  if (cached !== undefined) return cached;

  const parts = tag.split(".");
  let level: LogLevel = "info";
  for (let i = parts.length; i > 0; i--) {
    const prefix = parts.slice(0, i).join(".");
    const override = _overrides.get(prefix);
    if (override !== undefined) {
      level = override;
      break;
    }
  }

  _resolved.set(tag, level);
  return level;
}

/** Set the minimum log level for `tag` and all child loggers under it. */
export function setLevel(tag: string, level: LogLevel): void {
  _overrides.set(tag, level);
  // Invalidate cached resolutions for the tag and all its descendants.
  for (const t of _resolved.keys()) {
    if (t === tag || t.startsWith(`${tag}.`)) _resolved.delete(t);
  }
}

export interface Logger {
  readonly tag: string;
  error(...args: unknown[]): void;
  warn(...args: unknown[]): void;
  info(...args: unknown[]): void;
  debug(...args: unknown[]): void;
}

export function createLogger(tag: string): Logger {
  const scoped = _root.withTag(tag);
  // eslint-disable-next-line security/detect-object-injection -- keys are LogLevel union members indexing a Record<LogLevel, number>, never external input
  const enabled = (level: LogLevel) => NUMERIC[resolveLevel(tag)] >= NUMERIC[level];
  // Cast to satisfy TypeScript's strict spread-into-rest rules while keeping
  // our public interface typed as unknown[].
  type AnyArgs = [unknown, ...unknown[]];
  return {
    get tag() {
      return tag;
    },
    error: (...args) => {
      if (enabled("error")) scoped.error(...(args as AnyArgs));
    },
    warn: (...args) => {
      if (enabled("warn")) scoped.warn(...(args as AnyArgs));
    },
    info: (...args) => {
      if (enabled("info")) scoped.info(...(args as AnyArgs));
    },
    debug: (...args) => {
      if (enabled("debug")) scoped.debug(...(args as AnyArgs));
    },
  };
}

export const logger = createLogger("wardnet.sdk");
