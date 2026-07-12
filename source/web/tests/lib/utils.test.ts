import { describe, expect, it } from "vitest";
import { WardnetApiError } from "@wardnet/js";
import {
  cn,
  deviceDisplayName,
  formatBytes,
  formatMbps,
  formatMs,
  retentionPct,
  formatUptime,
  apiErrorMessage,
  apiRequestId,
  formatDate,
  formatTime,
  formatTimeShort,
  formatDateTime,
  timeAgo,
} from "../../src/lib/utils";

describe("cn", () => {
  it("merges class names and dedupes tailwind conflicts", () => {
    expect(cn("px-2", "px-4")).toBe("px-4");
    const skip: string | false = false;
    expect(cn("a", skip && "b", "c")).toBe("a c");
  });
});

describe("deviceDisplayName", () => {
  it("prefers name, then hostname, then mac", () => {
    expect(deviceDisplayName({ name: "TV", hostname: "h", mac: "m" })).toBe(
      "TV",
    );
    expect(deviceDisplayName({ name: null, hostname: "h", mac: "m" })).toBe(
      "h",
    );
    expect(deviceDisplayName({ name: null, hostname: null, mac: "m" })).toBe(
      "m",
    );
  });
});

describe("formatBytes", () => {
  it("formats zero", () => {
    expect(formatBytes(0)).toBe("0 B");
  });
  it("formats bytes without decimals", () => {
    expect(formatBytes(512)).toBe("512 B");
  });
  it("scales to KB/MB/GB with one decimal", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1024 * 1024)).toBe("1.0 MB");
    expect(formatBytes(1.5 * 1024 * 1024 * 1024)).toBe("1.5 GB");
  });
});

describe("formatMbps / formatMs", () => {
  it("formats mbps with one decimal", () => {
    expect(formatMbps(85)).toBe("85.0 Mbps");
  });
  it("rounds milliseconds", () => {
    expect(formatMs(23.4)).toBe("23 ms");
    expect(formatMs(23.6)).toBe("24 ms");
  });
});

describe("retentionPct", () => {
  it("returns 0 when direct is non-positive", () => {
    expect(retentionPct(0, 50)).toBe(0);
    expect(retentionPct(-1, 50)).toBe(0);
  });
  it("computes rounded percentage", () => {
    expect(retentionPct(100, 85)).toBe(85);
    expect(retentionPct(3, 1)).toBe(33);
  });
});

describe("formatUptime", () => {
  it("formats days/hours/minutes", () => {
    expect(formatUptime(2 * 86400 + 5 * 3600 + 30 * 60)).toBe("2d 5h 30m");
  });
  it("drops days when zero", () => {
    expect(formatUptime(5 * 3600 + 30 * 60)).toBe("5h 30m");
  });
  it("shows only minutes under an hour", () => {
    expect(formatUptime(45 * 60)).toBe("45m");
  });
});

describe("apiErrorMessage", () => {
  it("uses detail from a WardnetApiError", () => {
    const err = new WardnetApiError(400, "Bad Request", {
      error: "bad_request",
      detail: "Specific detail",
    });
    expect(apiErrorMessage(err)).toBe("Specific detail");
  });
  it("falls back to error field when no detail", () => {
    const err = new WardnetApiError(400, "Bad Request", {
      error: "bad_request",
    });
    expect(apiErrorMessage(err)).toBe("bad_request");
  });
  it("uses Error.message for generic errors", () => {
    expect(apiErrorMessage(new Error("boom"))).toBe("boom");
  });
  it("uses the fallback for unknown values", () => {
    expect(apiErrorMessage("nope", "fallback")).toBe("fallback");
    expect(apiErrorMessage(null)).toBe("Something went wrong");
  });
});

describe("apiRequestId", () => {
  it("returns the request id from a WardnetApiError", () => {
    const err = new WardnetApiError(
      500,
      "Server Error",
      { error: "err" },
      "req-123",
    );
    expect(apiRequestId(err)).toBe("req-123");
  });
  it("returns undefined for non-API errors", () => {
    expect(apiRequestId(new Error("x"))).toBeUndefined();
  });
});

describe("date/time formatters", () => {
  const d = new Date(2026, 6, 2, 9, 5, 7); // local time

  it("formats date as YYYY.MM.DD", () => {
    expect(formatDate(d)).toBe("2026.07.02");
  });
  it("formats time as HH:MM:SS", () => {
    expect(formatTime(d)).toBe("09:05:07");
  });
  it("formats short time as HH:MM", () => {
    expect(formatTimeShort(d)).toBe("09:05");
  });
  it("formats datetime", () => {
    expect(formatDateTime(d)).toBe("2026.07.02 09:05:07");
  });
  it("returns em dash for null/invalid/pre-2000 values", () => {
    expect(formatDate(null)).toBe("-");
    expect(formatDate("not a date")).toBe("-");
    expect(formatTime(undefined)).toBe("-");
    expect(formatTimeShort(new Date("1999-01-01"))).toBe("-");
    expect(formatDateTime("garbage")).toBe("-");
  });
  it("accepts string and numeric inputs", () => {
    expect(formatDate("2026-07-02T00:00:00")).toBe("2026.07.02");
    expect(formatDate(d.getTime())).toBe("2026.07.02");
  });
});

describe("timeAgo", () => {
  it("treats epoch/pre-2000 as never", () => {
    expect(timeAgo("1970-01-01T00:00:00Z")).toBe("never");
    expect(timeAgo("garbage")).toBe("never");
  });
  it("reports just now for sub-minute", () => {
    expect(timeAgo(new Date(Date.now() - 5_000).toISOString())).toBe(
      "just now",
    );
  });
  it("reports minutes / hours / days", () => {
    expect(timeAgo(new Date(Date.now() - 5 * 60_000).toISOString())).toBe(
      "5m ago",
    );
    expect(timeAgo(new Date(Date.now() - 3 * 3_600_000).toISOString())).toBe(
      "3h ago",
    );
    expect(timeAgo(new Date(Date.now() - 2 * 86_400_000).toISOString())).toBe(
      "2d ago",
    );
  });
});
