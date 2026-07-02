import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DeviceIcon } from "../../src/components/DeviceIcon";

describe("DeviceIcon", () => {
  it("renders an svg for a known device type", () => {
    const { container } = render(<DeviceIcon type="phone" />);
    expect(container.querySelector("svg")).toBeInTheDocument();
  });
  it("renders the inline set-top-box svg", () => {
    const { container } = render(<DeviceIcon type="settop_box" size={32} />);
    const svg = container.querySelector("svg");
    expect(svg).toBeInTheDocument();
    expect(svg).toHaveAttribute("width", "32");
  });
  it("falls back to a help icon for an unknown type", () => {
    // @ts-expect-error deliberately passing an unmapped type
    const { container } = render(<DeviceIcon type="mystery" />);
    expect(container.querySelector("svg")).toBeInTheDocument();
  });
});
