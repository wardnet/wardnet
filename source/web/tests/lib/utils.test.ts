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
  sortByLabel,
} from "../../src/lib/utils";

describe("cn", () => {
  it("merges class names and dedupes tailwind conflicts", () => {
    expect(cn("px-2", "px-4")).toBe("px-4");
    const skip: string | false = false;
    expect(cn("a", skip && "b", "c")).toBe("a c");
  });
});

describe("sortByLabel", () => {
  it("orders by the derived label, ignoring case", () => {
    const items = [{ n: "zeta" }, { n: "Alpha" }, { n: "beta" }];
    expect(sortByLabel(items, (i) => i.n).map((i) => i.n)).toEqual([
      "Alpha",
      "beta",
      "zeta",
    ]);
  });

  // Callers pass React Query data straight in; mutating it would reorder the
  // cache under other consumers.
  it("returns a new array and leaves the input untouched", () => {
    const items = [{ n: "b" }, { n: "a" }];
    const sorted = sortByLabel(items, (i) => i.n);
    expect(sorted).not.toBe(items);
    expect(items.map((i) => i.n)).toEqual(["b", "a"]);
  });

  it("sorts naturally across accents rather than by code point", () => {
    const items = [{ n: "Zoe" }, { n: "Ähnlich" }, { n: "apple" }];
    expect(sortByLabel(items, (i) => i.n).map((i) => i.n)).toEqual([
      "Ähnlich",
      "apple",
      "Zoe",
    ]);
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

  // A DHCP client that sends an empty hostname must not produce a blank label
  // that renders as nothing and sorts above every real name.
  it("treats blank and whitespace-only candidates as absent", () => {
    expect(deviceDisplayName({ name: "", hostname: "h", mac: "m" })).toBe("h");
    expect(deviceDisplayName({ name: "   ", hostname: "", mac: "m" })).toBe(
      "m",
    );
  });

  it("uses the fallback ahead of the mac when one is given", () => {
    expect(
      deviceDisplayName({ name: null, hostname: null, mac: "m" }, "10.0.0.5"),
    ).toBe("10.0.0.5");
    expect(
      deviceDisplayName({ name: "TV", hostname: null, mac: "m" }, "10.0.0.5"),
    ).toBe("TV");
  });

  // The dropdown passes `last_ip` as the fallback. It is typed non-null, but a
  // null slipping through must degrade to the MAC rather than yield a
  // non-string that would throw the moment a sort compares it.
  it("falls through a null/blank fallback to the mac", () => {
    expect(
      deviceDisplayName({ name: null, hostname: null, mac: "m" }, null),
    ).toBe("m");
    expect(
      deviceDisplayName({ name: null, hostname: null, mac: "m" }, ""),
    ).toBe("m");
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
  // Math.log is -Infinity / NaN for these; without a guard they produced
  // "NaN undefined" and an out-of-range unit index.
  it("returns 0 B for non-positive input", () => {
    expect(formatBytes(-1)).toBe("0 B");
    expect(formatBytes(-1024)).toBe("0 B");
  });
  // Past the last known unit, clamp to TB instead of indexing off the array
  // and printing "undefined".
  it("clamps values beyond the largest unit to TB", () => {
    expect(formatBytes(1024 ** 4)).toBe("1.0 TB");
    expect(formatBytes(1024 ** 5)).toBe("1024.0 TB");
    expect(formatBytes(5 * 1024 ** 5)).toBe("5120.0 TB");
  });
  // A positive value below 1 byte gives a negative magnitude; without the
  // lower clamp the index went to -1 and printed "undefined".
  it("clamps positive sub-byte input to the B unit", () => {
    expect(formatBytes(0.5)).toBe("1 B");
    expect(formatBytes(0.001)).toBe("0 B");
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
