import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import { WardnetApiError, type Device } from "@wardnet/js";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** Canonical display name for a device: explicit name, else hostname, else MAC. */
export function deviceDisplayName(
  device: Pick<Device, "name" | "hostname" | "mac">,
): string {
  return device.name ?? device.hostname ?? device.mac;
}

/** Strip a MAC to its 12 lowercase hex digits, or `null` if it isn't a
 *  full 6-octet address. Lets us compare MACs across formats
 *  (`AA:BB:..`, `aa-bb-..`, `aabb..`) without caring about case or
 *  separators. */
function normalizeMac(mac: string): string | null {
  const clean = mac.replace(/[^0-9a-f]/gi, "").toLowerCase();
  return clean.length === 12 ? clean : null;
}

/**
 * Hostname to auto-suggest for a reservation whose MAC matches a managed
 * device (issue #85). Returns the device's display name for the unique
 * match, or `undefined` when there is no usable suggestion:
 *
 * - the MAC isn't yet a complete address,
 * - zero devices match, or more than one does (ambiguous — shouldn't
 *   happen, but we don't guess),
 * - the only "name" we'd offer is the MAC itself (device has neither an
 *   explicit name nor a discovered hostname) — filling the hostname field
 *   with the MAC helps no one.
 */
export function suggestHostnameForMac(
  devices: Pick<Device, "name" | "hostname" | "mac">[],
  mac: string,
): string | undefined {
  const target = normalizeMac(mac);
  if (!target) return undefined;

  const matches = devices.filter((d) => normalizeMac(d.mac) === target);
  if (matches.length !== 1) return undefined;

  const name = deviceDisplayName(matches[0]);
  // deviceDisplayName falls back to the raw MAC; skip that case.
  return normalizeMac(name) === target ? undefined : name;
}

/** Format bytes into a human-readable string (e.g. "1.2 GB"). */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const value = bytes / Math.pow(1024, i);
  // eslint-disable-next-line security/detect-object-injection -- i is a numeric magnitude index into a local const unit array; no attacker-controlled key
  return `${value.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

/** Format a throughput value in megabits/s (e.g. "85.0 Mbps"). */
export function formatMbps(mbps: number): string {
  return `${mbps.toFixed(1)} Mbps`;
}

/** Format a latency/jitter value in milliseconds, rounded (e.g. "23 ms"). */
export function formatMs(ms: number): string {
  return `${Math.round(ms)} ms`;
}

/**
 * Percentage of the direct (WAN) line a tunnel retains — the speed-test
 * headline. `direct` of 0 yields 0 to avoid divide-by-zero.
 */
export function retentionPct(direct: number, tunnel: number): number {
  if (direct <= 0) return 0;
  return Math.round((tunnel / direct) * 100);
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
  if (!d) return "-";
  return `${d.getFullYear()}.${pad(d.getMonth() + 1)}.${pad(d.getDate())}`;
}

/** HH:MM:SS in 24-hour clock (local time). */
export function formatTime(
  value: Date | string | number | null | undefined,
): string {
  const d = parseDate(value);
  if (!d) return "-";
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** HH:MM in 24-hour clock (local time) — the compact form used in
 *  toolbars and chart axis ticks where seconds add noise. */
export function formatTimeShort(
  value: Date | string | number | null | undefined,
): string {
  const d = parseDate(value);
  if (!d) return "-";
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
  if (!d) return "-";
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
