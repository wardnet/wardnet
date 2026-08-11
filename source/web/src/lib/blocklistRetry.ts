import type { Blocklist } from "@wardnet/js";

/** First backoff step, in minutes. Mirrors the daemon's cron runner. */
const BASE_MINUTES = 5;
/** Ceiling on the backoff, in minutes. */
const MAX_MINUTES = 6 * 60;
/** Doublings past which the cap has long since bitten. */
const MAX_STEPS = 16;

/**
 * When the daemon will next attempt a failing blocklist, or `null` when it is
 * not in backoff.
 *
 * Mirrors `retry_not_before` in the daemon's DNS-filter runner: the delay
 * doubles from five minutes up to a six-hour cap, measured from the last
 * failure rather than from now — so it survives a restart. Kept in step with
 * that function by construction; if the ladder changes there, change it here.
 */
export function blocklistRetryAt(blocklist: Blocklist): Date | null {
  if (blocklist.consecutive_failures === 0 || !blocklist.last_error_at) {
    return null;
  }
  const steps = Math.min(blocklist.consecutive_failures - 1, MAX_STEPS);
  const minutes = Math.min(BASE_MINUTES * 2 ** steps, MAX_MINUTES);
  const lastError = new Date(blocklist.last_error_at);
  if (Number.isNaN(lastError.getTime())) return null;
  return new Date(lastError.getTime() + minutes * 60_000);
}

/**
 * "failed 7 times, next retry in 6h" — the failure state of a blocklist in
 * one line, or `null` when it is healthy.
 */
export function blocklistFailureSummary(blocklist: Blocklist): string | null {
  const failures = blocklist.consecutive_failures;
  if (failures === 0) return null;

  const times = failures === 1 ? "once" : `${failures} times`;
  const retryAt = blocklistRetryAt(blocklist);
  if (!retryAt) return `failed ${times}`;

  const remainingMs = retryAt.getTime() - Date.now();
  if (remainingMs <= 0) return `failed ${times}, retrying shortly`;
  return `failed ${times}, next retry in ${formatDuration(remainingMs)}`;
}

/** Coarse "6h" / "12m" / "45s" rendering — one unit is enough here. */
function formatDuration(ms: number): string {
  const minutes = Math.round(ms / 60_000);
  if (minutes < 1) return `${Math.max(1, Math.round(ms / 1000))}s`;
  if (minutes < 60) return `${minutes}m`;
  return `${Math.round(minutes / 60)}h`;
}
