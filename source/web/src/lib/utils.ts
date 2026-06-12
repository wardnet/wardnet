import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import { WardnetApiError } from "@wardnet/js";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** Format bytes into a human-readable string (e.g. "1.2 GB"). */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const value = bytes / Math.pow(1024, i);
  return `${value.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

/** Format seconds into a human-readable uptime string (e.g. "2d 5h 30m"). */
export function formatUptime(seconds: number): string {
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

/** Extract a user-friendly error message from an API error. */
export function apiErrorMessage(
  error: unknown,
  fallback = "Something went wrong",
): string {
  if (error instanceof WardnetApiError) {
    return error.body.detail ?? error.body.error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return fallback;
}

/** Extract the request ID from an API error (for log correlation). */
export function apiRequestId(error: unknown): string | undefined {
  if (error instanceof WardnetApiError) {
    return error.requestId;
  }
  return undefined;
}

/** Pad a number to two digits with a leading zero. */
function pad(n: number): string {
  return n.toString().padStart(2, "0");
}

/** Parse a value into a Date; returns `null` if invalid or pre-2000
 *  (which the backend may emit as a stand-in for "never"). */
function parseDate(
  value: Date | string | number | null | undefined,
): Date | null {
  if (value == null) return null;
  const d = value instanceof Date ? value : new Date(value);
  const ts = d.getTime();
  if (!Number.isFinite(ts) || ts < 946684800000) return null;
  return d;
}

/** YYYY.MM.DD (local time). Used everywhere a date is shown so it reads
 *  identically across locales — single source of truth for the date half of
 *  the app's date/time format (matches the CalVer dotted style). */
export function formatDate(
  value: Date | string | number | null | undefined,
): string {
  const d = parseDate(value);
  if (!d) return "—";
  return `${d.getFullYear()}.${pad(d.getMonth() + 1)}.${pad(d.getDate())}`;
}

/** HH:MM:SS in 24-hour clock (local time). */
export function formatTime(
  value: Date | string | number | null | undefined,
): string {
  const d = parseDate(value);
  if (!d) return "—";
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** HH:MM in 24-hour clock (local time) — the compact form used in
 *  toolbars and chart axis ticks where seconds add noise. */
export function formatTimeShort(
  value: Date | string | number | null | undefined,
): string {
  const d = parseDate(value);
  if (!d) return "—";
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** YYYY.MM.DD HH:MM:SS (local time, 24-hour). The default for any
 *  "show me when this happened" cell — keeps every row the same
 *  width and avoids the AM/PM that broke the UpdateCard layout on
 *  iPhone widths. */
export function formatDateTime(
  value: Date | string | number | null | undefined,
): string {
  const d = parseDate(value);
  if (!d) return "—";
  return `${formatDate(d)} ${formatTime(d)}`;
}

/** Format an ISO timestamp to a relative "time ago" string. */
export function timeAgo(iso: string): string {
  const ts = new Date(iso).getTime();
  // Treat epoch (or near-epoch — before 2000) as "never" rather than "55y ago".
  // WireGuard / the backend may report the zero timestamp for peers that have
  // not yet completed a handshake.
  if (!Number.isFinite(ts) || ts < 946684800000) return "never";
  const diff = (Date.now() - ts) / 1000;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}
