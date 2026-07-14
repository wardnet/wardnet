import { describe, expect, it } from "vitest";

import { classifyChannels, type ChannelRelease } from "../../scripts/channels";

/** Minimal release fixture — only the fields `classifyChannels` reads. */
function release(tag: string, overrides: Partial<ChannelRelease> = {}): ChannelRelease {
  return {
    tag_name: tag,
    prerelease: tag.includes("-"),
    draft: false,
    ...overrides,
  };
}

describe("classifyChannels", () => {
  it("picks the highest non-prerelease for stable", () => {
    const { stable } = classifyChannels([
      release("v2026.06.00"),
      release("v2026.07.00"),
      release("v2026.05.00"),
    ]);
    expect(stable?.tag_name).toBe("v2026.07.00");
  });

  it("picks the highest release overall for beta", () => {
    const { beta } = classifyChannels([
      release("v2026.07.00"),
      release("v2026.08.00-beta.2"),
      release("v2026.08.00-beta.1"),
    ]);
    expect(beta?.tag_name).toBe("v2026.08.00-beta.2");
  });

  // The landmine. `beta` was defined as "highest release overall", and an edge
  // build sorts above every beta of the same base — so without an explicit
  // exclusion the first edge build would silently become the next update for
  // every box on the beta channel, which is exactly what the channel promises
  // will never happen.
  it("never lets an edge build become the beta channel's release", () => {
    const { beta } = classifyChannels([
      release("v2026.07.00-beta.5"),
      release("edge-v2026.07.00-edge.147"),
    ]);
    expect(beta?.tag_name).toBe("v2026.07.00-beta.5");
  });

  it("never lets an edge build become the stable channel's release", () => {
    // An edge build off a branch whose base CalVer has no pre-release suffix
    // is still a pre-release tag, but belt-and-braces: `prerelease: false` on
    // the GitHub side must not be able to promote it either.
    const { stable } = classifyChannels([
      release("v2026.07.00"),
      release("edge-v2026.08.00-edge.9", { prerelease: false }),
    ]);
    expect(stable?.tag_name).toBe("v2026.07.00");
  });

  it("picks the highest edge build for the edge channel", () => {
    const { edge } = classifyChannels([
      release("v2026.07.00-beta.5"),
      release("edge-v2026.07.00-edge.9"),
      release("edge-v2026.07.00-edge.147"),
      release("edge-v2026.07.00-edge.12"),
    ]);
    // Run numbers are compared numerically, not lexically: 147 > 12 > 9.
    expect(edge?.tag_name).toBe("edge-v2026.07.00-edge.147");
  });

  it("leaves edge empty when no edge build has been published", () => {
    const { edge } = classifyChannels([release("v2026.07.00"), release("v2026.07.01-beta.1")]);
    expect(edge).toBeNull();
  });

  it("skips drafts on every channel", () => {
    const { stable, beta, edge } = classifyChannels([
      release("v2026.07.00"),
      release("v2026.09.00", { draft: true }),
      release("v2026.09.00-beta.1", { draft: true }),
      release("edge-v2026.09.00-edge.1", { draft: true }),
    ]);
    expect(stable?.tag_name).toBe("v2026.07.00");
    expect(beta?.tag_name).toBe("v2026.07.00");
    expect(edge).toBeNull();
  });
});
