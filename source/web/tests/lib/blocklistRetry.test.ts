import { afterEach, describe, expect, it, vi } from "vitest";
import type { Blocklist } from "@wardnet/js";
import {
  blocklistFailureSummary,
  blocklistRetryAt,
} from "../../src/lib/blocklistRetry";

const LAST_ERROR = "2026-08-01T00:00:00Z";

function blocklist(overrides: Partial<Blocklist> = {}): Blocklist {
  return {
    id: "bl-1",
    profile_id: "p-1",
    name: "HaGeZi",
    url: "https://example.test/list.txt",
    enabled: true,
    entry_count: 0,
    last_updated: null,
    cron_schedule: "0 3 * * *",
    last_error: "download failed",
    last_error_at: LAST_ERROR,
    consecutive_failures: 1,
    created_at: LAST_ERROR,
    updated_at: LAST_ERROR,
    ...overrides,
  };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("blocklistRetryAt", () => {
  it("is null for a healthy list", () => {
    expect(blocklistRetryAt(blocklist({ consecutive_failures: 0 }))).toBeNull();
  });

  it("is null without a recorded failure time", () => {
    expect(blocklistRetryAt(blocklist({ last_error_at: null }))).toBeNull();
  });

  // Mirrors the daemon's ladder: 5m doubling to a 6h cap, measured from the
  // last failure rather than from now.
  it.each([
    [1, 5],
    [2, 10],
    [3, 20],
    [4, 40],
    [5, 80],
    [6, 160],
    [7, 320],
    [8, 360],
    [99, 360],
  ])("waits %i failures -> %i minutes", (failures, minutes) => {
    const at = blocklistRetryAt(
      blocklist({ consecutive_failures: failures }),
    );
    const expected = new Date(Date.parse(LAST_ERROR) + minutes * 60_000);
    expect(at?.toISOString()).toBe(expected.toISOString());
  });
});

describe("blocklistFailureSummary", () => {
  it("is null for a healthy list", () => {
    expect(
      blocklistFailureSummary(blocklist({ consecutive_failures: 0 })),
    ).toBeNull();
  });

  it("reads as a sentence with the count and the wait", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(LAST_ERROR));
    expect(blocklistFailureSummary(blocklist({ consecutive_failures: 7 }))).toBe(
      "failed 7 times, next retry in 5h",
    );
  });

  it("says once rather than 1 times", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(LAST_ERROR));
    expect(blocklistFailureSummary(blocklist())).toBe(
      "failed once, next retry in 5m",
    );
  });

  it("says retrying shortly once the wait has elapsed", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(Date.parse(LAST_ERROR) + 24 * 3_600_000));
    expect(blocklistFailureSummary(blocklist({ consecutive_failures: 3 }))).toBe(
      "failed 3 times, retrying shortly",
    );
  });
});
