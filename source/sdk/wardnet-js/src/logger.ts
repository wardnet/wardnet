export type LogLevel = "silent" | "error" | "warn" | "info" | "debug";

/** The levels that actually reach an adapter (`silent` filters everything out). */
export type EmittedLevel = Exclude<LogLevel, "silent">;

const NUMERIC: Record<LogLevel, number> = {
  silent: -1,
  error: 0,
  warn: 1,
  info: 3,
  debug: 5,
};

/**
 * Output sink for log records. The SDK owns level filtering; an adapter only
 * has to write records it is handed. This is the seam that keeps the package
 * dependency-free by default: the built-in {@link consoleAdapter} needs
 * nothing, and richer sinks (e.g. the `consola` adapter under
 * `@wardnet/js/consola`) are opt-in via {@link setAdapter}.
 */
export interface LogAdapter {
  log(level: EmittedLevel, tag: string, args: unknown[]): void;
}

const CONSOLE_METHOD: Record<EmittedLevel, (...args: unknown[]) => void> = {
  error: (...args) => console.error(...args),
  warn: (...args) => console.warn(...args),
  info: (...args) => console.info(...args),
  debug: (...args) => console.debug(...args),
};

/**
 * Zero-dependency default adapter. Writes to the matching `console` method,
 * prefixing the record with its `[tag]` so scoped loggers stay
 * distinguishable in plain terminal or DevTools output.
 */
export const consoleAdapter: LogAdapter = {
  log(level, tag, args) {
    // eslint-disable-next-line security/detect-object-injection -- level is an EmittedLevel union member indexing a fixed Record, never external input
    CONSOLE_METHOD[level](`[${tag}]`, ...args);
  },
};

let _adapter: LogAdapter = consoleAdapter;

/**
 * Swap the output sink used by every logger, current and future. Apps that
 * bundle `consola` call this once at startup with the adapter from
 * `@wardnet/js/consola`; leaving it unset keeps the zero-dependency
 * {@link consoleAdapter}.
 */
export function setAdapter(adapter: LogAdapter): void {
  _adapter = adapter;
}

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
  const enabled = (level: EmittedLevel) => NUMERIC[resolveLevel(tag)] >= NUMERIC[level];
  const emit =
    (level: EmittedLevel) =>
    (...args: unknown[]) => {
      if (enabled(level)) _adapter.log(level, tag, args);
    };
  return {
    get tag() {
      return tag;
    },
    error: emit("error"),
    warn: emit("warn"),
    info: emit("info"),
    debug: emit("debug"),
  };
}

export const logger = createLogger("wardnet.sdk");
