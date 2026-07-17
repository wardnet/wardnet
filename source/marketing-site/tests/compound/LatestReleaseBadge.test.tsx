import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { LatestReleaseBadge } from "@/components/compound/LatestReleaseBadge";

vi.mock("@/generated/release-info", () => ({
  releaseInfo: {
    stable: {
      version: "2026.05.02",
      tag: "v2026.05.02",
      prerelease: false,
      published_at: "2026-05-07T11:41:45Z",
      asset_base_url: "https://example.com/v2026.05.02",
      binary: null,
      notes_url: "https://github.com/wardnet/wardnet/releases/tag/v2026.05.02",
    },
    beta: null,
    generated_at: "2026-05-11T00:00:00Z",
  },
}));

describe("LatestReleaseBadge", () => {
  it("renders a link to the release notes with the version", () => {
    render(<LatestReleaseBadge />);
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute(
      "href",
      "https://github.com/wardnet/wardnet/releases/tag/v2026.05.02",
    );
    expect(link).toHaveTextContent("Latest release");
    expect(link).toHaveTextContent("v2026.05.02");
  });

  it("merges a custom className with the pill classes", () => {
    render(<LatestReleaseBadge className="mt-4" />);
    const link = screen.getByRole("link");
    expect(link.className).toContain("pill");
    expect(link.className).toContain("pill--ok");
    expect(link.className).toContain("mt-4");
  });

  it("renders nothing when no stable release is available", async () => {
    vi.resetModules();
    vi.doMock("@/generated/release-info", () => ({
      releaseInfo: { stable: null, beta: null, generated_at: "" },
    }));
    const { LatestReleaseBadge: Reloaded } =
      await import("@/components/compound/LatestReleaseBadge");
    const { container } = render(<Reloaded />);
    expect(container.firstChild).toBeNull();
  });
});
