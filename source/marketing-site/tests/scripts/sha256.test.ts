import { describe, expect, it } from "vitest";

import { parseSha256Asset } from "../../scripts/sha256";

describe("parseSha256Asset", () => {
  const VALID = "a".repeat(64);

  it("reads the hash from sha256sum output", () => {
    expect(parseSha256Asset(`${VALID}  openapi.json\n`, "v1")).toBe(VALID);
  });

  it("accepts a bare hash and normalises case", () => {
    expect(parseSha256Asset(`  ${VALID.toUpperCase()}\n`, "v1")).toBe(VALID);
  });

  // The reason this guard exists. The asset body is supplied by whoever
  // published the release, and the value becomes a path component in
  // `resolve(PUBLIC_API_SPECS, `${hash}.json`)`. `resolve` normalises "..", so
  // without the check each of these would write attacker-controlled bytes
  // outside public/api-specs — and because the caller only logs on failure, it
  // would do so silently.
  it.each([
    ["../../../src/generated/release-info", "climbs out of public/api-specs"],
    ["../../../../../../../../tmp/pwned", "escapes the repo entirely"],
    ["/etc/cron.d/evil", "absolute path"],
    ["..%2F..%2Fetc", "encoded traversal"],
    ["subdir/nested", "smuggles a directory separator"],
  ])("rejects a path-traversal payload: %s (%s)", (payload) => {
    expect(() => parseSha256Asset(payload, "v1")).toThrow(/malformed sha256/);
  });

  it.each([
    ["", "empty asset"],
    ["   \n", "whitespace only"],
    ["not-a-hash", "not hex"],
    ["a".repeat(63), "too short"],
    ["a".repeat(65), "too long"],
    ["z".repeat(64), "right length, non-hex characters"],
  ])("rejects malformed input: %s (%s)", (payload) => {
    expect(() => parseSha256Asset(payload, "v1")).toThrow(/malformed sha256/);
  });

  it("does not leak an unbounded attacker string into the error message", () => {
    const huge = "../".repeat(10_000);
    expect(() => parseSha256Asset(huge, "v1")).toThrow(/malformed sha256/);
    try {
      parseSha256Asset(huge, "v1");
    } catch (e) {
      expect((e as Error).message.length).toBeLessThan(120);
    }
  });
});
