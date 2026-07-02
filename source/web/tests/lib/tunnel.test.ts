import { describe, expect, it } from "vitest";
import { tunnelStatusVariant, tunnelStatusLabel } from "../../src/lib/tunnel";

describe("tunnelStatusVariant", () => {
  it("maps each status to its pill variant", () => {
    expect(tunnelStatusVariant("up")).toBe("ok");
    expect(tunnelStatusVariant("connecting")).toBe("info");
    expect(tunnelStatusVariant("reconnecting")).toBe("warn");
    expect(tunnelStatusVariant("down")).toBe("down");
  });
  it("throws on an unhandled status", () => {
    // @ts-expect-error exercising the exhaustiveness guard
    expect(() => tunnelStatusVariant("bogus")).toThrow(
      /Unhandled TunnelStatus/,
    );
  });
});

describe("tunnelStatusLabel", () => {
  it("maps each status to its human label", () => {
    expect(tunnelStatusLabel("up")).toBe("Active");
    expect(tunnelStatusLabel("connecting")).toBe("Connecting");
    expect(tunnelStatusLabel("reconnecting")).toBe("Reconnecting");
    expect(tunnelStatusLabel("down")).toBe("Down");
  });
  it("throws on an unhandled status", () => {
    // @ts-expect-error exercising the exhaustiveness guard
    expect(() => tunnelStatusLabel("bogus")).toThrow(/Unhandled TunnelStatus/);
  });
});
