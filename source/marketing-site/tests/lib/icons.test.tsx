import { describe, expect, it } from "vitest";
import { resolveIcon } from "@/lib/icons";
import docsContent from "../../content/docs.yml";

interface IconedEntry {
  slug: string;
  icon: string;
}

describe("resolveIcon", () => {
  it("returns a component for a known icon name", () => {
    expect(resolveIcon("download")).toBeDefined();
    expect(resolveIcon("route")).toBeDefined();
    expect(resolveIcon("shield")).toBeDefined();
    expect(resolveIcon("code")).toBeDefined();
  });

  it("returns undefined for an unknown icon name", () => {
    expect(resolveIcon("nonexistent")).toBeUndefined();
  });

  // `Docs` renders nothing at all when `resolveIcon` misses, so an icon named
  // in `docs.yml` that was never added to `ICON_MAP` ships a silently
  // icon-less card. Nothing else cross-checks the two, so pin them here.
  it("resolves every icon named in docs.yml", () => {
    const entries = [
      ...(docsContent.recommended as IconedEntry[]),
      ...(docsContent.topics as IconedEntry[]),
    ];
    const unresolved = entries
      .filter((e) => !resolveIcon(e.icon))
      .map((e) => `${e.slug}: ${e.icon}`);
    expect(unresolved).toEqual([]);
  });
});
